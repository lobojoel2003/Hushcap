//! Transcript export: SubRip (SRT), WebVTT, plain text, Markdown and JSON.

use std::collections::HashMap;
use std::fmt::Write as _;

use serde::{Deserialize, Serialize};

use crate::caption::Caption;

/// Supported transcript formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    /// Plain text, one line per caption.
    Txt,
    /// SubRip subtitles.
    Srt,
    /// WebVTT subtitles (with `<v Speaker>` voice tags).
    Vtt,
    /// Markdown transcript.
    Markdown,
    /// Machine-readable JSON.
    Json,
}

impl ExportFormat {
    /// Conventional file extension (without the dot).
    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Txt => "txt",
            ExportFormat::Srt => "srt",
            ExportFormat::Vtt => "vtt",
            ExportFormat::Markdown => "md",
            ExportFormat::Json => "json",
        }
    }

    /// Guess the format from a file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "txt" | "text" => Some(Self::Txt),
            "srt" => Some(Self::Srt),
            "vtt" | "webvtt" => Some(Self::Vtt),
            "md" | "markdown" => Some(Self::Markdown),
            "json" => Some(Self::Json),
            _ => None,
        }
    }
}

impl std::str::FromStr for ExportFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_extension(s).ok_or_else(|| format!("unknown transcript format: {s}"))
    }
}

/// What to include in an export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExportOptions {
    /// Prefix lines with speaker names.
    pub include_speakers: bool,
    /// Include timestamps in text and Markdown exports.
    pub include_timestamps: bool,
    /// Title for Markdown exports.
    pub title: Option<String>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_speakers: true,
            include_timestamps: true,
            title: None,
        }
    }
}

/// Display name for a speaker: a custom name if one was set, else "Speaker N".
pub fn speaker_name(speaker: u32, names: &HashMap<u32, String>) -> String {
    names
        .get(&speaker)
        .map(|n| n.trim())
        .filter(|n| !n.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Speaker {speaker}"))
}

/// Format milliseconds as `HH:MM:SS<sep>mmm`.
pub fn timestamp(ms: u64, separator: char) -> String {
    let h = ms / 3_600_000;
    let m = ms / 60_000 % 60;
    let s = ms / 1000 % 60;
    let frac = ms % 1000;
    format!("{h:02}:{m:02}:{s:02}{separator}{frac:03}")
}

/// Format milliseconds as a compact clock, `M:SS` or `H:MM:SS`.
pub fn clock(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = ms / 60_000 % 60;
    let s = ms / 1000 % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Render captions in the requested format.
pub fn export(
    captions: &[Caption],
    names: &HashMap<u32, String>,
    format: ExportFormat,
    options: &ExportOptions,
) -> String {
    let who = |c: &Caption| -> Option<String> {
        if options.include_speakers {
            c.speaker.map(|s| speaker_name(s, names))
        } else {
            None
        }
    };
    let mut out = String::new();
    match format {
        ExportFormat::Txt => {
            for c in captions {
                if options.include_timestamps {
                    let _ = write!(out, "[{}] ", clock(c.start_ms));
                }
                if let Some(name) = who(c) {
                    let _ = write!(out, "{name}: ");
                }
                let _ = writeln!(out, "{}", c.text);
            }
        }
        ExportFormat::Srt => {
            for (i, c) in captions.iter().enumerate() {
                let _ = writeln!(out, "{}", i + 1);
                let _ = writeln!(
                    out,
                    "{} --> {}",
                    timestamp(c.start_ms, ','),
                    timestamp(c.end_ms.max(c.start_ms + 1), ',')
                );
                match who(c) {
                    Some(name) => {
                        let _ = writeln!(out, "{name}: {}", c.text);
                    }
                    None => {
                        let _ = writeln!(out, "{}", c.text);
                    }
                }
                out.push('\n');
            }
        }
        ExportFormat::Vtt => {
            out.push_str("WEBVTT\n\n");
            for c in captions {
                let _ = writeln!(
                    out,
                    "{} --> {}",
                    timestamp(c.start_ms, '.'),
                    timestamp(c.end_ms.max(c.start_ms + 1), '.')
                );
                let text = vtt_escape(&c.text);
                match who(c) {
                    Some(name) => {
                        let _ = writeln!(out, "<v {}>{text}", vtt_escape(&name));
                    }
                    None => {
                        let _ = writeln!(out, "{text}");
                    }
                }
                out.push('\n');
            }
        }
        ExportFormat::Markdown => {
            let title = options.title.as_deref().unwrap_or("Transcript");
            let _ = writeln!(out, "# {title}\n");
            for c in captions {
                let mut line = String::new();
                if let Some(name) = who(c) {
                    let _ = write!(line, "**{name}**");
                }
                if options.include_timestamps {
                    if !line.is_empty() {
                        line.push(' ');
                    }
                    let _ = write!(line, "_{}_", clock(c.start_ms));
                }
                if line.is_empty() {
                    let _ = writeln!(out, "{}\n", c.text);
                } else {
                    let _ = writeln!(out, "{line}: {}\n", c.text);
                }
            }
        }
        ExportFormat::Json => {
            let items: Vec<_> = captions
                .iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "source": c.source,
                        "speaker": c.speaker,
                        "speakerName": c.speaker.map(|s| speaker_name(s, names)),
                        "text": c.text,
                        "startMs": c.start_ms,
                        "endMs": c.end_ms,
                        "language": c.language,
                    })
                })
                .collect();
            out = serde_json::to_string_pretty(&serde_json::json!({ "captions": items }))
                .unwrap_or_default();
            out.push('\n');
        }
    }
    out
}

