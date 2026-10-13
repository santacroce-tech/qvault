//! On-disk model store under `<app data>/models/`.
//!
//! * Single-file catalog models live at `models/<file_name>`; multi-file
//!   models (a set of files that must sit side by side, e.g. Privacy Filter's
//!   ONNX graph, its external weights, tokenizer and calibration) live in
//!   `models/<dir>/`. A model counts as installed when every file is present
//!   with the catalog size (the SHA-256 of each file is verified when it is
//!   written by a download or import, and again before the sidecar loads the
//!   model for the first time in a session — see `QvacService`).
//! * Multi-file models are installed atomically: files are collected and
//!   verified in `models/.partial/<dir>/` (a download, or one or more
//!   imports), and the directory is renamed into place only when the set is
//!   complete.
//! * Files the user imported that match no catalog entry are accepted only
//!   with explicit consent, only as LLMs, and are recorded in
//!   `models/custom/custom.json` with their SHA-256.
//! * Downloads are HTTPS-only, streamed with progress, cancellable, verified
//!   (size + SHA-256 per file) and atomically renamed into place.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::catalog::{self, CatalogFile, CatalogModel, ModelPurpose};
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
    /// A folder / multi-file import found no file of any catalog model.
    NothingToImport,
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
            ModelError::NothingToImport => write!(f, "none of the selected files belongs to a known model"),
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
    /// Matched a catalog entry by SHA-256 and was installed (for a multi-file
    /// model: the last missing file arrived and the set was installed).
    Installed { id: String },
    /// Unknown file; the caller must ask the user and retry with consent.
    NeedsConsent { sha256: String, size: u64, file_name: String },
    /// Unknown file installed with the user's consent as a custom LLM.
    InstalledCustom { id: String },
    /// Verified file(s) of a multi-file model were kept; these files are
    /// still needed before it is installed.
    Incomplete { id: String, missing: Vec<String> },
}

