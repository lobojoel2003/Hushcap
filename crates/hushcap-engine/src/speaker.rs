//! Speaker embeddings with an ONNX model, run by `tract` (pure Rust, so no
//! native inference runtime has to be shipped).
//!
//! Supported models are the WeSpeaker and 3D-Speaker exports published by
//! sherpa-onnx: input `[batch, frames, 80]` Kaldi fbank features, output one
//! embedding vector per utterance. Feature settings are read from the
//! model's metadata so the input matches what the model was trained on.

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};
use hushcap_core::fbank::{subtract_mean, Fbank, FbankConfig, WindowType};
use tract_onnx::prelude::*;

/// Shortest audio worth embedding (0.5 s).
pub const MIN_SAMPLES: usize = 8_000;
/// Longest audio embedded; longer utterances use their middle part (8 s).
pub const MAX_SAMPLES: usize = 16_000 * 8;

/// Computes a fixed-size "voice print" for an utterance.
pub struct SpeakerEmbedder {
    plan: Arc<TypedRunnableModel>,
    fbank: Fbank,
    dim: usize,
    framework: String,
}

impl std::fmt::Debug for SpeakerEmbedder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpeakerEmbedder")
            .field("framework", &self.framework)
            .field("dim", &self.dim)
            .finish()
    }
}

fn meta(model: &InferenceModel, key: &str) -> Option<String> {
    model
        .properties
        .get(&format!("onnx.metadata_props.{key}"))
        .and_then(|t| {
            t.to_plain_array_view::<String>()
                .ok()?
                .iter()
                .next()
                .cloned()
        })
}

impl SpeakerEmbedder {
    /// Load and optimise an ONNX speaker model. Takes a few seconds.
    pub fn load(path: &Path) -> Result<Self> {
        crate::storage::validate_model_file(path, hushcap_core::models::ModelKind::Speaker)?;
        let model = tract_onnx::onnx()
            .model_for_path(path)
            .with_context(|| format!("could not read speaker model {}", path.display()))?;

        let framework = meta(&model, "framework").unwrap_or_default();
        if framework == "nemo" {
            bail!("NeMo speaker models are not supported; use a WeSpeaker or 3D-Speaker model");
        }
        if let Some(rate) = meta(&model, "sample_rate") {
            if rate.trim() != "16000" {
                bail!("speaker model expects {rate} Hz audio; only 16000 Hz models are supported");
            }
        }
        if model.input_outlets()?.len() != 1 {
            bail!("speaker model must have exactly one input");
        }
        // WeSpeaker trains on 16-bit integer scale audio with a Hamming window;
        // 3D-Speaker uses [-1, 1] audio and Kaldi's default Povey window.
        let scale_to_int16 = meta(&model, "normalize_samples").is_some_and(|v| v.trim() == "0");
        let window = if framework == "wespeaker" {
            WindowType::Hamming
        } else {
            WindowType::Povey
        };
        let dim = meta(&model, "output_dim")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0);

        let frames = model.symbols.sym("frames");
        let plan = model
            .with_input_fact(
                0,
                f32::fact([1.to_dim(), frames.into(), 80.to_dim()]).into(),
            )?
            .into_optimized()
            .context("could not optimise speaker model")?
            .into_runnable()
            .context("could not prepare speaker model")?;

        let fbank = Fbank::new(FbankConfig {
            window,
            scale_to_int16,
            ..Default::default()
        });
        let mut embedder = Self {
            plan,
            fbank,
            dim,
            framework,
        };
        // Probe once: confirms the model runs and learns its output size.
        let probe = embedder.embed(&vec![0.01; MIN_SAMPLES])?;
        embedder.dim = probe.len();
        Ok(embedder)
    }

    /// Embedding size.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Model family (`wespeaker`, `3d-speaker`, …) from its metadata.
    pub fn framework(&self) -> &str {
        &self.framework
    }

    /// Compute an embedding for 16 kHz mono audio. Audio shorter than
    /// [`MIN_SAMPLES`] is rejected.
    pub fn embed(&self, audio: &[f32]) -> Result<Vec<f32>> {
        if audio.len() < MIN_SAMPLES {
            bail!("audio too short for a speaker embedding");
        }
        let audio = if audio.len() > MAX_SAMPLES {
            let start = (audio.len() - MAX_SAMPLES) / 2;
            &audio[start..start + MAX_SAMPLES]
        } else {
            audio
        };
        let mut feats = self.fbank.compute(audio);
        let bins = self.fbank.config().num_bins;
        let frames = feats.len() / bins;
        subtract_mean(&mut feats, bins);
        let input = tract_ndarray::Array3::from_shape_vec((1, frames, bins), feats)?;
        let outputs = self
            .plan
            .run(tvec!(Tensor::from(input).into()))
            .map_err(|e| anyhow!("speaker model failed: {e}"))?;
        let view = outputs[0].to_plain_array_view::<f32>()?;
        let v: Vec<f32> = view.iter().copied().collect();
        if v.iter().any(|x| !x.is_finite()) {
            bail!("speaker model produced invalid output");
        }
        Ok(v)
    }
}
