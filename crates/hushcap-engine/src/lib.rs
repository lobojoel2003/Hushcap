//! # hushcap-engine
//!
//! The on-device engine behind Hushcap: audio capture (microphone and system
//! audio), whisper.cpp speech recognition, ONNX speaker embeddings and the
//! live captioning pipeline that ties them together.
//!
//! Audio never leaves the device. The only network code is
//! [`download`], which fetches model files when a user asks for them.

#![warn(missing_docs)]

pub mod asr;
pub mod build_info;
pub mod capture;
pub mod download;
pub mod pipeline;
pub mod speaker;
pub mod storage;
pub mod wav;

pub use hushcap_core as core;
pub use pipeline::{Engine, EngineConfig, EventSink, ProcessingSettings, Session, SourceSettings};
