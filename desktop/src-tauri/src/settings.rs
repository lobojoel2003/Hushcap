//! Persistent desktop settings, stored as JSON in the app's config folder.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use hushcap_engine::asr::Accuracy;
use hushcap_engine::ProcessingSettings;
use serde::{Deserialize, Serialize};

/// How captions look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Appearance {
    /// `system`, `light`, `dark` or `contrast` (yellow on black).
    pub theme: String,
    /// Caption font size multiplier.
    pub font_scale: f32,
    /// `hyperlegible`, `system`, `dyslexic` or `mono`.
    pub font_family: String,
    /// Line height multiplier.
    pub line_spacing: f32,
    /// Show start times next to captions.
    pub show_timestamps: bool,
    /// Keep the window above other windows (handy over video calls).
    pub always_on_top: bool,
    /// Hide everything but the captions.
    pub compact: bool,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            font_scale: 1.0,
            font_family: "hyperlegible".into(),
            line_spacing: 1.45,
            show_timestamps: false,
            always_on_top: false,
            compact: false,
        }
    }
}

/// Everything the user can change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// Speech model reference (catalogue id or `file:<name>`).
    pub speech_model: Option<String>,
    /// Speaker model reference; used when `speaker_labels` is on.
    pub speaker_model: Option<String>,
    /// Label who is speaking.
    pub speaker_labels: bool,
    /// Spoken language code, or `auto`.
    pub language: String,
    /// Translate speech into English.
    pub translate: bool,
    /// Speed/accuracy preset.
    pub accuracy: Accuracy,
    /// Recognition threads (0 = automatic).
    pub threads: usize,
    /// Caption the microphone.
    pub microphone_enabled: bool,
    /// Microphone device id (`None` = system default).
    pub microphone_device: Option<String>,
    /// Caption system audio.
    pub system_enabled: bool,
    /// System audio device id (`None` = system default).
    pub system_device: Option<String>,
    /// Segmentation, voice detection and speaker tuning.
    pub processing: ProcessingSettings,
    /// Look and feel.
    pub appearance: Appearance,
    /// Whether first-run setup has been completed.
    pub onboarding_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            speech_model: None,
            speaker_model: None,
            speaker_labels: true,
            language: "auto".into(),
            translate: false,
            accuracy: Accuracy::Balanced,
            threads: 0,
            microphone_enabled: true,
            microphone_device: None,
            system_enabled: false,
            system_device: None,
            processing: ProcessingSettings::default(),
            appearance: Appearance::default(),
            onboarding_done: false,
        }
    }
}

impl Settings {
    /// Clamp values edited by hand (or by an older version) into safe ranges.
    pub fn sanitized(mut self) -> Self {
        let a = &mut self.appearance;
        a.font_scale = a.font_scale.clamp(0.6, 4.0);
        a.line_spacing = a.line_spacing.clamp(1.0, 2.5);
        if !["system", "light", "dark", "contrast"].contains(&a.theme.as_str()) {
            a.theme = "system".into();
        }
        if !["hyperlegible", "system", "dyslexic", "mono"].contains(&a.font_family.as_str()) {
            a.font_family = "hyperlegible".into();
        }
        let p = &mut self.processing;
        p.vad_sensitivity = p.vad_sensitivity.clamp(0.0, 1.0);
        p.speaker_separation = p.speaker_separation.clamp(0.0, 1.0);
        p.end_silence_ms = p.end_silence_ms.clamp(250, 3000);
        p.max_segment_ms = p.max_segment_ms.clamp(3000, 25_000);
        self.threads = self.threads.min(64);
        if self.language.trim().is_empty() {
            self.language = "auto".into();
        }
        self
    }
}

/// Loads and saves [`Settings`].
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    /// Settings file inside `dir`.
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join("settings.json"),
        }
    }

    /// Load settings, falling back to defaults if the file is missing or
    /// unreadable (a broken file is kept aside for inspection).
    pub fn load(&self) -> Settings {
        match fs::read_to_string(&self.path) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(s) => s.sanitized(),
                Err(e) => {
                    log::warn!("settings file is invalid ({e}); using defaults");
                    let _ = fs::rename(&self.path, self.path.with_extension("json.invalid"));
                    Settings::default()
                }
            },
            Err(_) => Settings::default(),
        }
    }

    /// Save atomically (write to a temporary file, then rename).
    pub fn save(&self, settings: &Settings) -> Result<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(settings)?)
            .with_context(|| format!("could not write {}", tmp.display()))?;
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path());
        assert_eq!(store.load(), Settings::default());

        let s = Settings {
            speech_model: Some("whisper-base.en".into()),
            system_enabled: true,
            appearance: Appearance {
                font_scale: 2.0,
                ..Default::default()
            },
            ..Default::default()
        };
        store.save(&s).unwrap();
        assert_eq!(store.load(), s);
    }

    #[test]
    fn partial_and_out_of_range_files_are_repaired() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path());
        fs::write(
            dir.path().join("settings.json"),
            r#"{"appearance":{"fontScale":99,"theme":"neon"},"language":""}"#,
        )
        .unwrap();
        let s = store.load();
        assert_eq!(s.appearance.font_scale, 4.0);
        assert_eq!(s.appearance.theme, "system");
        assert_eq!(s.language, "auto");
        assert!(s.microphone_enabled, "missing fields take defaults");
    }

    #[test]
    fn corrupt_file_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path());
        fs::write(dir.path().join("settings.json"), "{not json").unwrap();
        assert_eq!(store.load(), Settings::default());
        assert!(dir.path().join("settings.json.invalid").exists());
    }
}
