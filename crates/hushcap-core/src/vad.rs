//! Voice activity detection.
//!
//! An adaptive, energy-based detector tuned for conversational speech:
//!
//! * audio is band-limited to the voice band (≈150 Hz – 4 kHz) so engine
//!   rumble, air conditioning and hiss count for less;
//! * the noise floor is tracked as a low percentile of recent frame energies,
//!   so the detector adapts to a quiet office, a noisy café or a bus;
//! * a frame is speech when it is clearly louder than that floor.
//!
//! It is deliberately cheap so it can run on every audio source continuously.
//! Smoothing (minimum speech length, trailing silence) is done by
//! [`crate::segmenter::Segmenter`].

use std::collections::VecDeque;
use std::f32::consts::PI;

use serde::{Deserialize, Serialize};

use crate::audio::{rms, to_dbfs};
use crate::SAMPLE_RATE;

/// Tunable parameters of the detector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VadConfig {
    /// Analysis frame length in milliseconds.
    pub frame_ms: u32,
    /// How far above the noise floor (dB) a frame must be to count as speech.
    /// Lower is more sensitive.
    pub threshold_db: f32,
    /// Frames quieter than this (dBFS) are never speech, whatever the floor.
    pub min_speech_dbfs: f32,
    /// Length of the history used to estimate the noise floor, in milliseconds.
    pub noise_window_ms: u32,
    /// Percentile (0–1) of the history used as the noise floor.
    pub noise_percentile: f32,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            frame_ms: 20,
            threshold_db: 9.0,
            min_speech_dbfs: -52.0,
            noise_window_ms: 4000,
            noise_percentile: 0.1,
        }
    }
}

impl VadConfig {
    /// Map a user-facing sensitivity (0 = only loud, close speech; 1 = pick
    /// up quiet, distant speech) to detector thresholds.
    pub fn with_sensitivity(mut self, sensitivity: f32) -> Self {
        let s = sensitivity.clamp(0.0, 1.0);
        self.threshold_db = 14.0 - 9.0 * s; // 14 dB … 5 dB
        self.min_speech_dbfs = -42.0 - 18.0 * s; // -42 … -60 dBFS
        self
    }

    /// Number of samples per analysis frame.
    pub fn frame_len(&self) -> usize {
        (SAMPLE_RATE * self.frame_ms.max(5) / 1000) as usize
    }
}

/// Result of analysing one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VadFrame {
    /// Whether the frame looks like speech.
    pub is_speech: bool,
    /// Voice-band energy of the frame in dBFS.
    pub energy_db: f32,
    /// Current noise-floor estimate in dBFS.
    pub noise_floor_db: f32,
}

