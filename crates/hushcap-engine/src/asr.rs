//! Speech recognition.
//!
//! [`Recognizer`] is the interface the pipeline uses; [`WhisperRecognizer`]
//! implements it with whisper.cpp (via `whisper-rs`), entirely on-device.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Whether a transcription is an interim preview or the final text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Fast, approximate pass on an utterance still in progress.
    Partial,
    /// Careful pass on a finished utterance.
    Final,
}

/// Speed/accuracy trade-off for final transcriptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Accuracy {
    /// Greedy decoding; lowest latency.
    Fast,
    /// Greedy decoding with temperature fallback when unsure.
    #[default]
    Balanced,
    /// Beam search; best accuracy, highest latency.
    Accurate,
}

/// Result of transcribing a chunk of audio.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Transcript {
    /// Raw recognised text (not yet cleaned).
    pub text: String,
    /// The recogniser's estimate that the audio contained no speech (0–1).
    pub no_speech_prob: f32,
    /// Detected or configured language code.
    pub language: Option<String>,
}

/// A speech-to-text engine.
pub trait Recognizer: Send {
    /// Transcribe 16 kHz mono audio. `prompt` is recent context (previous
    /// sentences) that helps with names and punctuation.
    fn transcribe(&mut self, audio: &[f32], mode: Mode, prompt: &str) -> Result<Transcript>;

    /// Human-readable description of the loaded model.
    fn describe(&self) -> String;
}

/// Options for [`WhisperRecognizer`].
#[derive(Debug, Clone, PartialEq)]
pub struct WhisperOptions {
    /// Language code (`"en"`, `"es"`, …) or `None` to detect automatically.
    pub language: Option<String>,
    /// CPU threads (0 = choose automatically).
    pub threads: usize,
    /// Accuracy preset for final transcriptions.
    pub accuracy: Accuracy,
    /// Translate speech into English instead of transcribing it.
    pub translate: bool,
    /// Shrink the encoder window to the audio length. Much faster for short
    /// utterances at a small cost in accuracy.
    pub dynamic_audio_ctx: bool,
}

impl Default for WhisperOptions {
    fn default() -> Self {
        Self {
            language: None,
            threads: 0,
            accuracy: Accuracy::Balanced,
            translate: false,
            dynamic_audio_ctx: true,
        }
    }
}

/// Number of threads to use when the caller asks for "automatic".
pub fn auto_threads() -> usize {
    let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
    // Leave a core for audio capture and the UI.
    cores.saturating_sub(1).clamp(1, 8)
}

/// Encoder context (in 20 ms mel frames, max 1500 = 30 s) needed for `samples`
/// of 16 kHz audio, with headroom, rounded up to a multiple of 64.
pub fn audio_ctx_for(samples: usize) -> i32 {
    let frames = samples as f64 / 16_000.0 * 50.0;
    let wanted = (frames + 128.0).max(448.0);
    let rounded = (wanted / 64.0).ceil() * 64.0;
    rounded.min(1500.0) as i32
}

#[cfg(feature = "whisper")]
pub use whisper_impl::WhisperRecognizer;

#[cfg(feature = "whisper")]
mod whisper_impl {
    use super::*;
    use anyhow::{anyhow, Context};
    use std::path::Path;
    use whisper_rs::{
        FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
    };

    /// whisper.cpp speech recogniser.
    pub struct WhisperRecognizer {
        // `state` borrows nothing from `ctx` (it holds its own reference),
        // but keep the context alive alongside it for clarity.
        _ctx: WhisperContext,
        state: WhisperState,
        options: WhisperOptions,
        multilingual: bool,
        description: String,
    }

    impl WhisperRecognizer {
        /// Load a GGML model file.
        pub fn load(model: &Path, options: WhisperOptions) -> Result<Self> {
            whisper_rs::install_logging_hooks();
            crate::storage::validate_model_file(model, hushcap_core::models::ModelKind::Speech)?;
            let mut params = WhisperContextParameters::default();
            params.use_gpu(false);
            let ctx = WhisperContext::new_with_params(model, params)
                .map_err(|e| anyhow!("could not load speech model {}: {e}", model.display()))?;
            let multilingual = ctx.is_multilingual();
            let kind = ctx
                .model_type_readable_str_lossy()
                .map(|s| s.into_owned())
                .unwrap_or_else(|_| "unknown".into());
            let state = ctx
                .create_state()
                .map_err(|e| anyhow!("could not initialise speech model: {e}"))?;
            let name = model
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let description = format!(
                "whisper.cpp {} · {name} ({kind}, {})",
                whisper_rs::WHISPER_CPP_VERSION,
                if multilingual {
                    "multilingual"
                } else {
                    "English-only"
                }
            );
            let mut options = options;
            if !multilingual {
                // English-only models must be told the language is English.
                options.language = Some("en".into());
                options.translate = false;
            }
            Ok(Self {
                _ctx: ctx,
                state,
                options,
                multilingual,
                description,
            })
        }

