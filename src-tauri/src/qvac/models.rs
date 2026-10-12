//! On-disk model store under `<app data>/models/`.
//!
//! * Catalog models live at `models/<file_name>`; a file counts as installed
//!   when its size matches the catalog (the SHA-256 is verified when the file
//!   is written by a download or import, and again before the sidecar loads it
//!   for the first time in a session — see `QvacService`).
//! * Files the user imported that match no catalog entry are accepted only
//!   with explicit consent, only as LLMs, and are recorded in
//!   `models/custom/custom.json` with their SHA-256.
//! * Downloads are HTTPS-only, streamed with progress, cancellable, verified
//!   (size + SHA-256) and atomically renamed into place.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::catalog::{self, CatalogModel, ModelPurpose};
use super::client::CancelToken;

#[derive(Debug, Clone, PartialEq)]
pub enum ModelError {
    NotFound(String),
    NoDownloadUrl(String),
    Network(String),
    Io(String),
    SizeMismatch { expected: u64, actual: u64 },
    ChecksumMismatch,
    Cancelled,
    InvalidName,
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelError::NotFound(id) => write!(f, "unknown model: {}", id),
            ModelError::NoDownloadUrl(id) => write!(f, "{} has no download source; import it from a file", id),
            ModelError::Network(e) => write!(f, "download failed: {}", e),
            ModelError::Io(e) => write!(f, "file error: {}", e),
            ModelError::SizeMismatch { expected, actual } => {
                write!(f, "size mismatch (expected {} bytes, got {})", expected, actual)
            }
            ModelError::ChecksumMismatch => write!(f, "SHA-256 checksum mismatch; the file was discarded"),
            ModelError::Cancelled => write!(f, "cancelled"),
            ModelError::InvalidName => write!(f, "invalid file name"),
        }
    }
}

fn io(e: std::io::Error) -> ModelError {
    ModelError::Io(e.kind().to_string())
}

/// A user-imported model that is not in the catalog.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CustomModel {
    pub id: String,
    pub name: String,
    pub file_name: String,
    pub size: u64,
    pub sha256: String,
    pub accepted_at: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum ImportOutcome {
    /// Matched a catalog entry by SHA-256 and was installed.
    Installed { id: String },
    /// Unknown file; the caller must ask the user and retry with consent.
    NeedsConsent { sha256: String, size: u64, file_name: String },
    /// Unknown file installed with the user's consent as a custom LLM.
    InstalledCustom { id: String },
}

pub struct ModelStore {
    dir: PathBuf,
}

impl ModelStore {
    pub fn new(app_data: &Path) -> Self {
        Self { dir: app_data.join("models") }
    }

