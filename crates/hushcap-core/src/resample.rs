//! Streaming band-limited resampler.
//!
//! Capture devices run at 44.1 kHz, 48 kHz or other rates; every model in the
//! pipeline expects 16 kHz. This is a windowed-sinc interpolator that works on
//! arbitrary (including non-integer) ratios, keeps state between calls so it
//! can be fed with whatever buffer sizes the audio driver delivers, and applies
//! an anti-aliasing low-pass filter when downsampling.

use std::f64::consts::PI;

/// Zero crossings of the sinc kernel on each side of the centre tap. Higher
/// means a steeper filter at the cost of more work per output sample.
const ZERO_CROSSINGS: usize = 12;
/// Kernel table resolution: entries per input sample.
const OVERSAMPLE: usize = 256;
/// Fraction of the output Nyquist frequency kept in the passband.
const PASSBAND: f64 = 0.92;

/// Converts a mono stream from one sample rate to another.
#[derive(Debug, Clone)]
pub struct Resampler {
    in_rate: u32,
    out_rate: u32,
    /// Input samples advanced per output sample.
    step: f64,
    /// Kernel half-width, in input samples.
    half_width: usize,
    /// Kernel values at `x = i / OVERSAMPLE` input samples, for `x` in `[0, half_width]`.
    table: Vec<f32>,
    /// History plus pending input.
    buf: Vec<f32>,
    /// Position of the next output sample, in input-sample units relative to `buf[0]`.
    pos: f64,
}

impl Resampler {
    /// Create a resampler from `in_rate` Hz to `out_rate` Hz.
    ///
    /// # Panics
    /// Panics if either rate is zero.
    pub fn new(in_rate: u32, out_rate: u32) -> Self {
        assert!(in_rate > 0 && out_rate > 0, "sample rates must be non-zero");
        let step = in_rate as f64 / out_rate as f64;
        // Cutoff relative to the input Nyquist frequency.
        let cutoff = (out_rate as f64 / in_rate as f64).min(1.0) * PASSBAND;
        let half_width = (ZERO_CROSSINGS as f64 / cutoff).ceil() as usize;
        let len = half_width * OVERSAMPLE + 1;
        let table = (0..len)
            .map(|i| {
                let x = i as f64 / OVERSAMPLE as f64;
                (cutoff * sinc(cutoff * x) * blackman(x / half_width as f64)) as f32
            })
            .collect();
        Self {
            in_rate,
            out_rate,
            step,
            half_width,
            table,
            // Start with silence as history so the first output sample aligns
            // with the first input sample.
            buf: vec![0.0; half_width],
            pos: half_width as f64,
        }
    }

    /// Input sample rate in Hz.
    pub fn in_rate(&self) -> u32 {
        self.in_rate
    }

    /// Output sample rate in Hz.
    pub fn out_rate(&self) -> u32 {
        self.out_rate
    }

    /// Resample `input`, appending the produced samples to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.in_rate == self.out_rate {
            out.extend_from_slice(input);
            return;
        }
        self.buf.extend_from_slice(input);
        let hw = self.half_width as f64;
        out.reserve((input.len() as f64 / self.step) as usize + 1);
        while self.pos + hw < self.buf.len() as f64 {
            out.push(self.interpolate(self.pos));
            self.pos += self.step;
        }
        // Drop input that no future output sample can reach.
        let keep_from = (self.pos.floor() as usize).saturating_sub(self.half_width);
        if keep_from > 0 {
            self.buf.drain(..keep_from);
            self.pos -= keep_from as f64;
        }
    }

    /// Reset all state, as if newly created.
    pub fn reset(&mut self) {
        *self = Self::new(self.in_rate, self.out_rate);
    }

    fn interpolate(&self, center: f64) -> f32 {
        let first = (center - self.half_width as f64).ceil().max(0.0) as usize;
        let last = ((center + self.half_width as f64).floor() as usize).min(self.buf.len() - 1);
        let mut acc = 0.0f32;
        let mut norm = 0.0f32;
        for k in first..=last {
            let w = self.kernel((center - k as f64).abs());
            acc += self.buf[k] * w;
            norm += w;
        }
        if norm.abs() > 1e-6 {
            acc / norm
        } else {
            0.0
        }
    }

    #[inline]
    fn kernel(&self, x: f64) -> f32 {
        let idx = x * OVERSAMPLE as f64;
        let i = idx.floor() as usize;
        if i + 1 >= self.table.len() {
            return 0.0;
        }
        let frac = (idx - i as f64) as f32;
        self.table[i] * (1.0 - frac) + self.table[i + 1] * frac
    }
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

