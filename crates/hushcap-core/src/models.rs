//! Catalogue of downloadable models.
//!
//! Models are downloaded once, on request, and then used fully offline.
//! Each entry pins an HTTPS URL and, where published, a SHA-256 checksum that
//! the downloader verifies before a file is used.

use serde::Serialize;

/// What a model is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModelKind {
    /// Speech-to-text (whisper.cpp GGML format).
    Speech,
    /// Speaker embeddings for speaker labels (ONNX format).
    Speaker,
}

/// A downloadable model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    /// Stable identifier, used in settings and on the command line.
    pub id: &'static str,
    /// Purpose of the model.
    pub kind: ModelKind,
    /// Display name.
    pub name: &'static str,
    /// One-line description for people choosing a model.
    pub description: &'static str,
    /// Languages it understands, for display.
    pub languages: &'static str,
    /// Whether it handles languages other than English.
    pub multilingual: bool,
    /// File name on disk.
    pub file_name: &'static str,
    /// Download URL (HTTPS).
    pub url: &'static str,
    /// Approximate download size in megabytes, for display.
    pub size_mb: u32,
    /// Exact size in bytes, when known.
    pub size_bytes: Option<u64>,
    /// Lower-case hex SHA-256 of the file, when known.
    pub sha256: Option<&'static str>,
    /// Suggested default for most people.
    pub recommended: bool,
    /// Relative speed, 1 (slowest) to 5 (fastest).
    pub speed: u8,
    /// Relative accuracy, 1 (lowest) to 5 (highest).
    pub accuracy: u8,
}

const HF: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";
const SHERPA: &str =
    "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/";

macro_rules! whisper {
    ($id:literal, $file:literal, $name:literal, $desc:literal, $langs:literal, $multi:literal,
     $mb:literal, $rec:literal, $speed:literal, $acc:literal) => {
        ModelInfo {
            id: $id,
            kind: ModelKind::Speech,
            name: $name,
            description: $desc,
            languages: $langs,
            multilingual: $multi,
            file_name: $file,
            url: concat!(
                "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/",
                $file
            ),
            size_mb: $mb,
            size_bytes: None,
            sha256: None,
            recommended: $rec,
            speed: $speed,
            accuracy: $acc,
        }
    };
}

static CATALOG: &[ModelInfo] = &[
    whisper!(
        "whisper-base.en-q5_1",
        "ggml-base.en-q5_1.bin",
        "English · Base (compact)",
        "Good accuracy and fast on most laptops. The best place to start.",
        "English",
        false,
        57,
        true,
        4,
        3
    ),
    whisper!(
        "whisper-tiny.en",
        "ggml-tiny.en.bin",
        "English · Tiny",
        "Fastest. For older or low-power computers; makes more mistakes.",
        "English",
        false,
        75,
        false,
        5,
        2
    ),
    whisper!(
        "whisper-base.en",
        "ggml-base.en.bin",
        "English · Base",
        "Full-precision base model. Slightly more accurate than the compact one.",
        "English",
        false,
        142,
        false,
        4,
        3
    ),
    whisper!(
        "whisper-small.en-q5_1",
        "ggml-small.en-q5_1.bin",
        "English · Small (compact)",
        "Noticeably more accurate. Needs a recent 4+ core CPU to keep up live.",
        "English",
        false,
        181,
        false,
        3,
        4
    ),
    whisper!(
        "whisper-small.en",
        "ggml-small.en.bin",
        "English · Small",
        "Full-precision small model. Best English accuracy for live use on a fast CPU.",
        "English",
        false,
        466,
        false,
        2,
        4
    ),
    whisper!(
        "whisper-base",
        "ggml-base.bin",
        "Multilingual · Base",
        "99 languages, detected automatically. Fast on most laptops.",
        "99 languages",
        true,
        142,
        false,
        4,
        3
    ),
    whisper!(
        "whisper-tiny",
        "ggml-tiny.bin",
        "Multilingual · Tiny",
        "99 languages. Fastest multilingual option; makes more mistakes.",
        "99 languages",
        true,
        75,
        false,
        5,
        2
    ),
    whisper!(
        "whisper-small",
        "ggml-small.bin",
        "Multilingual · Small",
        "99 languages with much better accuracy. Needs a fast CPU.",
        "99 languages",
        true,
        466,
        false,
        2,
        4
    ),
    whisper!(
        "whisper-large-v3-turbo-q5_0",
        "ggml-large-v3-turbo-q5_0.bin",
        "Multilingual · Large v3 Turbo (compact)",
        "Highest accuracy. Only for powerful desktops; may lag behind live speech.",
        "99 languages",
        true,
        547,
        false,
        1,
        5
    ),
    ModelInfo {
        id: "speaker-campplus-voxceleb",
        kind: ModelKind::Speaker,
        name: "Speaker labels · CAM++",
        description: "Tells voices apart so captions show who is speaking. Fast enough for any laptop.",
        languages: "Any language",
        multilingual: true,
        file_name: "3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx",
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx",
        size_mb: 30,
        size_bytes: Some(29_596_978),
        sha256: Some("357a834f702b80161e5b981182c038e18553c1f2ca752ed6cec2052365d4129b"),
        recommended: true,
        speed: 4,
        accuracy: 4,
    },
    ModelInfo {
        id: "speaker-wespeaker-resnet34",
        kind: ModelKind::Speaker,
        name: "Speaker labels · WeSpeaker ResNet34",
        description: "Most accurate speaker labels. Uses more CPU; best on desktops.",
        languages: "Any language",
        multilingual: true,
        file_name: "wespeaker_en_voxceleb_resnet34.onnx",
        url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/wespeaker_en_voxceleb_resnet34.onnx",
        size_mb: 26,
        size_bytes: Some(26_534_365),
        sha256: Some("5ef208a9da1453335308a6b6f4e6dfbd7e183a38b604de0a57664f45d257fe94"),
        recommended: false,
        speed: 2,
        accuracy: 5,
    },
];

