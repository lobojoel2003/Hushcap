//! Model downloads — the only network access Hushcap ever makes, and only
//! when someone explicitly asks for a model.
//!
//! * HTTPS only, to an allow-list of hosts (also enforced on redirects).
//! * Interrupted downloads resume where they stopped.
//! * Files are checked (SHA-256 when published, format magic always) before
//!   they are moved into place, so a half-written or tampered file is never
//!   used.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use hushcap_core::models::{ModelInfo, ALLOWED_DOWNLOAD_HOSTS};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::storage::{validate_model_file, ModelStore};

/// Download progress.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    /// Bytes on disk so far.
    pub downloaded: u64,
    /// Total size, if known.
    pub total: Option<u64>,
    /// Current speed in bytes per second.
    pub bytes_per_second: f64,
}

/// Whether `host` may serve model files.
pub fn host_allowed(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    ALLOWED_DOWNLOAD_HOSTS.contains(&host.as_str())
        || [".huggingface.co", ".hf.co", ".githubusercontent.com"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
}

fn client() -> Result<reqwest::blocking::Client> {
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() > 8 {
            return attempt.error("too many redirects");
        }
        let url = attempt.url();
        if url.scheme() != "https" || !url.host_str().is_some_and(host_allowed) {
            let msg = format!("refusing to follow redirect to {url}");
            return attempt.error(msg);
        }
        attempt.follow()
    });
    reqwest::blocking::Client::builder()
        .user_agent(concat!(
            "Hushcap/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/lobojoel2003/Hushcap)"
        ))
        .redirect(policy)
        .connect_timeout(Duration::from_secs(20))
        .timeout(None)
        .https_only(true)
        .build()
        .context("could not set up the downloader")
}

fn hash_file(path: &Path) -> Result<(Sha256, u64)> {
    let mut hasher = Sha256::new();
    let mut file = File::open(path)?;
    let mut buf = vec![0u8; 1 << 20];
    let mut total = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    Ok((hasher, total))
}

/// Path of the checksum written next to a downloaded model.
pub fn checksum_path(model: &Path) -> PathBuf {
    let mut name = model.file_name().unwrap_or_default().to_os_string();
    name.push(".sha256");
    model.with_file_name(name)
}

/// Download a catalogue model into `store`. Returns the installed path.
///
/// `cancel` aborts the download (the partial file is kept for resuming).
/// `progress` is called a few times per second.
pub fn download(
    info: &ModelInfo,
    store: &ModelStore,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Progress),
) -> Result<PathBuf> {
    let url = reqwest::Url::parse(info.url).context("invalid model URL")?;
    if url.scheme() != "https" || !url.host_str().is_some_and(host_allowed) {
        bail!("model URL is not on an allowed host: {url}");
    }
    let dest = store.path_for(info);
    let part = dest.with_file_name(format!("{}.part", info.file_name));

    // Resume from an earlier partial download if there is one.
    let mut existing = part.metadata().map(|m| m.len()).unwrap_or(0);
    if info.size_bytes.is_some_and(|size| existing > size) {
        fs::remove_file(&part)?;
        existing = 0;
    }

    let client = client()?;
    let mut request = client.get(url.clone());
    if existing > 0 {
        request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
    }
    let mut response = request
        .send()
        .with_context(|| format!("could not reach {}", url.host_str().unwrap_or("server")))?;
    let status = response.status();

    let (mut hasher, mut downloaded, mut file) =
        if status == reqwest::StatusCode::PARTIAL_CONTENT && existing > 0 {
            let (hasher, len) = hash_file(&part)?;
            let file = OpenOptions::new().append(true).open(&part)?;
            (hasher, len, file)
        } else if status.is_success() {
            (Sha256::new(), 0, File::create(&part)?)
        } else if status == reqwest::StatusCode::RANGE_NOT_SATISFIABLE {
            // The partial file is already complete (or invalid); start over next time.
            let _ = fs::remove_file(&part);
            bail!("download could not be resumed; please try again");
        } else {
            bail!("download failed: server answered {status}");
        };

    let total = response
        .content_length()
        .map(|len| len + downloaded)
        .or(info.size_bytes);
    let mut buf = vec![0u8; 256 * 1024];
    let started = Instant::now();
    let start_bytes = downloaded;
    let mut last_report = Instant::now() - Duration::from_secs(1);
    loop {
        if cancel.load(Ordering::SeqCst) {
            file.flush()?;
            bail!("download cancelled");
        }
        let n = response
            .read(&mut buf)
            .context("connection lost during download")?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        hasher.update(&buf[..n]);
        downloaded += n as u64;
        if last_report.elapsed() >= Duration::from_millis(200) {
            last_report = Instant::now();
            let secs = started.elapsed().as_secs_f64().max(0.001);
            progress(Progress {
                downloaded,
                total,
                bytes_per_second: (downloaded - start_bytes) as f64 / secs,
            });
        }
    }
    file.sync_all()?;
    drop(file);
    progress(Progress {
        downloaded,
        total,
        bytes_per_second: 0.0,
    });

    let digest = hex::encode(hasher.finalize());
    let verify = || -> Result<()> {
        if let Some(size) = info.size_bytes {
            if downloaded != size {
                bail!("downloaded file has the wrong size ({downloaded} bytes, expected {size})");
            }
        }
        if let Some(expected) = info.sha256 {
            if digest != expected {
                bail!("downloaded file failed its integrity check (SHA-256 mismatch)");
            }
        }
        validate_model_file(&part, info.kind)
    };
    if let Err(e) = verify() {
        let _ = fs::remove_file(&part);
        return Err(e);
    }
    fs::rename(&part, &dest).context("could not move the model into place")?;
    fs::write(
        checksum_path(&dest),
        format!("{digest}  {}\n", info.file_name),
    )?;
    Ok(dest)
}

