//! Catalogue of downloadable models.
//!
//! Models are downloaded once, on request, and then used fully offline.
//! Each entry pins an HTTPS URL, the exact file size and its SHA-256
//! checksum; the downloader verifies both before a file is used. The "Model
//! catalogue" workflow re-checks every entry against upstream each month.

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
     $mb:literal, $bytes:literal, $sha:literal, $rec:literal, $speed:literal, $acc:literal) => {
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
            size_bytes: Some($bytes),
            sha256: Some($sha),
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
        59721011,
        "4baf70dd0d7c4247ba2b81fafd9c01005ac77c2f9ef064e00dcf195d0e2fdd2f",
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
        77704715,
        "921e4cf8686fdd993dcd081a5da5b6c365bfde1162e72b08d75ac75289920b1f",
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
        147964211,
        "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
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
        190098681,
        "bfdff4894dcb76bbf647d56263ea2a96645423f1669176f4844a1bf8e478ad30",
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
        487614201,
        "c6138d6d58ecc8322097e0f987c32f1be8bb0a18532a3f88f734d1bbf9c41e5d",
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
        147951465,
        "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
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
        77691713,
        "be07e048e1e599ad46341c8d2a135645097a538221678b7acdd1b1919c6e1b21",
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
        487601967,
        "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
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
        574041195,
        "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2",
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
    fn every_model_is_pinned_and_well_formed() {
        for m in catalog() {
            // Every download is verified, so every entry needs a checksum and size.
            let h = m.sha256.unwrap_or_else(|| panic!("{} has no sha256", m.id));
            assert_eq!(h.len(), 64, "{}", m.id);
            assert!(h
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
            let bytes = m
                .size_bytes
                .unwrap_or_else(|| panic!("{} has no size", m.id));
            let mb = bytes as f64 / 1_000_000.0;
            assert!(
                (mb - m.size_mb as f64).abs() / mb < 0.1,
                "{}: size_mb {} vs {bytes} bytes",
                m.id,
                m.size_mb
            );
            assert!((1..=5).contains(&m.speed) && (1..=5).contains(&m.accuracy));
        }
        assert!(find("whisper-base.en").is_some());
        assert!(find("nope").is_none());
    }
}