        /// Whether the loaded model supports languages other than English.
        pub fn is_multilingual(&self) -> bool {
            self.multilingual
        }
    }

    impl Recognizer for WhisperRecognizer {
        fn transcribe(&mut self, audio: &[f32], mode: Mode, prompt: &str) -> Result<Transcript> {
            if audio.len() < 1600 {
                return Ok(Transcript::default());
            }
            // whisper.cpp needs at least ~1 s of audio to behave; pad short clips with silence.
            let padded;
            let audio = if audio.len() < 16_000 + 1600 {
                let mut v = audio.to_vec();
                v.resize(16_000 + 1600, 0.0);
                padded = v;
                &padded[..]
            } else {
                audio
            };

            let strategy = match (mode, self.options.accuracy) {
                (Mode::Partial, _) | (Mode::Final, Accuracy::Fast) => {
                    SamplingStrategy::Greedy { best_of: 1 }
                }
                (Mode::Final, Accuracy::Balanced) => SamplingStrategy::Greedy { best_of: 3 },
                (Mode::Final, Accuracy::Accurate) => SamplingStrategy::BeamSearch {
                    beam_size: 4,
                    patience: -1.0,
                },
            };
            let mut params = FullParams::new(strategy);
            let threads = if self.options.threads == 0 {
                auto_threads()
            } else {
                self.options.threads
            };
            params.set_n_threads(threads as i32);
            let lang = self.options.language.as_deref().unwrap_or("auto");
            params.set_language(Some(lang));
            params.set_translate(self.options.translate);
            params.set_no_context(true);
            params.set_no_timestamps(true);
            params.set_single_segment(mode == Mode::Partial);
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            params.set_suppress_blank(true);
            params.set_suppress_nst(false);
            if mode == Mode::Partial || self.options.accuracy == Accuracy::Fast {
                // No temperature fallback: one pass only.
                params.set_temperature_inc(0.0);
            }
            if self.options.dynamic_audio_ctx {
                params.set_audio_ctx(audio_ctx_for(audio.len()));
            }
            if mode == Mode::Final && !prompt.is_empty() {
                params.set_initial_prompt(prompt);
            }

            self.state
                .full(params, audio)
                .map_err(|e| anyhow!("speech recognition failed: {e}"))?;

            let mut text = String::new();
            let mut no_speech: f32 = 0.0;
            let mut segments = 0;
            for segment in self.state.as_iter() {
                let piece = segment
                    .to_str_lossy()
                    .context("could not read recognised text")?;
                if !text.is_empty() && !piece.starts_with(' ') {
                    text.push(' ');
                }
                text.push_str(&piece);
                no_speech = no_speech.max(segment.no_speech_probability());
                segments += 1;
            }
            if segments == 0 {
                no_speech = 1.0;
            }
            let language = if self.multilingual && lang == "auto" {
                whisper_rs::get_lang_str(self.state.full_lang_id_from_state()).map(str::to_owned)
            } else {
                Some(lang.to_string())
            };
            Ok(Transcript {
                text,
                no_speech_prob: no_speech,
                language,
            })
        }

        fn describe(&self) -> String {
            self.description.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_ctx_scales_with_length() {
        assert_eq!(audio_ctx_for(16_000), 448);
        assert_eq!(audio_ctx_for(16_000 * 10), 640);
        assert_eq!(audio_ctx_for(16_000 * 12), 768);
        assert_eq!(audio_ctx_for(16_000 * 60), 1500);
        for secs in 1..30 {
            let ctx = audio_ctx_for(16_000 * secs);
            assert!(
                ctx as usize >= secs * 50,
                "{secs}s needs {} frames",
                secs * 50
            );
            assert!(ctx % 64 == 0 || ctx == 1500);
        }
    }

    #[test]
    fn auto_threads_is_reasonable() {
        let t = auto_threads();
        assert!((1..=8).contains(&t));
    }
}