    /// Store rooted at an explicit models directory (self-test, tests).
    pub fn with_dir(dir: &Path) -> Self {
        Self { dir: dir.to_path_buf() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path_of(&self, m: &CatalogModel) -> PathBuf {
        self.dir.join(m.file_name)
    }

    fn custom_dir(&self) -> PathBuf {
        self.dir.join("custom")
    }

    fn partial_dir(&self) -> PathBuf {
        self.dir.join(".partial")
    }

    pub fn is_installed(&self, m: &CatalogModel) -> bool {
        fs::metadata(self.path_of(m)).map(|md| md.is_file() && md.len() == m.size).unwrap_or(false)
    }

    pub fn custom_models(&self) -> Vec<CustomModel> {
        let p = self.custom_dir().join("custom.json");
        fs::read(&p)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<CustomModel>>(&b).ok())
            .unwrap_or_default()
            .into_iter()
            .filter(|c| self.custom_dir().join(&c.file_name).is_file())
            .collect()
    }

    pub fn custom_path(&self, c: &CustomModel) -> PathBuf {
        self.custom_dir().join(&c.file_name)
    }

    fn save_custom(&self, list: &[CustomModel]) -> Result<(), ModelError> {
        fs::create_dir_all(self.custom_dir()).map_err(io)?;
        let tmp = self.custom_dir().join("custom.json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(list).unwrap_or_default()).map_err(io)?;
        fs::rename(&tmp, self.custom_dir().join("custom.json")).map_err(io)
    }

    /// Resolve an installed model id (catalog or custom) to its path.
    pub fn installed_path(&self, id: &str) -> Option<PathBuf> {
        if let Some(m) = catalog::find(id) {
            return if self.is_installed(m) { Some(self.path_of(m)) } else { None };
        }
        self.custom_models().into_iter().find(|c| c.id == id).map(|c| self.custom_path(&c))
    }

    /// Re-hash an installed file and compare with the expected digest.
    pub fn verify_file(path: &Path, expected_sha256: &str) -> Result<bool, ModelError> {
        let (sha, _) = hash_file(path, None)?;
        Ok(sha.eq_ignore_ascii_case(expected_sha256))
    }

    pub fn delete(&self, id: &str) -> Result<(), ModelError> {
        if let Some(m) = catalog::find(id) {
            let p = self.path_of(m);
            if p.exists() {
                fs::remove_file(&p).map_err(io)?;
            }
            return Ok(());
        }
        let mut list = self.custom_models();
        let before = list.len();
        if let Some(c) = list.iter().find(|c| c.id == id).cloned() {
            let _ = fs::remove_file(self.custom_path(&c));
            list.retain(|x| x.id != id);
        }
        if list.len() == before {
            return Err(ModelError::NotFound(id.to_string()));
        }
        self.save_custom(&list)
    }

    /// Import a local file. Catalog files are recognised by SHA-256; anything
    /// else needs `accept_unknown` (explicit user consent) and is stored as a
    /// custom LLM.
    pub fn import(&self, source: &Path, accept_unknown: bool) -> Result<ImportOutcome, ModelError> {
        let md = fs::metadata(source).map_err(io)?;
        if !md.is_file() {
            return Err(ModelError::Io("not a file".into()));
        }
        let (sha, size) = hash_file(source, None)?;
        fs::create_dir_all(&self.dir).map_err(io)?;

        if let Some(m) = catalog::find_by_sha256(&sha) {
            if size != m.size {
                return Err(ModelError::SizeMismatch { expected: m.size, actual: size });
            }
            copy_atomic(source, &self.partial_dir(), &self.path_of(m), Some(&sha))?;
            return Ok(ImportOutcome::Installed { id: m.id.to_string() });
        }

        let original = source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("model.gguf")
            .to_string();
        if !accept_unknown {
            return Ok(ImportOutcome::NeedsConsent { sha256: sha, size, file_name: original });
        }
        if !original.to_ascii_lowercase().ends_with(".gguf") {
            return Err(ModelError::InvalidName);
        }
        let id = format!("custom-{}", &sha[..12]);
        let file_name = format!("{}.gguf", id);
        let dest = self.custom_dir().join(&file_name);
        fs::create_dir_all(self.custom_dir()).map_err(io)?;
        copy_atomic(source, &self.partial_dir(), &dest, Some(&sha))?;
        let mut list = self.custom_models();
        list.retain(|c| c.id != id);
        list.push(CustomModel {
            id: id.clone(),
            name: sanitize_display_name(&original),
            file_name,
            size,
            sha256: sha,
            accepted_at: chrono::Utc::now().to_rfc3339(),
        });
        self.save_custom(&list)?;
        Ok(ImportOutcome::InstalledCustom { id })
    }

    /// Download a catalog model over HTTPS. `progress(received, total)` is
    /// called as bytes arrive.
    #[cfg(desktop)]
    pub fn download(
        &self,
        m: &CatalogModel,
        cancel: &CancelToken,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<PathBuf, ModelError> {
        let url = m.url.ok_or_else(|| ModelError::NoDownloadUrl(m.id.to_string()))?;
        if !url.starts_with("https://") {
            return Err(ModelError::Network("refusing non-HTTPS URL".into()));
        }
        fs::create_dir_all(self.partial_dir()).map_err(io)?;
        let part = self.partial_dir().join(format!("{}.part", m.file_name));
        let _ = fs::remove_file(&part);

        let agent = ureq::AgentBuilder::new()
            .https_only(true)
            .timeout_connect(Duration::from_secs(30))
            .timeout_read(Duration::from_secs(60))
            .redirects(5)
            .user_agent(concat!("Qvault/", env!("CARGO_PKG_VERSION")))
            .build();
        let resp = agent.get(url).call().map_err(|e| ModelError::Network(short_ureq_error(&e)))?;
        if let Some(len) = resp.header("content-length").and_then(|v| v.parse::<u64>().ok()) {
            if len != m.size {
                return Err(ModelError::SizeMismatch { expected: m.size, actual: len });
            }
        }
        let mut reader = resp.into_reader();
        let result = (|| -> Result<(), ModelError> {
            let mut file = File::create(&part).map_err(io)?;
            let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
            let mut buf = vec![0u8; 256 * 1024];
            let mut received: u64 = 0;
            let mut last_report = std::time::Instant::now();
            progress(0, m.size);
            loop {
                if cancel.is_cancelled() {
                    return Err(ModelError::Cancelled);
                }
                let n = reader.read(&mut buf).map_err(|e| ModelError::Network(e.kind().to_string()))?;
                if n == 0 {
                    break;
                }
                received += n as u64;
                if received > m.size {
                    return Err(ModelError::SizeMismatch { expected: m.size, actual: received });
                }
                ctx.update(&buf[..n]);
                file.write_all(&buf[..n]).map_err(io)?;
                if last_report.elapsed() >= Duration::from_millis(150) {
                    progress(received, m.size);
                    last_report = std::time::Instant::now();
                }
            }
            progress(received, m.size);
            if received != m.size {
                return Err(ModelError::SizeMismatch { expected: m.size, actual: received });
            }
            if hex::encode(ctx.finish().as_ref()) != m.sha256 {
                return Err(ModelError::ChecksumMismatch);
            }
            file.sync_all().map_err(io)?;
            Ok(())
        })();
        if let Err(e) = result {
            let _ = fs::remove_file(&part);
            return Err(e);
        }
        let dest = self.path_of(m);
        fs::rename(&part, &dest).map_err(io)?;
        Ok(dest)
    }
}

/// Stream a file through SHA-256. Returns (hex digest, size).
pub fn hash_file(path: &Path, cancel: Option<&CancelToken>) -> Result<(String, u64), ModelError> {
    let mut f = File::open(path).map_err(io)?;
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; 1024 * 1024];
    let mut size = 0u64;
    loop {
        if cancel.map(|c| c.is_cancelled()).unwrap_or(false) {
            return Err(ModelError::Cancelled);
        }
        let n = f.read(&mut buf).map_err(io)?;
        if n == 0 {
            break;
        }
        size += n as u64;
        ctx.update(&buf[..n]);
    }
    Ok((hex::encode(ctx.finish().as_ref()), size))
}

/// Copy `src` to `dest` via a temp file in `tmp_dir`, re-verifying the hash
/// of the copy before the atomic rename.
fn copy_atomic(src: &Path, tmp_dir: &Path, dest: &Path, expect_sha: Option<&str>) -> Result<(), ModelError> {
    fs::create_dir_all(tmp_dir).map_err(io)?;
    let name = dest.file_name().ok_or(ModelError::InvalidName)?;
    let tmp = tmp_dir.join(format!("{}.import", name.to_string_lossy()));
    let _ = fs::remove_file(&tmp);
    fs::copy(src, &tmp).map_err(io)?;
    if let Some(sha) = expect_sha {
        let (got, _) = hash_file(&tmp, None)?;
        if !got.eq_ignore_ascii_case(sha) {
            let _ = fs::remove_file(&tmp);
            return Err(ModelError::ChecksumMismatch);
        }
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(io)?;
    }
    fs::rename(&tmp, dest).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io(e)
    })
}

