//! Caption data types shared by the engine, the CLI and the desktop UI.

use serde::{Deserialize, Serialize};

/// Where audio comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioSource {
    /// A microphone (the room you are in).
    Microphone,
    /// Sound playing on this device: a video call, a video, a podcast.
    System,
}

impl AudioSource {
    /// Short human-readable name.
    pub fn label(self) -> &'static str {
        match self {
            AudioSource::Microphone => "Microphone",
            AudioSource::System => "System audio",
        }
    }
}

impl std::fmt::Display for AudioSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A finished caption line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Caption {
    /// Unique, increasing identifier within a session.
    pub id: u64,
    /// Which input the words came from.
    pub source: AudioSource,
    /// Speaker number (1-based) when speaker labelling is on and confident.
    pub speaker: Option<u32>,
    /// Recognised text.
    pub text: String,
    /// Start time from the beginning of the session, in milliseconds.
    pub start_ms: u64,
    /// End time from the beginning of the session, in milliseconds.
    pub end_ms: u64,
    /// Detected or configured language code (e.g. `"en"`), if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

/// Overall engine state, for status displays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EngineState {
    /// Not capturing.
    Stopped,
    /// Loading models and opening devices.
    Starting,
    /// Capturing and captioning.
    Running,
    /// Shutting down.
    Stopping,
}

/// Everything the engine reports to its host (UI or CLI).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EngineEvent {
    /// Engine state changed.
    #[serde(rename_all = "camelCase")]
    State {
        /// New state.
        state: EngineState,
    },
    /// Someone started talking on a source.
    #[serde(rename_all = "camelCase")]
    SpeechStarted {
        /// Caption id that will be used for this utterance.
        id: u64,
        /// Source.
        source: AudioSource,
        /// Start time in milliseconds.
        start_ms: u64,
    },
    /// Interim text for an utterance in progress. Replaces earlier partials
    /// with the same `id`.
    #[serde(rename_all = "camelCase")]
    Partial {
        /// Caption id.
        id: u64,
        /// Source.
        source: AudioSource,
        /// Interim text.
        text: String,
        /// Start time in milliseconds.
        start_ms: u64,
    },
    /// A finished caption. Replaces any partial with the same `id`.
    Final {
        /// The caption.
        caption: Caption,
    },
    /// An utterance in progress turned out to be noise; drop its partial.
    #[serde(rename_all = "camelCase")]
    Dropped {
        /// Caption id.
        id: u64,
    },
    /// Two speaker labels were found to be the same person: relabel every
    /// caption with speaker `from` as `into`.
    #[serde(rename_all = "camelCase")]
    SpeakersMerged {
        /// Label being retired.
        from: u32,
        /// Label it joins.
        into: u32,
    },
    /// Input level reading, about ten per second per source.
    #[serde(rename_all = "camelCase")]
    Level {
        /// Source.
        source: AudioSource,
        /// RMS level in dBFS.
        db: f32,
        /// Whether speech is currently detected.
        speaking: bool,
    },
    /// Recognition performance, so people can tell if a model is too slow.
    #[serde(rename_all = "camelCase")]
    Stats {
        /// Processing time divided by audio duration for the last utterance
        /// (below 1.0 keeps up with real time).
        realtime_factor: f32,
        /// Utterances waiting to be transcribed.
        queued: usize,
    },
    /// A non-fatal problem worth showing to the user.
    #[serde(rename_all = "camelCase")]
    Warning {
        /// Human-readable message.
        message: String,
    },
    /// A fatal problem; the engine has stopped.
    #[serde(rename_all = "camelCase")]
    Error {
        /// Human-readable message.
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_serialize_with_type_tag() {
        let e = EngineEvent::Partial {
            id: 3,
            source: AudioSource::System,
            text: "hello".into(),
            start_ms: 1200,
        };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["type"], "partial");
        assert_eq!(json["source"], "system");
        assert_eq!(json["startMs"], 1200);

        let f = EngineEvent::Final {
            caption: Caption {
                id: 1,
                source: AudioSource::Microphone,
                speaker: Some(2),
                text: "hi".into(),
                start_ms: 0,
                end_ms: 900,
                language: None,
            },
        };
        let json = serde_json::to_value(&f).unwrap();
        assert_eq!(json["type"], "final");
        assert_eq!(json["caption"]["endMs"], 900);
        assert!(json["caption"].get("language").is_none());
        let back: EngineEvent = serde_json::from_value(json).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn merged_event_shape() {
        let json =
            serde_json::to_string(&EngineEvent::SpeakersMerged { from: 3, into: 1 }).unwrap();
        assert_eq!(json, r#"{"type":"speakersMerged","from":3,"into":1}"#);
    }
}
