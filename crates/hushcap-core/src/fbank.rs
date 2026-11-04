//! Kaldi-compatible log mel filterbank ("fbank") features.
//!
//! Speaker-embedding models from WeSpeaker and 3D-Speaker are trained on
//! `torchaudio.compliance.kaldi.fbank` features. This module reproduces that
//! computation (framing, DC removal, pre-emphasis, windowing, power spectrum,
//! triangular mel filters on the Kaldi mel scale, natural log) so the models
//! see exactly the input they expect.

use std::f32::consts::PI;
use std::sync::Arc;

use rustfft::num_complex::Complex32;
use rustfft::{Fft, FftPlanner};
use serde::{Deserialize, Serialize};

/// Analysis window applied to each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WindowType {
    /// Kaldi's default "povey" window (Hann raised to the power 0.85).
    Povey,
    /// Hamming window.
    Hamming,
    /// Hann window.
    Hann,
}

/// Feature extraction parameters (defaults match Kaldi / torchaudio).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FbankConfig {
    /// Input sample rate in Hz.
    pub sample_rate: u32,
    /// Number of mel bins.
    pub num_bins: usize,
    /// Frame length in milliseconds.
    pub frame_length_ms: f32,
    /// Frame shift in milliseconds.
    pub frame_shift_ms: f32,
    /// Lowest filter frequency in Hz.
    pub low_freq: f32,
    /// Highest filter frequency in Hz; `<= 0` is relative to Nyquist.
    pub high_freq: f32,
    /// Pre-emphasis coefficient.
    pub preemphasis: f32,
    /// Subtract the mean of each frame before processing.
    pub remove_dc_offset: bool,
    /// Analysis window.
    pub window: WindowType,
    /// Multiply samples by 32768 first (models trained on 16-bit integer audio).
    pub scale_to_int16: bool,
}

impl Default for FbankConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16_000,
            num_bins: 80,
            frame_length_ms: 25.0,
            frame_shift_ms: 10.0,
            low_freq: 20.0,
            high_freq: 0.0,
            preemphasis: 0.97,
            remove_dc_offset: true,
            window: WindowType::Povey,
            scale_to_int16: false,
        }
    }
}

/// Kaldi mel scale.
pub fn hz_to_mel(hz: f32) -> f32 {
    1127.0 * (1.0 + hz / 700.0).ln()
}

/// Precomputed filterbank extractor.
pub struct Fbank {
    config: FbankConfig,
    frame_len: usize,
    frame_shift: usize,
    fft_len: usize,
    window: Vec<f32>,
    /// For each mel bin: first FFT bin and its weights.
    filters: Vec<(usize, Vec<f32>)>,
    fft: Arc<dyn Fft<f32>>,
}

impl std::fmt::Debug for Fbank {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fbank")
            .field("config", &self.config)
            .finish()
    }
}

impl Fbank {
    /// Build an extractor.
    pub fn new(config: FbankConfig) -> Self {
        let sr = config.sample_rate as f32;
        let frame_len = (sr * config.frame_length_ms / 1000.0) as usize;
        let frame_shift = (sr * config.frame_shift_ms / 1000.0) as usize;
        let fft_len = frame_len.next_power_of_two();

        let window: Vec<f32> = (0..frame_len)
            .map(|i| {
                let a = 2.0 * PI * i as f32 / (frame_len as f32 - 1.0);
                match config.window {
                    WindowType::Hann => 0.5 - 0.5 * a.cos(),
                    WindowType::Hamming => 0.54 - 0.46 * a.cos(),
                    WindowType::Povey => (0.5 - 0.5 * a.cos()).powf(0.85),
                }
            })
            .collect();

        let nyquist = sr / 2.0;
        let high = if config.high_freq <= 0.0 {
            nyquist + config.high_freq
        } else {
            config.high_freq
        };
        let num_fft_bins = fft_len / 2;
        let bin_width = sr / fft_len as f32;
        let mel_low = hz_to_mel(config.low_freq);
        let mel_high = hz_to_mel(high);
        let delta = (mel_high - mel_low) / (config.num_bins as f32 + 1.0);
        let filters = (0..config.num_bins)
            .map(|b| {
                let left = mel_low + b as f32 * delta;
                let center = left + delta;
                let right = center + delta;
                let mut first = None;
                let mut weights = Vec::new();
                for i in 0..num_fft_bins {
                    let mel = hz_to_mel(bin_width * i as f32);
                    if mel > left && mel < right {
                        let w = if mel <= center {
                            (mel - left) / (center - left)
                        } else {
                            (right - mel) / (right - center)
                        };
                        first.get_or_insert(i);
                        weights.push(w);
                    } else if first.is_some() {
                        break;
                    }
                }
                (first.unwrap_or(0), weights)
            })
            .collect();

        let fft = FftPlanner::new().plan_fft_forward(fft_len);
        Self {
            config,
            frame_len,
            frame_shift,
            fft_len,
            window,
            filters,
            fft,
        }
    }

    /// The active configuration.
    pub fn config(&self) -> &FbankConfig {
        &self.config
    }

    /// Number of frames produced for `num_samples` input samples.
    pub fn num_frames(&self, num_samples: usize) -> usize {
        if num_samples < self.frame_len {
            0
        } else {
            1 + (num_samples - self.frame_len) / self.frame_shift
        }
    }

