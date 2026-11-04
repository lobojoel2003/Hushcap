//! Where Hushcap keeps models on disk, and which ones are installed.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use hushcap_core::models::{self, ModelInfo, ModelKind};
use serde::Serialize;

/// Application identifier, shared with the desktop app so the CLI and the app
/// use the same model folder.
pub const APP_ID: &str = "io.github.lobojoel2003.hushcap";

/// Default data directory (`~/.local/share/<id>` on Linux,
/// `%LOCALAPPDATA%\<id>` on Windows). `HUSHCAP_DATA_DIR` overrides it.
pub fn default_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("HUSHCAP_DATA_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_ID)
}

/// Default model directory: `<data dir>/models`, or `HUSHCAP_MODELS_DIR`.
pub fn default_models_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("HUSHCAP_MODELS_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir);
    }
    default_data_dir().join("models")
}

/// A model file present on disk.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModel {
    /// Catalogue id, or `file:<name>` for imported models.
    pub id: String,
    /// Purpose.
    pub kind: ModelKind,
    /// Display name.
    pub name: String,
    /// Absolute path.
    pub path: PathBuf,
    /// Size in bytes.
    pub size_bytes: u64,
    /// Whether this came from the catalogue (as opposed to being imported).
    pub from_catalog: bool,
}

/// Status of a catalogue model, for model pickers.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    /// Catalogue entry.
    #[serde(flatten)]
    pub info: ModelInfo,
    /// Whether the file is on disk.
    pub installed: bool,
    /// Where it is (or would be) stored.
    pub path: PathBuf,
}

/// Magic number at the start of whisper.cpp GGML model files ("ggml", little-endian).
const GGML_MAGIC: [u8; 4] = *b"lmgg";

/// A directory of model files.
#[derive(Debug, Clone)]
pub struct ModelStore {
    dir: PathBuf,
}

impl ModelStore {
    /// Use `dir`, creating it if needed.
    pub fn open(dir: impl Into<PathBuf>) -> Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)
            .with_context(|| format!("could not create model folder {}", dir.display()))?;
        Ok(Self { dir })
    }

    /// The model directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Path a catalogue model is stored at.
    pub fn path_for(&self, info: &ModelInfo) -> PathBuf {
        self.dir.join(info.file_name)
    }

    /// Whether a catalogue model is installed.
    pub fn is_installed(&self, info: &ModelInfo) -> bool {
        self.path_for(info).is_file()
    }

    /// Every catalogue model with its install status.
    pub fn catalog_status(&self) -> Vec<ModelStatus> {
        models::catalog()
            .iter()
            .map(|info| ModelStatus {
                info: info.clone(),
                installed: self.is_installed(info),
                path: self.path_for(info),
            })
            .collect()
    }

    /// Every usable model file on disk, catalogue or imported.
    pub fn installed(&self) -> Vec<InstalledModel> {
        let mut out = Vec::new();
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()).map(str::to_owned) else {
                continue;
            };
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() {
                continue;
            }
            if let Some(info) = models::catalog().iter().find(|m| m.file_name == name) {
                out.push(InstalledModel {
                    id: info.id.to_string(),
                    kind: info.kind,
                    name: info.name.to_string(),
                    path,
                    size_bytes: meta.len(),
                    from_catalog: true,
                });
            } else if let Some(kind) = kind_from_extension(&path) {
                out.push(InstalledModel {
                    id: format!("file:{name}"),
                    kind,
                    name: format!("Imported · {name}"),
                    path,
                    size_bytes: meta.len(),
                    from_catalog: false,
                });
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Resolve a model reference: a catalogue id, an imported `file:<name>`
    /// id, or a path to a file.
    pub fn resolve(&self, reference: &str) -> Result<PathBuf> {
        if let Some(info) = models::find(reference) {
            let path = self.path_for(info);
            if path.is_file() {
                return Ok(path);
            }
            bail!(
                "model '{}' is not downloaded yet (expected at {})",
                info.id,
                path.display()
            );
        }
        if let Some(name) = reference.strip_prefix("file:") {
            let path = self.dir.join(name);
            if path.is_file() && !name.contains(['/', '\\']) {
                return Ok(path);
            }
            bail!(
                "imported model '{name}' was not found in {}",
                self.dir.display()
            );
        }
        let path = PathBuf::from(reference);
        if path.is_file() {
            return Ok(path);
        }
        bail!("unknown model '{reference}'");
    }

    /// Copy a model file into the store after checking it is the right kind.
    /// Returns the installed path.
    pub fn import(&self, source: &Path) -> Result<InstalledModel> {
        let kind = kind_from_extension(source)
            .context("model files must end in .bin (speech) or .onnx (speaker labels)")?;
        validate_model_file(source, kind)?;
        let name = source
            .file_name()
            .context("model path has no file name")?
            .to_owned();
        let dest = self.dir.join(&name);
        if dest != source {
            let tmp = self
                .dir
                .join(format!("{}.importing", name.to_string_lossy()));
            fs::copy(source, &tmp)
                .with_context(|| format!("could not copy {}", source.display()))?;
            fs::rename(&tmp, &dest)?;
        }
        Ok(self
            .installed()
            .into_iter()
            .find(|m| m.path == dest)
            .expect("just-imported model is listed"))
    }

    /// Delete a model by id (catalogue or imported).
    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.resolve(id)?;
        if path.parent() != Some(self.dir.as_path()) {
            bail!("refusing to delete a file outside the model folder");
        }
        fs::remove_file(&path)?;
        let _ = fs::remove_file(path.with_extension(format!(
            "{}.sha256",
            path.extension().and_then(|e| e.to_str()).unwrap_or("")
        )));
        Ok(())
    }
}

