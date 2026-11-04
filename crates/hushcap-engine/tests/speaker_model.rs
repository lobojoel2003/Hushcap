//! Integration test for the ONNX speaker embedder. Needs a speaker model file:
//!
//! ```sh
//! HUSHCAP_TEST_SPEAKER_MODEL=/path/to/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx \
//!     cargo test -p hushcap-engine --test speaker_model
//! ```
//!
//! Skips (passes) when the variable is not set.

use std::path::PathBuf;

use hushcap_core::diarization::{cosine, ClusterConfig, SpeakerClusterer};
use hushcap_engine::speaker::SpeakerEmbedder;

/// A synthetic "voice": glottal pulse train at `f0` through two formant resonators.
fn voice(f0: f32, formants: (f32, f32), secs: f32, seed: u32) -> Vec<f32> {
    let rate = 16_000.0;
    let n = (secs * rate) as usize;
    let mut out = Vec::with_capacity(n);
    let mut phase = 0.0f32;
    let mut res = [(0.0f32, 0.0f32); 2];
    let coeffs: Vec<(f32, f32)> = [formants.0, formants.1]
        .iter()
        .map(|&f| {
            let r = 0.97f32;
            (
                2.0 * r * (2.0 * std::f32::consts::PI * f / rate).cos(),
                -r * r,
            )
        })
        .collect();
    for i in 0..n {
        let t = i as f32 / rate;
        // Slow pitch drift and syllable-rate amplitude envelope make it speech-like.
        let pitch =
            f0 * (1.0 + 0.06 * (2.0 * std::f32::consts::PI * (0.7 + seed as f32 * 0.1) * t).sin());
        phase += pitch / rate;
        let pulse = if phase >= 1.0 {
            phase -= 1.0;
            1.0
        } else {
            0.0
        };
        let mut x = pulse;
        for (k, (a1, a2)) in coeffs.iter().enumerate() {
            let y = x + a1 * res[k].0 + a2 * res[k].1;
            res[k].1 = res[k].0;
            res[k].0 = y;
            x = y;
        }
        let env = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 3.5 * t).sin().abs();
        out.push(x * env);
    }
    let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    out.iter().map(|v| v / peak * 0.3).collect()
}

#[test]
fn embeddings_separate_different_voices() {
    let Some(path) = std::env::var_os("HUSHCAP_TEST_SPEAKER_MODEL").map(PathBuf::from) else {
        eprintln!("HUSHCAP_TEST_SPEAKER_MODEL not set; skipping");
        return;
    };
    let embedder = SpeakerEmbedder::load(&path).expect("model loads");
    assert!(embedder.dim() >= 128, "dim {}", embedder.dim());

    let low_a = embedder
        .embed(&voice(105.0, (600.0, 1100.0), 3.0, 1))
        .unwrap();
    let low_b = embedder
        .embed(&voice(108.0, (610.0, 1120.0), 3.0, 2))
        .unwrap();
    let high = embedder
        .embed(&voice(230.0, (850.0, 2300.0), 3.0, 3))
        .unwrap();
    let same = cosine(&low_a, &low_b);
    let different = cosine(&low_a, &high);
    eprintln!("same {same:.3} different {different:.3}");
    assert!(
        same > different,
        "same {same} should exceed different {different}"
    );

    let mut clusterer = SpeakerClusterer::new(ClusterConfig::default());
    let a = clusterer.assign(&low_a, 3000).speaker;
    let b = clusterer.assign(&low_b, 3000).speaker;
    assert_eq!(a, b);
    assert!(
        embedder.embed(&[0.0; 100]).is_err(),
        "too-short audio is rejected"
    );
}