fn vtt_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        // "-->" would end the cue timing line if it ever started a line.
        .replace("-->", "--&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caption::AudioSource;

    fn sample() -> Vec<Caption> {
        vec![
            Caption {
                id: 1,
                source: AudioSource::Microphone,
                speaker: Some(1),
                text: "Hello, how are you feeling today?".into(),
                start_ms: 1_500,
                end_ms: 3_250,
                language: Some("en".into()),
            },
            Caption {
                id: 2,
                source: AudioSource::System,
                speaker: Some(2),
                text: "Much better <thanks> & rested.".into(),
                start_ms: 3_600_000 + 61_005,
                end_ms: 3_600_000 + 63_000,
                language: None,
            },
            Caption {
                id: 3,
                source: AudioSource::System,
                speaker: None,
                text: "[laughter]".into(),
                start_ms: 3_700_000,
                end_ms: 3_700_000,
                language: None,
            },
        ]
    }

    fn names() -> HashMap<u32, String> {
        HashMap::from([(1, "Dr. Rivera".to_string()), (2, "   ".to_string())])
    }

    #[test]
    fn timestamps() {
        assert_eq!(timestamp(3_723_004, ','), "01:02:03,004");
        assert_eq!(timestamp(0, '.'), "00:00:00.000");
        assert_eq!(clock(65_000), "1:05");
        assert_eq!(clock(3_725_000), "1:02:05");
    }

    #[test]
    fn srt_output() {
        let srt = export(
            &sample(),
            &names(),
            ExportFormat::Srt,
            &ExportOptions::default(),
        );
        let expected = "1\n00:00:01,500 --> 00:00:03,250\nDr. Rivera: Hello, how are you feeling today?\n\n\
                        2\n01:01:01,005 --> 01:01:03,000\nSpeaker 2: Much better <thanks> & rested.\n\n\
                        3\n01:01:40,000 --> 01:01:40,001\n[laughter]\n\n";
        assert_eq!(srt, expected);
    }

    #[test]
    fn vtt_output_escapes_and_uses_voice_tags() {
        let vtt = export(
            &sample(),
            &names(),
            ExportFormat::Vtt,
            &ExportOptions::default(),
        );
        assert!(vtt.starts_with("WEBVTT\n\n00:00:01.500 --> 00:00:03.250\n<v Dr. Rivera>Hello"));
        assert!(vtt.contains("<v Speaker 2>Much better &lt;thanks&gt; &amp; rested."));
        assert!(vtt.contains("\n[laughter]\n"));
    }

    #[test]
    fn txt_respects_options() {
        let opts = ExportOptions {
            include_speakers: false,
            include_timestamps: false,
            title: None,
        };
        let txt = export(&sample(), &names(), ExportFormat::Txt, &opts);
        assert_eq!(
            txt,
            "Hello, how are you feeling today?\nMuch better <thanks> & rested.\n[laughter]\n"
        );
        let txt = export(
            &sample(),
            &names(),
            ExportFormat::Txt,
            &ExportOptions::default(),
        );
        assert!(txt.starts_with("[0:01] Dr. Rivera: Hello"));
        assert!(txt.contains("[1:01:01] Speaker 2: Much better"));
    }

    #[test]
    fn markdown_has_title_and_bold_speakers() {
        let opts = ExportOptions {
            title: Some("Appointment".into()),
            ..Default::default()
        };
        let md = export(&sample(), &names(), ExportFormat::Markdown, &opts);
        assert!(md.starts_with("# Appointment\n\n**Dr. Rivera** _0:01_: Hello"));
        assert!(md.contains("_1:01:40_: [laughter]"));
    }

    #[test]
    fn json_round_trips_names() {
        let json = export(
            &sample(),
            &names(),
            ExportFormat::Json,
            &ExportOptions::default(),
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["captions"][0]["speakerName"], "Dr. Rivera");
        assert_eq!(v["captions"][1]["speakerName"], "Speaker 2");
        assert_eq!(v["captions"][2]["speakerName"], serde_json::Value::Null);
        assert_eq!(v["captions"][1]["source"], "system");
    }

    #[test]
    fn format_parsing() {
        assert_eq!("SRT".parse::<ExportFormat>(), Ok(ExportFormat::Srt));
        assert_eq!(
            ExportFormat::from_extension("markdown"),
            Some(ExportFormat::Markdown)
        );
        assert!("docx".parse::<ExportFormat>().is_err());
        assert_eq!(ExportFormat::Vtt.extension(), "vtt");
    }
}