/// Second-order IIR section (RBJ audio-EQ-cookbook biquad).
#[derive(Debug, Clone, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn high_pass(freq: f32, rate: f32) -> Self {
        let w0 = 2.0 * PI * freq / rate;
        let alpha = w0.sin() / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let cos = w0.cos();
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 + cos) / 2.0 / a0,
            b1: -(1.0 + cos) / a0,
            b2: (1.0 + cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            ..Default::default()
        }
    }

    fn low_pass(freq: f32, rate: f32) -> Self {
        let w0 = 2.0 * PI * freq / rate;
        let alpha = w0.sin() / (2.0 * std::f32::consts::FRAC_1_SQRT_2);
        let cos = w0.cos();
        let a0 = 1.0 + alpha;
        Self {
            b0: (1.0 - cos) / 2.0 / a0,
            b1: (1.0 - cos) / a0,
            b2: (1.0 - cos) / 2.0 / a0,
            a1: -2.0 * cos / a0,
            a2: (1.0 - alpha) / a0,
            ..Default::default()
        }
    }

    #[inline]
    fn process(&mut self, x: f32) -> f32 {
        // Transposed direct form II.
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Adaptive energy-based voice activity detector operating on 16 kHz mono audio.
#[derive(Debug, Clone)]
pub struct Vad {
    config: VadConfig,
    high_pass: Biquad,
    low_pass: Biquad,
    history: VecDeque<f32>,
    history_len: usize,
    scratch: Vec<f32>,
    sorted: Vec<f32>,
}

impl Vad {
    /// Lowest noise floor the detector will assume, in dBFS.
    pub const FLOOR_MIN_DB: f32 = -80.0;

    /// Create a detector.
    pub fn new(config: VadConfig) -> Self {
        let history_len = (config.noise_window_ms / config.frame_ms.max(5)).max(5) as usize;
        Self {
            high_pass: Biquad::high_pass(150.0, SAMPLE_RATE as f32),
            low_pass: Biquad::low_pass(4000.0, SAMPLE_RATE as f32),
            history: VecDeque::with_capacity(history_len),
            history_len,
            scratch: Vec::new(),
            sorted: Vec::with_capacity(history_len),
            config,
        }
    }

    /// The active configuration.
    pub fn config(&self) -> &VadConfig {
        &self.config
    }

    /// Current noise floor estimate in dBFS.
    pub fn noise_floor_db(&mut self) -> f32 {
        if self.history.is_empty() {
            return Self::FLOOR_MIN_DB;
        }
        self.sorted.clear();
        self.sorted.extend(self.history.iter().copied());
        self.sorted.sort_by(|a, b| a.total_cmp(b));
        let idx = ((self.sorted.len() - 1) as f32 * self.config.noise_percentile).round() as usize;
        self.sorted[idx].max(Self::FLOOR_MIN_DB)
    }

    /// Analyse one frame of audio (normally [`VadConfig::frame_len`] samples).
    pub fn process_frame(&mut self, frame: &[f32]) -> VadFrame {
        self.scratch.clear();
        for &x in frame {
            let y = self.low_pass.process(self.high_pass.process(x));
            self.scratch.push(y);
        }
        let energy_db = to_dbfs(rms(&self.scratch));
        let noise_floor_db = self.noise_floor_db();
        let is_speech = energy_db >= self.config.min_speech_dbfs
            && energy_db >= noise_floor_db + self.config.threshold_db;

        if self.history.len() == self.history_len {
            self.history.pop_front();
        }
        self.history.push_back(energy_db);

        VadFrame {
            is_speech,
            energy_db,
            noise_floor_db,
        }
    }

    /// Forget the noise history and filter state.
    pub fn reset(&mut self) {
        *self = Self::new(self.config.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{noise, silence, sine, speech_like};

    fn speech_fraction(vad: &mut Vad, audio: &[f32]) -> f32 {
        let n = vad.config().frame_len();
        let frames: Vec<_> = audio
            .chunks_exact(n)
            .map(|f| vad.process_frame(f))
            .collect();
        frames.iter().filter(|f| f.is_speech).count() as f32 / frames.len() as f32
    }

    #[test]
    fn silence_is_not_speech() {
        let mut vad = Vad::new(VadConfig::default());
        assert_eq!(speech_fraction(&mut vad, &silence(2000)), 0.0);
    }

    #[test]
    fn speech_after_quiet_room_is_detected() {
        let mut vad = Vad::new(VadConfig::default());
        speech_fraction(&mut vad, &noise(0.002, 2000, 1));
        let mut voice = speech_like(1500, 140.0);
        for (v, n) in voice.iter_mut().zip(noise(0.002, 1500, 2)) {
            *v += n;
        }
        let frac = speech_fraction(&mut vad, &voice);
        assert!(frac > 0.7, "speech fraction {frac}");
    }

    #[test]
    fn steady_noise_is_learned_as_background() {
        let mut vad = Vad::new(VadConfig::default());
        let loud_noise = noise(0.05, 6000, 3);
        let n = vad.config().frame_len();
        let frames: Vec<_> = loud_noise
            .chunks_exact(n)
            .map(|f| vad.process_frame(f))
            .collect();
        // After the floor has adapted, stationary noise is not speech.
        let tail = &frames[frames.len() - 100..];
        let speech = tail.iter().filter(|f| f.is_speech).count();
        assert!(speech < 5, "{speech} noise frames flagged as speech");
    }

    #[test]
    fn low_frequency_rumble_is_attenuated() {
        let mut vad = Vad::new(VadConfig::default());
        speech_fraction(&mut vad, &silence(1000));
        // A 40 Hz hum at a level that would otherwise be "loud".
        let hum = sine(40.0, 0.02, 1000, SAMPLE_RATE);
        let frac = speech_fraction(&mut vad, &hum);
        assert!(frac < 0.1, "rumble fraction {frac}");
    }

    #[test]
    fn sensitivity_moves_thresholds() {
        let low = VadConfig::default().with_sensitivity(0.0);
        let high = VadConfig::default().with_sensitivity(1.0);
        assert!(low.threshold_db > high.threshold_db);
        assert!(low.min_speech_dbfs > high.min_speech_dbfs);
    }
}