/// Result of importing several files or a folder.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub outcomes: Vec<ImportOutcome>,
    /// Files that belong to no catalog model (not imported).
    pub ignored: usize,
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

    /// Path of a model's primary file (the one the sidecar loads).
    pub fn path_of(&self, m: &CatalogModel) -> PathBuf {
        self.file_path(m, m.file_name)
    }

    /// Path of one file of a catalog model.
    pub fn file_path(&self, m: &CatalogModel, name: &str) -> PathBuf {
        match m.dir {
            Some(d) => self.dir.join(d).join(name),
            None => self.dir.join(name),
        }
    }

    fn custom_dir(&self) -> PathBuf {
        self.dir.join("custom")
    }

    fn partial_dir(&self) -> PathBuf {
        self.dir.join(".partial")
    }

    /// Where the files of a multi-file model are collected before install.
    fn staging_dir(&self, m: &CatalogModel) -> PathBuf {
        self.partial_dir().join(m.dir.unwrap_or(m.id))
    }

    pub fn is_installed(&self, m: &CatalogModel) -> bool {
        m.files().iter().all(|f| {
            fs::metadata(self.file_path(m, f.name)).map(|md| md.is_file() && md.len() == f.size).unwrap_or(false)
        })
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

    /// Resolve an installed model id (catalog or custom) to its (primary) path.
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

    /// Re-hash every file of an installed catalog model. Returns the name of
    /// the first file that does not match, if any.
    pub fn verify_model(&self, m: &CatalogModel) -> Result<Option<String>, ModelError> {
        for f in m.files() {
            if !Self::verify_file(&self.file_path(m, f.name), f.sha256)? {
                return Ok(Some(f.name.to_string()));
            }
        }
        Ok(None)
    }

    pub fn delete(&self, id: &str) -> Result<(), ModelError> {
        if let Some(m) = catalog::find(id) {
            if let Some(d) = m.dir {
                let p = self.dir.join(d);
                if p.exists() {
                    fs::remove_dir_all(&p).map_err(io)?;
                }
                let _ = fs::remove_dir_all(self.staging_dir(m));
                return Ok(());
            }
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

    /// Files of a multi-file model that are neither installed nor staged.
    fn missing_parts(&self, m: &CatalogModel) -> Vec<String> {
        let staged = self.staging_dir(m);
        m.files()
            .into_iter()
            .filter(|f| {
                let ok = |p: PathBuf| fs::metadata(p).map(|md| md.is_file() && md.len() == f.size).unwrap_or(false);
                !ok(staged.join(f.name))
            })
            .map(|f| f.name.to_string())
            .collect()
    }

    /// Move a complete staging directory into place (replacing any earlier
    /// copy). Every file was verified when it was staged; sizes are checked
    /// again here.
    fn install_staged(&self, m: &CatalogModel) -> Result<(), ModelError> {
        let staged = self.staging_dir(m);
        for f in m.files() {
            let len = fs::metadata(staged.join(f.name)).map_err(io)?.len();
            if len != f.size {
                return Err(ModelError::SizeMismatch { expected: f.size, actual: len });
            }
        }
        let dest = self.dir.join(m.dir.ok_or(ModelError::InvalidName)?);
        if dest.exists() {
            fs::remove_dir_all(&dest).map_err(io)?;
        }
        fs::rename(&staged, &dest).map_err(io)
    }

    /// Stage one verified part of a multi-file model; install the model when
    /// the set is complete.
    fn import_part(&self, source: &Path, m: &CatalogModel, f: &CatalogFile, sha: &str) -> Result<ImportOutcome, ModelError> {
        if self.is_installed(m) {
            // Same verified bytes as the installed copy: nothing to do.
            return Ok(ImportOutcome::Installed { id: m.id.to_string() });
        }
        let staged = self.staging_dir(m);
        fs::create_dir_all(&staged).map_err(io)?;
        copy_atomic(source, &self.partial_dir(), &staged.join(f.name), Some(sha))?;
        let missing = self.missing_parts(m);
        if missing.is_empty() {
            self.install_staged(m)?;
            return Ok(ImportOutcome::Installed { id: m.id.to_string() });
        }
        Ok(ImportOutcome::Incomplete { id: m.id.to_string(), missing })
    }

    /// Import a local file. Catalog files are recognised by SHA-256 (a part
    /// of a multi-file model is kept until the set is complete); anything
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
        if let Some((m, f)) = catalog::find_part_by_sha256(&sha) {
            if size != f.size {
                return Err(ModelError::SizeMismatch { expected: f.size, actual: size });
            }
            return self.import_part(source, m, &f, &sha);
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

    /// Import several files and/or folders (a folder contributes its files
    /// and those of an `onnx/` subfolder, as in the Hugging Face layout).
    /// Only files that match a catalog file by size and SHA-256 are taken;
    /// the rest are counted as ignored (no custom-model consent here: unknown
    /// models are imported one file at a time).
    pub fn import_paths(&self, paths: &[PathBuf], cancel: Option<&CancelToken>) -> Result<ImportSummary, ModelError> {
        let mut files: Vec<PathBuf> = Vec::new();
        for p in paths {
            let md = fs::metadata(p).map_err(io)?;
            if md.is_dir() {
                for d in [p.clone(), p.join("onnx")] {
                    if let Ok(rd) = fs::read_dir(&d) {
                        let mut v: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file()).collect();
                        v.sort();
                        files.extend(v);
                    }
                }
            } else if md.is_file() {
                files.push(p.clone());
            }
        }
        let mut outcomes: Vec<ImportOutcome> = Vec::new();
        let mut ignored = 0;
        for f in files {
            if cancel.map(|c| c.is_cancelled()).unwrap_or(false) {
                return Err(ModelError::Cancelled);
            }
            let size = fs::metadata(&f).map(|m| m.len()).unwrap_or(0);
            if !catalog::has_file_of_size(size) {
                ignored += 1;
                continue;
            }
            let (sha, _) = hash_file(&f, cancel)?;
            let known = catalog::find_by_sha256(&sha).is_some() || catalog::find_part_by_sha256(&sha).is_some();
            if !known {
                ignored += 1;
                continue;
            }
            let out = self.import(&f, false)?;
            // Keep one outcome per model: the latest state wins.
            let id = match &out {
                ImportOutcome::Installed { id } | ImportOutcome::Incomplete { id, .. } => id.clone(),
                _ => unreachable!("catalog files never need consent"),
            };
            outcomes.retain(|o| !matches!(o, ImportOutcome::Installed { id: i } | ImportOutcome::Incomplete { id: i, .. } if *i == id));
            outcomes.push(out);
        }
        if outcomes.is_empty() {
            return Err(ModelError::NothingToImport);
        }
        Ok(ImportSummary { outcomes, ignored })
    }

    /// Download a catalog model over HTTPS (every file of a multi-file
    /// model). `progress(received, total)` covers all files.
    #[cfg(desktop)]
    pub fn download(
        &self,
        m: &CatalogModel,
        cancel: &CancelToken,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<PathBuf, ModelError> {
        let files = m.files();
        if files.iter().any(|f| f.url.is_none()) {
            return Err(ModelError::NoDownloadUrl(m.id.to_string()));
        }
        let agent = ureq::AgentBuilder::new()
            .https_only(true)
            .timeout_connect(Duration::from_secs(30))
            .timeout_read(Duration::from_secs(60))
            .redirects(5)
            .user_agent(concat!("Qvault/", env!("CARGO_PKG_VERSION")))
            .build();
        let total = m.total_size();
        fs::create_dir_all(self.partial_dir()).map_err(io)?;
        if !m.is_multi_file() {
            let part = self.partial_dir().join(format!("{}.part", m.file_name));
            download_file(&agent, &files[0], &part, cancel, 0, total, &mut progress)?;
            let dest = self.path_of(m);
            fs::rename(&part, &dest).map_err(io)?;
            return Ok(dest);
        }
        // Multi-file: everything into a fresh staging dir, then one rename.
        let staged = self.staging_dir(m);
        let _ = fs::remove_dir_all(&staged);
        fs::create_dir_all(&staged).map_err(io)?;
        let mut done = 0u64;
        let result = (|| -> Result<(), ModelError> {
            for f in &files {
                let part = staged.join(format!("{}.part", f.name));
                download_file(&agent, f, &part, cancel, done, total, &mut progress)?;
                fs::rename(&part, staged.join(f.name)).map_err(io)?;
                done += f.size;
            }
            self.install_staged(m)
        })();
        if let Err(e) = result {
            let _ = fs::remove_dir_all(&staged);
            return Err(e);
        }
        Ok(self.path_of(m))
    }
}

/// Download one file to `part`, verifying size and SHA-256. `offset` and
/// `total` place this file's bytes in the whole model's progress.
#[cfg(desktop)]
fn download_file(
    agent: &ureq::Agent,
    f: &CatalogFile,
    part: &Path,
    cancel: &CancelToken,
    offset: u64,
    total: u64,
    progress: &mut impl FnMut(u64, u64),
) -> Result<(), ModelError> {
    let url = f.url.ok_or(ModelError::NoDownloadUrl(f.name.to_string()))?;
    if !url.starts_with("https://") {
        return Err(ModelError::Network("refusing non-HTTPS URL".into()));
    }
    let _ = fs::remove_file(part);
    let resp = agent.get(url).call().map_err(|e| ModelError::Network(short_ureq_error(&e)))?;
    if let Some(len) = resp.header("content-length").and_then(|v| v.parse::<u64>().ok()) {
        if len != f.size {
            return Err(ModelError::SizeMismatch { expected: f.size, actual: len });
        }
    }
    let mut reader = resp.into_reader();
    let result = (|| -> Result<(), ModelError> {
        let mut file = File::create(part).map_err(io)?;
        let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
        let mut buf = vec![0u8; 256 * 1024];
        let mut received: u64 = 0;
        let mut last_report = std::time::Instant::now();
        progress(offset, total);
        loop {
            if cancel.is_cancelled() {
                return Err(ModelError::Cancelled);
            }
            let n = reader.read(&mut buf).map_err(|e| ModelError::Network(e.kind().to_string()))?;
            if n == 0 {
                break;
            }
            received += n as u64;
            if received > f.size {
                return Err(ModelError::SizeMismatch { expected: f.size, actual: received });
            }
            ctx.update(&buf[..n]);
            file.write_all(&buf[..n]).map_err(io)?;
            if last_report.elapsed() >= Duration::from_millis(150) {
                progress(offset + received, total);
                last_report = std::time::Instant::now();
            }
        }
        progress(offset + received, total);
        if received != f.size {
            return Err(ModelError::SizeMismatch { expected: f.size, actual: received });
        }
        if hex::encode(ctx.finish().as_ref()) != f.sha256 {
            return Err(ModelError::ChecksumMismatch);
        }
        file.sync_all().map_err(io)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(part);
    }
    result
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

    #[test]
    fn multi_file_models_live_in_their_directory_and_need_every_file() {
        let d = tmpdir("multi");
        let store = ModelStore::new(&d);
        let pf = catalog::privacy_filter();
        assert_eq!(store.path_of(pf), d.join("models").join("openai-privacy-filter-int8").join("model_quantized.onnx"));
        assert!(!store.is_installed(pf));
        // Only the small files present (right sizes): still not installed.
        let dir = d.join("models").join(pf.dir.unwrap());
        fs::create_dir_all(&dir).unwrap();
        for f in pf.files().iter().filter(|f| f.size < 1_000_000) {
            let file = File::create(dir.join(f.name)).unwrap();
            file.set_len(f.size).unwrap();
        }
        assert!(!store.is_installed(pf));
        assert!(store.installed_path(pf.id).is_none());
        // A sparse file of the right size for the weights completes the set
        // as far as `is_installed` goes (hashes are checked before loading).
        let big = pf.files().into_iter().find(|f| f.size > 1_000_000).unwrap();
        File::create(dir.join(big.name)).unwrap().set_len(big.size).unwrap();
        let tok = pf.files().into_iter().find(|f| f.name == "tokenizer.json").unwrap();
        File::create(dir.join(tok.name)).unwrap().set_len(tok.size).unwrap();
        assert!(store.is_installed(pf));
        assert_eq!(store.verify_model(pf).unwrap().as_deref(), Some("model_quantized.onnx"), "contents are checked by hash");
        store.delete(pf.id).unwrap();
        assert!(!dir.exists());
        let _ = fs::remove_dir_all(d);
    }

    #[test]
    fn folder_import_ignores_unknown_files() {
        let d = tmpdir("folder");
        let store = ModelStore::new(&d);
        let src = d.join("src");
        fs::create_dir_all(src.join("onnx")).unwrap();
        fs::write(src.join("README.md"), b"hello").unwrap();
        fs::write(src.join("onnx").join("other.onnx"), b"x").unwrap();
        assert_eq!(store.import_paths(&[src.clone()], None).unwrap_err(), ModelError::NothingToImport);
        let _ = fs::remove_dir_all(d);
    }

    /// The published `viterbi_calibration.json` (372 bytes), byte for byte.
    const PF_CALIBRATION: &str = "{\n  \"operating_points\": {\n    \"default\": {\n      \"biases\": {\n        \"transition_bias_background_stay\": 0.0,\n        \"transition_bias_background_to_start\": 0.0,\n        \"transition_bias_end_to_background\": 0.0,\n        \"transition_bias_end_to_start\": 0.0,\n        \"transition_bias_inside_to_continue\": 0.0,\n        \"transition_bias_inside_to_end\": 0.0\n      }\n    }\n  }\n}\n";

    #[test]
    fn a_part_of_a_multi_file_model_is_kept_until_the_set_is_complete() {
        let d = tmpdir("part");
        let store = ModelStore::new(&d);
        let pf = catalog::privacy_filter();
        let src = d.join("dl");
        fs::create_dir_all(&src).unwrap();
        let cal = src.join("viterbi_calibration.json");
        fs::write(&cal, PF_CALIBRATION).unwrap();
        let (sha, size) = hash_file(&cal, None).unwrap();
        assert_eq!((sha.as_str(), size), (pf.extra_files[2].sha256, 372), "fixture is the published file");
        match store.import(&cal, false).unwrap() {
            ImportOutcome::Incomplete { id, missing } => {
                assert_eq!(id, pf.id);
                assert_eq!(missing, vec!["model_quantized.onnx", "model_quantized.onnx_data", "tokenizer.json"]);
            }
            other => panic!("unexpected {:?}", other),
        }
        assert!(!store.is_installed(pf));
        assert!(d.join("models/.partial/openai-privacy-filter-int8/viterbi_calibration.json").is_file());
        // folder import: same file found through the folder, unknown files ignored
        fs::write(src.join("notes.txt"), b"x").unwrap();
        let sum = store.import_paths(&[src.clone()], None).unwrap();
        assert_eq!(sum.ignored, 1);
        assert!(matches!(&sum.outcomes[..], [ImportOutcome::Incomplete { .. }]));
        // deleting the model also drops what was staged
        store.delete(pf.id).unwrap();
        assert!(!d.join("models/.partial/openai-privacy-filter-int8").exists());
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
