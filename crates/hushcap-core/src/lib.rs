//! # hushcap-core
//!
//! Platform-independent building blocks for Hushcap, a private, offline live
//! captioning app. Nothing in this crate touches the network, the file system
//! or audio hardware: it turns raw audio samples into speech segments, speaker
//! labels and caption files.
//!
//! The pipeline looks like this:
//!
//! ```text
//! device audio ─► audio::downmix ─► resample::Resampler (→ 16 kHz mono)
//!     ─► segmenter::Segmenter (VAD, partial snapshots, final utterances)
//!     ─► speech recognizer (hushcap-engine)
//!     ─► fbank + speaker embedding model (hushcap-engine)
//!     ─► diarization::SpeakerClusterer (who is speaking)
//!     ─► caption::Caption ─► export (SRT, VTT, TXT, Markdown, JSON)
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod audio;
pub mod caption;
pub mod diarization;
pub mod export;
pub mod fbank;
pub mod models;
pub mod resample;
pub mod segmenter;
pub mod text;
pub mod vad;

/// Sample rate (Hz) used by every model in the pipeline.
pub const SAMPLE_RATE: u32 = 16_000;

/// Convert a number of samples at [`SAMPLE_RATE`] to milliseconds.
#[inline]
pub fn samples_to_ms(samples: u64) -> u64 {
    samples * 1000 / SAMPLE_RATE as u64
}

/// Convert milliseconds to a number of samples at [`SAMPLE_RATE`].
#[inline]
pub fn ms_to_samples(ms: u64) -> u64 {
    ms * SAMPLE_RATE as u64 / 1000
}

#[cfg(test)]
pub(crate) mod test_util {
    use crate::SAMPLE_RATE;
    use std::f32::consts::PI;

    /// A sine tone at `freq` Hz with the given amplitude.
    pub fn sine(freq: f32, amp: f32, ms: u32, rate: u32) -> Vec<f32> {
        let n = (rate as u64 * ms as u64 / 1000) as usize;
        (0..n)
            .map(|i| amp * (2.0 * PI * freq * i as f32 / rate as f32).sin())
            .collect()
    }

    /// Deterministic pseudo-random noise in [-amp, amp].
    pub fn noise(amp: f32, ms: u32, seed: u64) -> Vec<f32> {
        let n = (SAMPLE_RATE as u64 * ms as u64 / 1000) as usize;
        let mut state = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (0..n)
            .map(|_| {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                let v = ((state >> 33) as f64 / (1u64 << 31) as f64) as f32; // [0, 1)
                (v * 2.0 - 1.0) * amp
            })
            .collect()
    }

    /// A crude "voiced speech" signal: a harmonic-rich tone with a syllable-rate
    /// amplitude envelope, which sits in the speech band like real voice does.
    pub fn speech_like(ms: u32, f0: f32) -> Vec<f32> {
        let n = (SAMPLE_RATE as u64 * ms as u64 / 1000) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / SAMPLE_RATE as f32;
                let env = 0.55 + 0.45 * (2.0 * PI * 4.0 * t).sin();
                let mut v = 0.0;
                for h in 1..=8 {
                    v += (2.0 * PI * f0 * h as f32 * t).sin() / h as f32;
                }
                0.15 * env * v
            })
            .collect()
    }

    pub fn silence(ms: u32) -> Vec<f32> {
        vec![0.0; (SAMPLE_RATE as u64 * ms as u64 / 1000) as usize]
    }
}