/// Verify an installed model against the catalogue checksum (or the
/// checksum recorded when it was downloaded).
pub fn verify_installed(info: &ModelInfo, store: &ModelStore) -> Result<bool> {
    let path = store.path_for(info);
    let (hasher, _) = hash_file(&path)?;
    let digest = hex::encode(hasher.finalize());
    let expected = match info.sha256 {
        Some(h) => h.to_string(),
        None => fs::read_to_string(checksum_path(&path))
            .ok()
            .and_then(|s| s.split_whitespace().next().map(str::to_owned))
            .ok_or_else(|| anyhow!("no checksum recorded for {}", info.id))?,
    };
    Ok(digest == expected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_list() {
        assert!(host_allowed("huggingface.co"));
        assert!(host_allowed("cdn-lfs.huggingface.co"));
        assert!(host_allowed("cas-bridge.xethub.hf.co"));
        assert!(host_allowed("objects.githubusercontent.com"));
        assert!(host_allowed("GitHub.com"));
        assert!(!host_allowed("example.com"));
        assert!(!host_allowed("huggingface.co.evil.com"));
        assert!(!host_allowed("nothuggingface.co"));
    }

    #[test]
    fn checksum_sidecar_name() {
        assert_eq!(
            checksum_path(Path::new("/m/ggml-base.en.bin")),
            PathBuf::from("/m/ggml-base.en.bin.sha256")
        );
    }

    #[test]
    fn verify_uses_recorded_checksum_when_catalogue_has_none() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::open(dir.path()).unwrap();
        let mut info = hushcap_core::models::find("whisper-tiny.en")
            .unwrap()
            .clone();
        info.sha256 = None; // exercise the fallback to the recorded checksum
        let path = store.path_for(&info);
        fs::write(&path, b"lmgg-test").unwrap();
        assert!(verify_installed(&info, &store).is_err(), "no checksum yet");
        let digest = hex::encode(Sha256::digest(b"lmgg-test"));
        fs::write(checksum_path(&path), format!("{digest}  x\n")).unwrap();
        assert!(verify_installed(&info, &store).unwrap());
        fs::write(&path, b"lmgg-tampered").unwrap();
        assert!(!verify_installed(&info, &store).unwrap());
    }

    #[test]
    fn verify_prefers_the_catalogue_checksum() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::open(dir.path()).unwrap();
        let info = hushcap_core::models::find("whisper-tiny.en").unwrap();
        let path = store.path_for(info);
        fs::write(&path, b"lmgg-not-the-real-model").unwrap();
        // A matching sidecar must not override the pinned catalogue checksum.
        let digest = hex::encode(Sha256::digest(b"lmgg-not-the-real-model"));
        fs::write(checksum_path(&path), format!("{digest}  x\n")).unwrap();
        assert!(!verify_installed(info, &store).unwrap());
    }

    /// Real network download of the smallest model; run with
    /// `cargo test -p hushcap-engine -- --ignored download_speaker_model`.
    #[test]
    #[ignore = "needs network access"]
    fn download_speaker_model() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::open(dir.path()).unwrap();
        let info = hushcap_core::models::find("speaker-campplus-voxceleb").unwrap();
        let cancel = AtomicBool::new(false);
        let mut reports = 0;
        let path = download(info, &store, &cancel, |_| reports += 1).unwrap();
        assert!(path.is_file());
        assert!(reports > 0);
        assert!(verify_installed(info, &store).unwrap());
    }
}
