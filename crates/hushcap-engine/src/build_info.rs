//! Version and build metadata baked in at compile time.

use serde::Serialize;

/// Semantic version of this build (from Cargo).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// CI build number (`0` for local builds).
pub const BUILD_NUMBER: &str = env!("HUSHCAP_BUILD_NUMBER");
/// Short git commit hash, or `unknown`.
pub const GIT_SHA: &str = env!("HUSHCAP_GIT_SHA");
/// Build date (UTC, `YYYY-MM-DD`).
pub const BUILD_DATE: &str = env!("HUSHCAP_BUILD_DATE");

/// Everything about this build, for "About" screens and bug reports.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildInfo {
    /// Semantic version.
    pub version: &'static str,
    /// CI build number.
    pub build_number: &'static str,
    /// Git commit.
    pub git_sha: &'static str,
    /// Build date.
    pub build_date: &'static str,
    /// Operating system the binary was built for.
    pub os: &'static str,
    /// CPU architecture.
    pub arch: &'static str,
    /// Bundled whisper.cpp version, when the speech backend is compiled in.
    pub whisper_cpp: Option<&'static str>,
}

/// Build metadata for this binary.
pub fn build_info() -> BuildInfo {
    BuildInfo {
        version: VERSION,
        build_number: BUILD_NUMBER,
        git_sha: GIT_SHA,
        build_date: BUILD_DATE,
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        #[cfg(feature = "whisper")]
        whisper_cpp: Some(whisper_rs::WHISPER_CPP_VERSION),
        #[cfg(not(feature = "whisper"))]
        whisper_cpp: None,
    }
}

/// `0.1.0 (build 42, 1a2b3c4d5e)`
pub fn version_string() -> String {
    format!("{VERSION} (build {BUILD_NUMBER}, {GIT_SHA})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_is_populated() {
        assert!(!VERSION.is_empty());
        assert!(BUILD_NUMBER.chars().all(|c| c.is_ascii_digit()) || !BUILD_NUMBER.is_empty());
        assert_eq!(BUILD_DATE.len(), 10);
        assert!(version_string().starts_with(VERSION));
        let info = build_info();
        assert_eq!(info.os, std::env::consts::OS);
    }
}