/// Blackman window over `t` in [0, 1] (0 = centre, 1 = edge).
fn blackman(t: f64) -> f64 {
    if t >= 1.0 {
        return 0.0;
    }
    // Map [0, 1] (centre → edge) onto the right half of a symmetric window.
    let n = 0.5 + 0.5 * t;
    0.42 - 0.5 * (2.0 * PI * n).cos() + 0.08 * (4.0 * PI * n).cos()
}

/// One-shot convenience: resample a whole buffer.
pub fn resample_all(input: &[f32], in_rate: u32, out_rate: u32) -> Vec<f32> {
    let mut r = Resampler::new(in_rate, out_rate);
    let mut out = Vec::with_capacity(input.len() * out_rate as usize / in_rate as usize + 1);
    r.process(input, &mut out);
    // Flush the filter tail with silence so the output length matches the input duration.
    let tail = vec![0.0; r.half_width + 1];
    r.process(&tail, &mut out);
    let expected = (input.len() as u64 * out_rate as u64 / in_rate as u64) as usize;
    out.truncate(expected);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::rms;
    use crate::test_util::sine;

    fn tone_amplitude(signal: &[f32], freq: f32, rate: u32) -> f32 {
        // Single-bin DFT magnitude, normalised to a sine's amplitude.
        let (mut re, mut im) = (0.0f64, 0.0f64);
        for (i, &s) in signal.iter().enumerate() {
            let ph = 2.0 * PI * freq as f64 * i as f64 / rate as f64;
            re += s as f64 * ph.cos();
            im += s as f64 * ph.sin();
        }
        (2.0 * (re * re + im * im).sqrt() / signal.len() as f64) as f32
    }

    #[test]
    fn passthrough_when_rates_match() {
        let mut r = Resampler::new(16_000, 16_000);
        let mut out = Vec::new();
        r.process(&[0.1, 0.2, 0.3], &mut out);
        assert_eq!(out, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn output_length_tracks_ratio() {
        for &(from, to) in &[(48_000u32, 16_000u32), (44_100, 16_000), (8_000, 16_000)] {
            let input = sine(440.0, 0.5, 1000, from);
            let out = resample_all(&input, from, to);
            assert_eq!(out.len(), to as usize, "{from} -> {to}");
        }
    }

    #[test]
    fn preserves_in_band_tone() {
        for &from in &[48_000u32, 44_100, 22_050] {
            let input = sine(1000.0, 0.5, 1000, from);
            let out = resample_all(&input, from, 16_000);
            // Skip the filter warm-up at both ends.
            let mid = &out[2000..14000];
            let amp = tone_amplitude(mid, 1000.0, 16_000);
            assert!((amp - 0.5).abs() < 0.02, "{from}: amplitude {amp}");
        }
    }

    #[test]
    fn rejects_frequencies_above_new_nyquist() {
        // 12 kHz cannot be represented at 16 kHz and must not alias down to 4 kHz.
        let input = sine(12_000.0, 0.5, 1000, 48_000);
        let out = resample_all(&input, 48_000, 16_000);
        let level = rms(&out[2000..14000]);
        assert!(level < 0.01, "aliasing leaked through: rms {level}");
    }

    #[test]
    fn chunked_processing_matches_one_shot() {
        let input = sine(700.0, 0.4, 500, 44_100);
        let one_shot = {
            let mut r = Resampler::new(44_100, 16_000);
            let mut out = Vec::new();
            r.process(&input, &mut out);
            out
        };
        let chunked = {
            let mut r = Resampler::new(44_100, 16_000);
            let mut out = Vec::new();
            for chunk in input.chunks(137) {
                r.process(chunk, &mut out);
            }
            out
        };
        assert_eq!(one_shot.len(), chunked.len());
        for (a, b) in one_shot.iter().zip(&chunked) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    #[test]
    fn upsampling_preserves_tone() {
        let input = sine(500.0, 0.5, 1000, 8_000);
        let out = resample_all(&input, 8_000, 16_000);
        let amp = tone_amplitude(&out[2000..14000], 500.0, 16_000);
        assert!((amp - 0.5).abs() < 0.02, "amplitude {amp}");
    }
}