    /// Compute features. Returns a row-major `[frames × num_bins]` matrix.
    pub fn compute(&self, samples: &[f32]) -> Vec<f32> {
        let frames = self.num_frames(samples.len());
        let bins = self.config.num_bins;
        let scale = if self.config.scale_to_int16 {
            32768.0
        } else {
            1.0
        };
        let mut out = Vec::with_capacity(frames * bins);
        let mut frame = vec![0.0f32; self.frame_len];
        let mut spectrum = vec![Complex32::new(0.0, 0.0); self.fft_len];
        let mut power = vec![0.0f32; self.fft_len / 2 + 1];
        let mut scratch = vec![Complex32::new(0.0, 0.0); self.fft.get_inplace_scratch_len()];

        for f in 0..frames {
            let start = f * self.frame_shift;
            for (dst, &src) in frame
                .iter_mut()
                .zip(&samples[start..start + self.frame_len])
            {
                *dst = src * scale;
            }
            if self.config.remove_dc_offset {
                let mean = frame.iter().sum::<f32>() / frame.len() as f32;
                frame.iter_mut().for_each(|x| *x -= mean);
            }
            let p = self.config.preemphasis;
            if p != 0.0 {
                for i in (1..frame.len()).rev() {
                    frame[i] -= p * frame[i - 1];
                }
                frame[0] -= p * frame[0];
            }
            for (i, c) in spectrum.iter_mut().enumerate() {
                let v = if i < self.frame_len {
                    frame[i] * self.window[i]
                } else {
                    0.0
                };
                *c = Complex32::new(v, 0.0);
            }
            self.fft.process_with_scratch(&mut spectrum, &mut scratch);
            for (p, c) in power.iter_mut().zip(&spectrum) {
                *p = c.norm_sqr();
            }
            for (first, weights) in &self.filters {
                let energy: f32 = weights
                    .iter()
                    .zip(&power[*first..])
                    .map(|(w, p)| w * p)
                    .sum();
                out.push(energy.max(f32::EPSILON).ln());
            }
        }
        out
    }
}

/// Subtract the per-dimension mean over all frames ("cepstral mean
/// normalisation"), in place.
pub fn subtract_mean(features: &mut [f32], num_bins: usize) {
    if num_bins == 0 || features.is_empty() {
        return;
    }
    let frames = features.len() / num_bins;
    let mut mean = vec![0.0f64; num_bins];
    for row in features.chunks_exact(num_bins) {
        for (m, &v) in mean.iter_mut().zip(row) {
            *m += v as f64;
        }
    }
    for m in &mut mean {
        *m /= frames as f64;
    }
    for row in features.chunks_exact_mut(num_bins) {
        for (v, &m) in row.iter_mut().zip(&mean) {
            *v -= m as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::sine;

    #[test]
    fn frame_count_matches_kaldi_snip_edges() {
        let fb = Fbank::new(FbankConfig::default());
        assert_eq!(fb.num_frames(399), 0);
        assert_eq!(fb.num_frames(400), 1);
        assert_eq!(fb.num_frames(16_000), 98);
        let feats = fb.compute(&vec![0.0; 16_000]);
        assert_eq!(feats.len(), 98 * 80);
    }

    #[test]
    fn silence_hits_energy_floor() {
        let fb = Fbank::new(FbankConfig::default());
        let feats = fb.compute(&vec![0.0; 1600]);
        for v in feats {
            assert!((v - f32::EPSILON.ln()).abs() < 1e-3);
        }
    }

    #[test]
    fn tone_energy_lands_in_matching_mel_bin() {
        let fb = Fbank::new(FbankConfig::default());
        let tone = sine(1000.0, 0.5, 200, 16_000);
        let feats = fb.compute(&tone);
        let row = &feats[5 * 80..6 * 80];
        let peak_bin = row
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .unwrap()
            .0;
        // Find which filter is centred closest to 1 kHz.
        let mel_low = hz_to_mel(20.0);
        let delta = (hz_to_mel(8000.0) - mel_low) / 81.0;
        let expected = ((hz_to_mel(1000.0) - mel_low) / delta - 1.0).round() as usize;
        assert!(
            (peak_bin as i64 - expected as i64).abs() <= 1,
            "peak {peak_bin}, expected {expected}"
        );
    }

    #[test]
    fn int16_scaling_shifts_log_energy() {
        let tone = sine(440.0, 0.3, 100, 16_000);
        let a = Fbank::new(FbankConfig::default()).compute(&tone);
        let b = Fbank::new(FbankConfig {
            scale_to_int16: true,
            ..Default::default()
        })
        .compute(&tone);
        // Scaling by 32768 adds ln(32768²) to log power wherever it is above the floor.
        let shift = 2.0 * 32768f32.ln();
        let i = a
            .iter()
            .enumerate()
            .max_by(|x, y| x.1.total_cmp(y.1))
            .unwrap()
            .0;
        assert!((b[i] - a[i] - shift).abs() < 1e-2);
    }

    #[test]
    fn mean_subtraction_zeroes_columns() {
        let mut feats = vec![1.0, 10.0, 3.0, 20.0, 5.0, 30.0];
        subtract_mean(&mut feats, 2);
        assert_eq!(feats, vec![-2.0, -10.0, 0.0, 0.0, 2.0, 10.0]);
    }

    #[test]
    fn filters_cover_spectrum_without_gaps() {
        let fb = Fbank::new(FbankConfig::default());
        assert_eq!(fb.filters.len(), 80);
        for (first, w) in &fb.filters {
            assert!(!w.is_empty());
            assert!(*first + w.len() <= 256);
            assert!(w.iter().all(|&x| (0.0..=1.0).contains(&x)));
        }
    }
}