fn sanitize_display_name(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(120).collect()
}

#[cfg(desktop)]
fn short_ureq_error(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => format!("HTTP {}", code),
        ureq::Error::Transport(t) => format!("{:?}", t.kind()),
    }
}

/// Purpose a model id can serve (catalog or custom = LLM).
pub fn purpose_of(id: &str) -> ModelPurpose {
    catalog::find(id).map(|m| m.purpose).unwrap_or(ModelPurpose::Llm)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("qvault-models-{}-{}", tag, uuid::Uuid::new_v4()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn hash_file_matches_known_digest() {
        let d = tmpdir("hash");
        let p = d.join("x.bin");
        fs::write(&p, b"abc").unwrap();
        let (sha, size) = hash_file(&p, None).unwrap();
        assert_eq!(size, 3);
        assert_eq!(sha, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn unknown_import_needs_consent_then_installs_as_custom() {
        let d = tmpdir("import");
        let store = ModelStore::new(&d);
        let src = d.join("my-model.gguf");
        fs::write(&src, b"GGUF not really a model").unwrap();

        let out = store.import(&src, false).unwrap();
        assert!(matches!(out, ImportOutcome::NeedsConsent { .. }));
        assert!(store.custom_models().is_empty(), "nothing installed without consent");

        let out = store.import(&src, true).unwrap();
        let id = match out {
            ImportOutcome::InstalledCustom { id } => id,
            other => panic!("unexpected {:?}", other),
        };
        let customs = store.custom_models();
        assert_eq!(customs.len(), 1);
        assert_eq!(customs[0].name, "my-model.gguf");
        let path = store.installed_path(&id).unwrap();
        assert!(ModelStore::verify_file(&path, &customs[0].sha256).unwrap());

        store.delete(&id).unwrap();
        assert!(store.custom_models().is_empty());
        assert!(!path.exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn unknown_non_gguf_is_rejected_even_with_consent() {
        let d = tmpdir("nongguf");
        let store = ModelStore::new(&d);
        let src = d.join("evil.exe");
        fs::write(&src, b"MZ").unwrap();
        assert_eq!(store.import(&src, true).unwrap_err(), ModelError::InvalidName);
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn installed_requires_exact_size() {
        let d = tmpdir("installed");
        let store = ModelStore::new(&d);
        let m = catalog::find("easyocr-latin-g2").unwrap();
        fs::create_dir_all(store.dir()).unwrap();
        fs::write(store.path_of(m), b"truncated").unwrap();
        assert!(!store.is_installed(m));
        assert!(store.installed_path(m.id).is_none());
        store.delete(m.id).unwrap();
        assert!(!store.path_of(m).exists());
        let _ = fs::remove_dir_all(d);
    }

    #[cfg(desktop)]
    #[test]
    fn download_without_url_is_refused() {
        let d = tmpdir("nourl");
        let store = ModelStore::new(&d);
        let m = catalog::find("easyocr-craft-mlt-25k").unwrap();
        let err = store.download(m, &CancelToken::new(), |_, _| {}).unwrap_err();
        assert_eq!(err, ModelError::NoDownloadUrl(m.id.to_string()));
        let _ = fs::remove_dir_all(d);
    }
}