/// Infer a model's purpose from its file extension.
pub fn kind_from_extension(path: &Path) -> Option<ModelKind> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "bin" => Some(ModelKind::Speech),
        "onnx" => Some(ModelKind::Speaker),
        _ => None,
    }
}

/// Cheap sanity checks that a file is the kind of model it claims to be.
pub fn validate_model_file(path: &Path, kind: ModelKind) -> Result<()> {
    let mut file =
        fs::File::open(path).with_context(|| format!("could not open {}", path.display()))?;
    let mut head = [0u8; 4];
    file.read_exact(&mut head)
        .context("model file is too small")?;
    match kind {
        ModelKind::Speech if head != GGML_MAGIC => {
            bail!(
                "{} is not a whisper.cpp (GGML) speech model",
                path.display()
            )
        }
        // ONNX files are protobufs; the first field is normally ir_version (tag 0x08).
        ModelKind::Speaker if head[0] != 0x08 => {
            bail!("{} is not an ONNX model", path.display())
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: &[u8]) {
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn lists_catalog_and_imported_models() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::open(dir.path()).unwrap();
        let base = models::find("whisper-base.en").unwrap();
        write(&store.path_for(base), b"lmgg....");
        write(&dir.path().join("my-model.bin"), b"lmgg....");
        write(&dir.path().join("notes.txt"), b"hello");

        let installed = store.installed();
        assert_eq!(installed.len(), 2);
        assert!(installed
            .iter()
            .any(|m| m.id == "whisper-base.en" && m.from_catalog));
        assert!(installed
            .iter()
            .any(|m| m.id == "file:my-model.bin" && !m.from_catalog));

        let status = store.catalog_status();
        assert!(
            status
                .iter()
                .find(|s| s.info.id == "whisper-base.en")
                .unwrap()
                .installed
        );
        assert!(
            !status
                .iter()
                .find(|s| s.info.id == "whisper-tiny.en")
                .unwrap()
                .installed
        );

        assert_eq!(
            store.resolve("whisper-base.en").unwrap(),
            store.path_for(base)
        );
        assert!(store.resolve("file:my-model.bin").is_ok());
        assert!(store.resolve("whisper-tiny.en").is_err());
        assert!(store.resolve("file:../etc/passwd").is_err());
    }

    #[test]
    fn import_validates_and_copies() {
        let src_dir = tempfile::tempdir().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::open(dir.path()).unwrap();

        let good = src_dir.path().join("custom.bin");
        write(&good, b"lmgg\x01\x02");
        let m = store.import(&good).unwrap();
        assert_eq!(m.kind, ModelKind::Speech);
        assert!(dir.path().join("custom.bin").is_file());

        let bad = src_dir.path().join("fake.bin");
        write(&bad, b"PK\x03\x04");
        assert!(store.import(&bad).is_err());

        let onnx = src_dir.path().join("spk.onnx");
        write(&onnx, b"\x08\x07\x12\x00");
        assert_eq!(store.import(&onnx).unwrap().kind, ModelKind::Speaker);

        assert!(store.import(&src_dir.path().join("x.txt")).is_err());
    }

    #[test]
    fn delete_removes_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = ModelStore::open(dir.path()).unwrap();
        write(&dir.path().join("a.bin"), b"lmgg");
        store.delete("file:a.bin").unwrap();
        assert!(store.installed().is_empty());
        assert!(store.delete("file:a.bin").is_err());
    }
}