/// Every known model.
pub fn catalog() -> &'static [ModelInfo] {
    CATALOG
}

/// Look a model up by id.
pub fn find(id: &str) -> Option<&'static ModelInfo> {
    CATALOG.iter().find(|m| m.id == id)
}

/// The recommended model of a kind.
pub fn recommended(kind: ModelKind) -> &'static ModelInfo {
    CATALOG
        .iter()
        .find(|m| m.kind == kind && m.recommended)
        .expect("catalog has a recommended model of every kind")
}

/// Hosts the downloader is allowed to contact. Downloads are the only
/// network traffic Hushcap ever makes, and only when someone asks for one.
pub const ALLOWED_DOWNLOAD_HOSTS: &[&str] = &[
    "huggingface.co",
    "cdn-lfs.huggingface.co",
    "cdn-lfs-us-1.huggingface.co",
    "cas-bridge.xethub.hf.co",
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

#[doc(hidden)]
pub const _URL_PREFIXES: (&str, &str) = (HF, SHERPA);

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_and_files_are_unique() {
        let ids: HashSet<_> = catalog().iter().map(|m| m.id).collect();
        let files: HashSet<_> = catalog().iter().map(|m| m.file_name).collect();
        assert_eq!(ids.len(), catalog().len());
        assert_eq!(files.len(), catalog().len());
    }

    #[test]
    fn one_recommendation_per_kind() {
        for kind in [ModelKind::Speech, ModelKind::Speaker] {
            let n = catalog()
                .iter()
                .filter(|m| m.kind == kind && m.recommended)
                .count();
            assert_eq!(n, 1, "{kind:?}");
            assert_eq!(recommended(kind).kind, kind);
        }
    }

    #[test]
    fn urls_are_https_on_allowed_hosts_and_end_with_file_name() {
        for m in catalog() {
            assert!(m.url.starts_with("https://"), "{}", m.id);
            assert!(m.url.ends_with(m.file_name), "{}", m.id);
            let host = m
                .url
                .trim_start_matches("https://")
                .split('/')
                .next()
                .unwrap();
            assert!(ALLOWED_DOWNLOAD_HOSTS.contains(&host), "{host}");
            assert!(m.url.starts_with(HF) || m.url.starts_with(SHERPA));
        }
    }

    #[test]
    fn checksums_are_well_formed() {
        for m in catalog() {
            if let Some(h) = m.sha256 {
                assert_eq!(h.len(), 64, "{}", m.id);
                assert!(h
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            }
            assert!((1..=5).contains(&m.speed) && (1..=5).contains(&m.accuracy));
        }
        assert!(find("whisper-base.en").is_some());
        assert!(find("nope").is_none());
    }
}
