//! QVAC integration (desktop only): sidecar client, pinned model catalog,
//! model store, and the service that ties them together for the commands
//! and the PII pipeline. See docs/qvac-integration.md.
//!
//! Spawning: `std::process` with the sidecar path resolved from the Tauri
//! bundle (externalBin next to the main executable, worker.js under the
//! resource dir), rather than tauri-plugin-shell. The plugin would spawn the
//! same way but adds a JS-reachable shell API and capability surface we do
//! not want, and std::process gives us direct control over stdio, env and
//! kill semantics (and lets unit tests drive a fake sidecar).

pub mod catalog;
pub mod client;
pub mod models;

use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::llm::llm_scanner::{self, Layer3Engine, Layer3Error, PromptVersion};
use catalog::{CatalogModel, ModelPurpose};
use client::{CallOptions, CancelToken, ClientConfig, QvacClient, QvacError, SpawnSpec};
use models::{ImportOutcome, ModelError, ModelStore};

/// Stop the sidecar after this long without AI work (frees model memory).
const IDLE_STOP: Duration = Duration::from_secs(5 * 60);
const LOAD_TIMEOUT: Duration = Duration::from_secs(180);
const COMPLETE_TIMEOUT: Duration = Duration::from_secs(120);
const OCR_TIMEOUT: Duration = Duration::from_secs(180);
const FIT_TIMEOUT: Duration = Duration::from_secs(90);
const EMBED_TIMEOUT: Duration = Duration::from_secs(120);
/// Strings per `embed` request (the sidecar accepts up to 256).
const EMBED_BATCH: usize = 64;
const LLM_CTX: u64 = 4096;
const LLM_PREDICT: u64 = 512;
const LLM_SEED: u64 = 42;

/// Where the bundled sidecar lives.
#[derive(Debug, Clone)]
pub struct SidecarPaths {
    pub program: PathBuf,
    pub worker: PathBuf,
}

impl SidecarPaths {
    /// Resolve the bundled sidecar: the externalBin is placed next to the
    /// main executable (Tauri strips the target-triple suffix), the worker is
    /// a bundled resource. Returns `None` when either is missing or is the
    /// zero-byte placeholder build.rs creates for builds without the sidecar.
    pub fn resolve(resource_dir: Option<&Path>) -> Option<Self> {
        #[cfg(debug_assertions)]
        if let (Some(p), Some(w)) = (std::env::var_os("QVAC_SIDECAR_BIN"), std::env::var_os("QVAC_SIDECAR_WORKER")) {
            return Self::checked(PathBuf::from(p), PathBuf::from(w));
        }
        let exe = std::env::current_exe().ok()?;
        let dir = exe.parent()?;
        let program = dir.join(format!("qvac-sidecar{}", std::env::consts::EXE_SUFFIX));
        let worker = resource_dir?.join("qvac-worker").join("worker.js");
        Self::checked(program, worker)
    }

    pub fn checked(program: PathBuf, worker: PathBuf) -> Option<Self> {
        let prog_ok = std::fs::metadata(&program).map(|m| m.is_file() && m.len() > 0).unwrap_or(false);
        if prog_ok && worker.is_file() {
            Some(Self { program, worker })
        } else {
            None
        }
    }
}

/// Persisted AI settings (`<app data>/ai-settings.json`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    pub layer3_enabled: bool,
    pub layer3_model: Option<String>,
    /// OCR rasterisation resolution; `None` = [`crate::ocr::DEFAULT_OCR_DPI`].
    #[serde(default)]
    pub ocr_dpi: Option<u32>,
    /// Use the embedding model (when installed) to improve entity grouping.
    /// `None` = on.
    #[serde(default)]
    pub entity_embeddings: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FitInfo {
    /// likely-fits | likely-too-large | unknown
    pub verdict: String,
    pub evidence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DownloadState {
    pub received: u64,
    pub total: u64,
    pub active: bool,
    pub error: Option<String>,
    #[serde(skip)]
    cancel: CancelToken,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub purpose: ModelPurpose,
    pub size: u64,
    pub sha256: String,
    pub licence: String,
    pub source: String,
    pub downloadable: bool,
    /// Import-only models: where the user can download the file themselves.
    pub manual_url: Option<String>,
    pub installed: bool,
    pub custom: bool,
    pub default_llm: bool,
    pub download: Option<DownloadState>,
    pub fit: Option<FitInfo>,
    /// Approximate memory needed to run the model (bytes, measured; 0 = unknown).
    pub ram_needed: u64,
    /// Short catalog note shown before download (licence or role caveats).
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub supported: bool,
    pub sidecar_bundled: bool,
    pub sidecar_running: bool,
    pub sidecar_available: bool,
    pub sidecar_version: Option<String>,
    pub unavailable_reason: Option<String>,
    pub ocr_installed: bool,
    /// OCR can run now: sidecar bundled and not given up on, models installed.
    pub ocr_available: bool,
    pub ocr_dpi: u32,
    pub layer3_enabled: bool,
    pub layer3_model: Option<String>,
    pub layer3_ready: bool,
    /// The entity-grouping embedding model is installed.
    pub embedding_installed: bool,
    /// The user setting (default on).
    pub entity_embeddings_enabled: bool,
    /// Embeddings will be used for grouping (installed, enabled, engine usable).
    pub embedding_ready: bool,
    pub models: Vec<ModelInfo>,
    /// Physical memory of this computer, when it can be read (macOS, Linux).
    pub system_memory: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrBlock {
    pub text: String,
    pub bbox: Option<[f64; 4]>,
    pub confidence: Option<f64>,
}

/// Something worth an audit-log line (metadata only).
#[derive(Debug, Clone)]
pub struct AuditEvent {
    pub action: &'static str,
    pub details: String,
}

pub struct QvacService {
    app_data: PathBuf,
    pub store: ModelStore,
    sidecar: Option<SidecarPaths>,
    client: Mutex<Option<Arc<QvacClient>>>,
    /// catalog/custom id -> (client generation, QVAC model id)
    loaded: Mutex<HashMap<String, (u64, String)>>,
    /// Models whose SHA-256 was re-verified this session.
    verified: Mutex<HashSet<String>>,
    fit: Mutex<HashMap<String, FitInfo>>,
    settings: Mutex<AiSettings>,
    downloads: Mutex<HashMap<String, DownloadState>>,
    last_used: AtomicU64,
    active: AtomicUsize,
    reaper_started: AtomicBool,
    /// One OCR request at a time: results of concurrent requests on one OCR
    /// model must never be able to cross (see `discard_engine_after_abandoned_ocr`).
    ocr_lock: Mutex<()>,
}

/// Physical memory, for the Model Manager's pre-download estimate (the
/// real check is assessFit on the downloaded file). Local reads only.
pub fn system_memory_bytes() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("/usr/sbin/sysctl").args(["-n", "hw.memsize"]).output().ok()?;
        return String::from_utf8_lossy(&out.stdout).trim().parse().ok();
    }
    #[cfg(target_os = "linux")]
    {
        let info = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb: u64 = info
            .lines()
            .find(|l| l.starts_with("MemTotal:"))?
            .split_whitespace()
            .nth(1)?
            .parse()
            .ok()?;
        return Some(kb * 1024);
    }
    #[allow(unreachable_code)]
    None
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs()
}

impl QvacService {
    pub fn new(app_data: &Path, models_dir: Option<&Path>, sidecar: Option<SidecarPaths>) -> Self {
        let store = match models_dir {
            Some(d) => ModelStore::with_dir(d),
            None => ModelStore::new(app_data),
        };
        let settings = std::fs::read(app_data.join("ai-settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice::<AiSettings>(&b).ok())
            .unwrap_or_default();
        if sidecar.is_none() {
            log::warn!("qvac: sidecar not bundled; AI features (Layer 3, OCR) are unavailable");
        }
        // Page images from an earlier session that ended abruptly.
        let _ = std::fs::remove_dir_all(app_data.join("qvac-cache").join("pages"));
        Self {
            app_data: app_data.to_path_buf(),
            store,
            sidecar,
            client: Mutex::new(None),
            loaded: Mutex::new(HashMap::new()),
            verified: Mutex::new(HashSet::new()),
            fit: Mutex::new(HashMap::new()),
            settings: Mutex::new(settings),
            downloads: Mutex::new(HashMap::new()),
            last_used: AtomicU64::new(0),
            active: AtomicUsize::new(0),
            reaper_started: AtomicBool::new(false),
            ocr_lock: Mutex::new(()),
        }
    }

    // ── settings ─────────────────────────────────────────────────────────

    pub fn settings(&self) -> AiSettings {
        self.settings.lock().unwrap().clone()
    }

    fn save_settings(&self, s: &AiSettings) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(s).unwrap_or_default();
        std::fs::write(self.app_data.join("ai-settings.json"), bytes).map_err(|e| e.kind().to_string())
    }

    pub fn ocr_dpi(&self) -> u32 {
        self.settings.lock().unwrap().ocr_dpi.unwrap_or(crate::ocr::DEFAULT_OCR_DPI)
    }

    pub fn set_ocr_dpi(&self, dpi: u32) -> Result<(), String> {
        if !crate::ocr::ALLOWED_OCR_DPI.contains(&dpi) {
            return Err(format!("unsupported OCR resolution {} DPI", dpi));
        }
        let mut s = self.settings.lock().unwrap();
        s.ocr_dpi = Some(dpi);
        self.save_settings(&s)
    }

    pub fn set_entity_embeddings(&self, enabled: bool) -> Result<(), String> {
        let mut s = self.settings.lock().unwrap();
        s.entity_embeddings = Some(enabled);
        self.save_settings(&s)?;
        drop(s);
        if !enabled {
            self.unload_purpose(ModelPurpose::Embedding);
        }
        Ok(())
    }

    /// The embedding model entity grouping should use: installed and not
    /// turned off. (The engine may still be unavailable; callers fall back to
    /// rules, and stored vectors of this model remain usable.)
    pub fn embedding_model(&self) -> Option<String> {
        if self.settings.lock().unwrap().entity_embeddings == Some(false) {
            return None;
        }
        let m = catalog::embedding_model();
        self.store.is_installed(m).then(|| m.id.to_string())
    }

    pub fn set_layer3(&self, enabled: bool, model: Option<String>) -> Result<(), String> {
        if let Some(id) = &model {
            if models::purpose_of(id) != ModelPurpose::Llm {
                return Err("not an LLM".into());
            }
        }
        let mut s = self.settings.lock().unwrap();
        s.layer3_enabled = enabled;
        if model.is_some() {
            s.layer3_model = model;
        }
        self.save_settings(&s)?;
        drop(s);
        if !enabled {
            self.unload_all_llms();
        }
        Ok(())
    }

    /// The LLM Layer 3 would use: the selected one if installed, else the
    /// first installed catalog LLM (default first).
    pub fn layer3_model(&self) -> Option<String> {
        let selected = self.settings.lock().unwrap().layer3_model.clone();
        if let Some(id) = selected {
            if self.store.installed_path(&id).is_some() {
                return Some(id);
            }
        }
        let mut llms: Vec<&CatalogModel> =
            catalog::CATALOG.iter().filter(|m| m.purpose == ModelPurpose::Llm).collect();
        llms.sort_by_key(|m| !m.default_llm);
        llms.into_iter().find(|m| self.store.is_installed(m)).map(|m| m.id.to_string())
    }

    // ── sidecar ──────────────────────────────────────────────────────────

    pub fn is_bundled(&self) -> bool {
        self.sidecar.is_some()
    }

    fn client(&self) -> Result<Arc<QvacClient>, QvacError> {
        let paths = self
            .sidecar
            .as_ref()
            .ok_or_else(|| QvacError::Unavailable("AI engine is not bundled in this build".into()))?;
        let mut guard = self.client.lock().unwrap();
        if let Some(c) = guard.as_ref() {
            return Ok(Arc::clone(c));
        }
        let cache = self.app_data.join("qvac-cache");
        let _ = std::fs::create_dir_all(&cache);
        let mut env: Vec<(OsString, OsString)> = Vec::new();
        for key in ["PATH", "HOME", "TMPDIR", "TMP", "TEMP", "SystemRoot", "WINDIR", "USERPROFILE", "LANG"] {
            if let Some(v) = std::env::var_os(key) {
                env.push((key.into(), v));
            }
        }
        let spec = SpawnSpec {
            program: paths.program.clone(),
            args: vec![paths.worker.clone().into_os_string(), "--cache-dir".into(), cache.into_os_string()],
            env,
            clear_env: true,
        };
        let c = Arc::new(QvacClient::new(spec, ClientConfig::default()));
        *guard = Some(Arc::clone(&c));
        Ok(c)
    }

    fn touch(&self) {
        self.last_used.store(now_secs(), Ordering::Relaxed);
    }

    /// Stop the sidecar after a period without AI work.
    pub fn start_idle_reaper(self: &Arc<Self>) {
        if self.reaper_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let weak = Arc::downgrade(self);
        std::thread::Builder::new()
            .name("qvac-idle".into())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_secs(30));
                let svc = match weak.upgrade() {
                    Some(s) => s,
                    None => return,
                };
                let last = svc.last_used.load(Ordering::Relaxed);
                if last == 0 || svc.active.load(Ordering::SeqCst) > 0 {
                    continue;
                }
                if now_secs().saturating_sub(last) >= IDLE_STOP.as_secs() {
                    let client = svc.client.lock().unwrap().clone();
                    if let Some(c) = client {
                        if c.is_running() {
                            log::info!("qvac: stopping idle sidecar");
                            c.stop();
                            svc.loaded.lock().unwrap().clear();
                        }
                    }
                    svc.last_used.store(0, Ordering::Relaxed);
                }
            })
            .ok();
    }

    /// Clean shutdown on app exit.
    pub fn shutdown(&self) {
        for d in self.downloads.lock().unwrap().values() {
            d.cancel.cancel();
        }
        if let Some(c) = self.client.lock().unwrap().take() {
            c.shutdown();
        }
    }

    fn call(&self, method: &str, params: Value, opts: CallOptions) -> Result<Value, QvacError> {
        let c = self.client()?;
        self.active.fetch_add(1, Ordering::SeqCst);
        self.touch();
        let r = c.call(method, params, opts);
        self.touch();
        self.active.fetch_sub(1, Ordering::SeqCst);
        r
    }

    // ── models in the sidecar ────────────────────────────────────────────

    fn expected_sha(&self, id: &str) -> Option<String> {
        if let Some(m) = catalog::find(id) {
            return Some(m.sha256.to_string());
        }
        self.store.custom_models().into_iter().find(|c| c.id == id).map(|c| c.sha256)
    }

    /// Verify (once per session) that an installed file still has the
    /// SHA-256 it was accepted with, then return its path.
    fn verified_path(&self, id: &str) -> Result<PathBuf, String> {
        let path = self.store.installed_path(id).ok_or_else(|| format!("model {} is not installed", id))?;
        if self.verified.lock().unwrap().contains(id) {
            return Ok(path);
        }
        let sha = self.expected_sha(id).ok_or_else(|| format!("unknown model {}", id))?;
        match ModelStore::verify_file(&path, &sha) {
            Ok(true) => {
                self.verified.lock().unwrap().insert(id.to_string());
                Ok(path)
            }
            Ok(false) => Err(format!("model {} failed its SHA-256 check; delete and re-install it", id)),
            Err(e) => Err(e.to_string()),
        }
    }

    /// Load a model (cached per sidecar generation). Returns the QVAC model
    /// id and, when a load actually happened, how long it took.
    fn ensure_loaded(&self, id: &str) -> Result<(String, Option<Duration>), QvacError> {
        let client = self.client()?;
        let gen_now = client.generation();
        if client.is_running() {
            if let Some((g, qid)) = self.loaded.lock().unwrap().get(id).cloned() {
                if g == gen_now {
                    return Ok((qid, None));
                }
            }
        }
        let purpose = models::purpose_of(id);
        let path = self.verified_path(id).map_err(QvacError::Unavailable)?;
        let (type_, config) = match purpose {
            ModelPurpose::Llm => ("llm", json!({"ctx_size": LLM_CTX, "temp": 0, "predict": LLM_PREDICT})),
            ModelPurpose::OcrRecognizer => {
                let (det, _) = catalog::ocr_pair();
                let det_path = self.verified_path(det.id).map_err(QvacError::Unavailable)?;
                ("ocr", json!({"detectorModelSrc": det_path, "langList": ["en"]}))
            }
            ModelPurpose::OcrDetector => return Err(QvacError::Protocol("load the recognizer instead".into())),
            // Model defaults (pooling and normalisation from the GGUF).
            ModelPurpose::Embedding => ("embeddings", json!({})),
        };
        let started = Instant::now();
        let r = self.call(
            "loadModel",
            json!({"path": path, "type": type_, "config": config}),
            CallOptions::timeout(LOAD_TIMEOUT),
        )?;
        let qid = r
            .get("modelId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| QvacError::Protocol("loadModel returned no modelId".into()))?
            .to_string();
        // The process may have restarted during the load; record the
        // generation the model now lives in.
        let generation = client.generation();
        self.loaded.lock().unwrap().insert(id.to_string(), (generation, qid.clone()));
        Ok((qid, Some(started.elapsed())))
    }

    fn unload_all_llms(&self) {
        self.unload_purpose(ModelPurpose::Llm)
    }

    fn unload_purpose(&self, purpose: ModelPurpose) {
        let ids: Vec<(String, String)> = self
            .loaded
            .lock()
            .unwrap()
            .iter()
            .filter(|(id, _)| models::purpose_of(id) == purpose)
            .map(|(id, (_, q))| (id.clone(), q.clone()))
            .collect();
        let running = self.client.lock().unwrap().as_ref().map(|c| c.is_running()).unwrap_or(false);
        for (id, qid) in ids {
            self.loaded.lock().unwrap().remove(&id);
            if running {
                let _ = self.call("unloadModel", json!({"modelId": qid}), CallOptions::timeout(Duration::from_secs(30)));
            }
        }
    }

    // ── fit ──────────────────────────────────────────────────────────────

    /// assessFit on an installed (local) LLM file. Cached for the session.
    pub fn assess_fit(&self, id: &str) -> Result<FitInfo, String> {
        if let Some(f) = self.fit.lock().unwrap().get(id).cloned() {
            return Ok(f);
        }
        if models::purpose_of(id) != ModelPurpose::Llm {
            return Err("fit is only assessed for LLMs".into());
        }
        let path = self.store.installed_path(id).ok_or("model is not installed")?;
        let r = self
            .call(
                "assessFit",
                json!({"path": path, "type": "llm", "config": {"ctx_size": LLM_CTX}}),
                CallOptions::timeout(FIT_TIMEOUT),
            )
            .map_err(|e| e.to_string())?;
        let info = FitInfo {
            verdict: r.get("verdict").and_then(|v| v.as_str()).unwrap_or("unknown").to_string(),
            evidence: r.get("evidence").and_then(|v| v.as_str()).map(String::from),
        };
        self.fit.lock().unwrap().insert(id.to_string(), info.clone());
        Ok(info)
    }

    // ── Layer 3 ──────────────────────────────────────────────────────────

    /// Get Layer 3 ready for a scan: sidecar up, model verified, fits, and
    /// loaded. Errors are user-facing reasons to skip Layer 3.
    pub fn prepare_layer3(self: &Arc<Self>) -> Result<Layer3Session, String> {
        if !self.settings().layer3_enabled {
            return Err("disabled".into());
        }
        let id = self.layer3_model().ok_or("no AI model installed")?;
        if let Ok(f) = self.assess_fit(&id) {
            if f.verdict == "likely-too-large" {
                return Err("the selected AI model is too large for this computer's free memory".into());
            }
        }
        let (_, load_time) = self.ensure_loaded(&id).map_err(|e| e.to_string())?;
        Ok(Layer3Session {
            service: Arc::clone(self),
            model: id.clone(),
            loaded_in: load_time,
            on_tokens: Mutex::new(None),
            prompt: llm_scanner::DEFAULT_PROMPT,
            examples: Vec::new(),
        })
    }

    fn complete_layer3(
        &self,
        model: &str,
        prompt: PromptVersion,
        examples: &[crate::learning::Example],
        chunk: &str,
        on_event: Option<client::EventFn>,
    ) -> Result<String, Layer3Error> {
        let qid = match self.ensure_loaded(model) {
            Ok((q, _)) => q,
            Err(QvacError::Unavailable(r)) => return Err(Layer3Error::Fatal(r)),
            Err(e) => return Err(Layer3Error::Chunk(e.to_string())),
        };
        let messages: Vec<Value> = llm_scanner::messages_with_examples(prompt, chunk, examples)
            .into_iter()
            .map(|(role, content)| json!({"role": role, "content": content}))
            .collect();
        let params = json!({
            "modelId": qid,
            "messages": messages,
            "responseFormat": {"type": "json_schema", "json_schema": {
                "name": "pii_entities", "schema": llm_scanner::response_schema(), "strict": true
            }},
            // Greedy decoding with a fixed seed (determinism), and no
            // reasoning channel: Qwen3 / Gemma 4 would otherwise "think"
            // first; `reasoning_budget: 0` turns it off for this request
            // (a no-op for models without one).
            "generationParams": {"temp": 0, "seed": LLM_SEED, "predict": LLM_PREDICT, "reasoning_budget": 0},
        });
        let opts = CallOptions { timeout: Some(COMPLETE_TIMEOUT), cancel: None, on_event };
        match self.call("complete", params, opts) {
            Ok(r) => Ok(r.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string()),
            Err(QvacError::Unavailable(r)) => Err(Layer3Error::Fatal(r)),
            Err(QvacError::Remote { code, message }) if code == "MODEL_NOT_LOADED" => {
                self.loaded.lock().unwrap().remove(model);
                Err(Layer3Error::Chunk(message))
            }
            Err(e) => Err(Layer3Error::Chunk(e.to_string())),
        }
    }

    // ── embeddings (entity grouping) ─────────────────────────────────────

    /// Embed strings with an installed embedding model. Vectors come back
    /// L2-normalised (QVAC normalises them). Batched; any failure fails the
    /// whole call (callers fall back to rules).
    pub fn embed_texts(&self, model: &str, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if models::purpose_of(model) != ModelPurpose::Embedding {
            return Err("not an embedding model".into());
        }
        let qid = self.ensure_loaded(model).map_err(|e| e.to_string())?.0;
        let mut out = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(EMBED_BATCH) {
            let r = match self.call("embed", json!({"modelId": qid, "text": chunk}), CallOptions::timeout(EMBED_TIMEOUT)) {
                Ok(r) => r,
                Err(QvacError::Remote { code, message }) if code == "MODEL_NOT_LOADED" => {
                    self.loaded.lock().unwrap().remove(model);
                    return Err(message);
                }
                Err(e) => return Err(e.to_string()),
            };
            let vecs: Vec<Vec<f32>> = r
                .get("embedding")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .ok_or("the embedding model returned no vectors")?;
            if vecs.len() != chunk.len() || vecs.iter().any(|v| v.is_empty()) {
                return Err("the embedding model returned the wrong number of vectors".into());
            }
            out.extend(vecs);
        }
        Ok(out)
    }

    // ── OCR ──────────────────────────────────────────────────────────────

    pub fn ocr_installed(&self) -> bool {
        let (d, r) = catalog::ocr_pair();
        self.store.is_installed(d) && self.store.is_installed(r)
    }

    /// OCR one image (absolute path to PNG/JPEG/BMP). Box coordinates are in
    /// pixels of the input image.
    pub fn ocr(&self, image: &Path) -> Result<Vec<OcrBlock>, String> {
        if !self.ocr_installed() {
            return Err("OCR models are not installed".into());
        }
        self.ocr_blocks(image, None)
            .map(|blocks| {
                blocks
                    .into_iter()
                    .map(|b| OcrBlock { text: b.text, bbox: Some(b.bbox), confidence: b.confidence })
                    .collect()
            })
            .map_err(|e| e.to_string())
    }

    /// QVAC's OCR has no native cancel (its plugin declares
    /// `cancel: {scope: 'none'}`, a soft cancel): an abandoned page keeps
    /// running in the addon, and its output was observed being delivered as
    /// the result of the *next* OCR request on the same model (page 1's
    /// blocks returned for page 3). After a cancelled or timed-out OCR the
    /// sidecar is therefore stopped, so the next page runs in a fresh
    /// process. This is a stop, not a crash: it does not count towards the
    /// restart cap.
    fn discard_engine_after_abandoned_ocr(&self) {
        log::info!("qvac: OCR request abandoned; restarting the sidecar before the next OCR");
        let client = self.client.lock().unwrap().clone();
        if let Some(c) = client {
            c.stop();
        }
        self.loaded.lock().unwrap().clear();
    }

    /// App-private directory for page images sent to OCR (inside the
    /// sidecar's cache dir; wiped at start-up, files deleted after each page).
    pub fn page_work_dir(&self) -> Result<PathBuf, String> {
        let dir = self.app_data.join("qvac-cache").join("pages");
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create OCR work dir: {}", e.kind()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }
        Ok(dir)
    }

    /// OCR one image with cancellation, for the page pipeline. Every block
    /// must carry a box: text we cannot place on the page could not be
    /// redacted, so such a result fails the page instead.
    pub fn ocr_blocks(&self, image: &Path, cancel: Option<CancelToken>) -> Result<Vec<crate::ocr::RawBlock>, crate::ocr::OcrFailure> {
        use crate::ocr::OcrFailure;
        if !self.is_bundled() {
            return Err(OcrFailure::Unavailable("AI engine is not bundled in this build".into()));
        }
        if !self.ocr_installed() {
            return Err(OcrFailure::Unavailable("OCR models are not installed".into()));
        }
        // Wait for any other OCR request to finish (cancellable).
        let _one_at_a_time = loop {
            match self.ocr_lock.try_lock() {
                Ok(g) => break g,
                Err(std::sync::TryLockError::Poisoned(p)) => break p.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => {
                    if cancel.as_ref().map(|c| c.is_cancelled()).unwrap_or(false) {
                        return Err(OcrFailure::Cancelled);
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        };
        let (_, rec) = catalog::ocr_pair();
        let qid = match self.ensure_loaded(rec.id) {
            Ok((q, _)) => q,
            Err(QvacError::Unavailable(r)) | Err(QvacError::Spawn(r)) => return Err(OcrFailure::Unavailable(r)),
            Err(e) => return Err(OcrFailure::Failed(e.to_string())),
        };
        let opts = CallOptions { timeout: Some(OCR_TIMEOUT), cancel, on_event: None };
        let r = match self.call("ocr", json!({"modelId": qid, "image": image}), opts) {
            Ok(r) => r,
            Err(QvacError::Cancelled) => {
                self.discard_engine_after_abandoned_ocr();
                return Err(OcrFailure::Cancelled);
            }
            Err(QvacError::Timeout) => {
                self.discard_engine_after_abandoned_ocr();
                return Err(OcrFailure::Failed("OCR timed out".into()));
            }
            Err(QvacError::Unavailable(r)) => return Err(OcrFailure::Unavailable(r)),
            Err(QvacError::Remote { code, .. }) if code == "CANCELLED" => {
                self.discard_engine_after_abandoned_ocr();
                return Err(OcrFailure::Cancelled);
            }
            Err(QvacError::Remote { code, message }) if code == "MODEL_NOT_LOADED" => {
                self.loaded.lock().unwrap().remove(rec.id);
                return Err(OcrFailure::Failed(message));
            }
            Err(e) => return Err(OcrFailure::Failed(e.to_string())),
        };
        let mut out = Vec::new();
        for b in r.get("blocks").and_then(|b| b.as_array()).cloned().unwrap_or_default() {
            let text = b.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let bbox: Option<[f64; 4]> = b.get("bbox").and_then(|v| serde_json::from_value(v.clone()).ok());
            match bbox {
                Some(bbox) => out.push(crate::ocr::RawBlock { text, bbox, confidence: b.get("confidence").and_then(|v| v.as_f64()) }),
                None if text.trim().is_empty() => {}
                None => return Err(OcrFailure::Failed("OCR returned text without a position".into())),
            }
        }
        Ok(out)
    }

    // ── model manager ────────────────────────────────────────────────────

    pub fn list_models(&self) -> Vec<ModelInfo> {
        let downloads = self.downloads.lock().unwrap().clone();
        let fit = self.fit.lock().unwrap().clone();
        let mut out: Vec<ModelInfo> = catalog::CATALOG
            .iter()
            .map(|m| ModelInfo {
                id: m.id.into(),
                name: m.name.into(),
                purpose: m.purpose,
                size: m.size,
                sha256: m.sha256.into(),
                licence: m.licence.into(),
                source: m.source.into(),
                downloadable: m.url.is_some(),
                manual_url: m.manual_url.map(Into::into),
                installed: self.store.is_installed(m),
                custom: false,
                default_llm: m.default_llm,
                ram_needed: m.ram_needed,
                note: m.note.map(Into::into),
                download: downloads.get(m.id).cloned(),
                fit: fit.get(m.id).cloned(),
            })
            .collect();
        for c in self.store.custom_models() {
            out.push(ModelInfo {
                id: c.id.clone(),
                name: c.name.clone(),
                purpose: ModelPurpose::Llm,
                size: c.size,
                sha256: c.sha256.clone(),
                licence: "Unknown (imported by user)".into(),
                source: "Imported from a local file".into(),
                downloadable: false,
                manual_url: None,
                installed: true,
                custom: true,
                default_llm: false,
                ram_needed: 0,
                note: None,
                download: None,
                fit: fit.get(&c.id).cloned(),
            });
        }
        out
    }

    pub fn capabilities(&self, probe: bool) -> Capabilities {
        let mut reason = None;
        if probe && self.is_bundled() {
            match self.client().and_then(|c| c.ensure_started().map(|_| c)) {
                Ok(_) => {
                    self.touch();
                    // Fit verdicts for installed LLMs (local files only).
                    for m in catalog::CATALOG.iter().filter(|m| m.purpose == ModelPurpose::Llm) {
                        if self.store.is_installed(m) {
                            if let Err(e) = self.assess_fit(m.id) {
                                log::warn!("qvac: fit check for {} failed: {}", m.id, e);
                            }
                        }
                    }
                }
                Err(e) => reason = Some(e.to_string()),
            }
        }
        let client = self.client.lock().unwrap().clone();
        let settings = self.settings();
        let layer3_model = self.layer3_model();
        let available = self.is_bundled() && client.as_ref().map(|c| c.is_available()).unwrap_or(true);
        Capabilities {
            supported: true,
            sidecar_bundled: self.is_bundled(),
            sidecar_running: client.as_ref().map(|c| c.is_running()).unwrap_or(false),
            sidecar_available: available,
            sidecar_version: client.as_ref().and_then(|c| c.version()),
            unavailable_reason: reason
                .or_else(|| client.as_ref().and_then(|c| c.unavailable_reason()))
                .or_else(|| (!self.is_bundled()).then(|| "AI engine is not bundled in this build".to_string())),
            ocr_installed: self.ocr_installed(),
            ocr_available: available && self.ocr_installed(),
            ocr_dpi: settings.ocr_dpi.unwrap_or(crate::ocr::DEFAULT_OCR_DPI),
            layer3_enabled: settings.layer3_enabled,
            layer3_ready: settings.layer3_enabled && layer3_model.is_some() && available,
            layer3_model,
            embedding_installed: self.store.is_installed(catalog::embedding_model()),
            entity_embeddings_enabled: settings.entity_embeddings != Some(false),
            embedding_ready: self.embedding_model().is_some() && available,
            models: self.list_models(),
            system_memory: system_memory_bytes(),
        }
    }

    pub fn download_state(&self, id: &str) -> Option<DownloadState> {
        self.downloads.lock().unwrap().get(id).cloned()
    }

    /// Start a user-initiated download in the background. `notify` is called
    /// with progress updates and once at the end (Ok(()) or Err(reason)).
    pub fn start_download<F, D>(self: &Arc<Self>, id: &str, notify: F, done: D) -> Result<(), String>
    where
        F: Fn(&str, u64, u64) + Send + 'static,
        D: FnOnce(&str, Result<Duration, String>) + Send + 'static,
    {
        let m = catalog::find(id).ok_or("unknown model")?;
        if m.url.is_none() {
            return Err(ModelError::NoDownloadUrl(m.id.into()).to_string());
        }
        let cancel = CancelToken::new();
        {
            let mut d = self.downloads.lock().unwrap();
            if d.get(id).map(|s| s.active).unwrap_or(false) {
                return Err("download already in progress".into());
            }
            d.insert(id.into(), DownloadState { received: 0, total: m.size, active: true, error: None, cancel: cancel.clone() });
        }
        let svc = Arc::clone(self);
        std::thread::Builder::new()
            .name("qvac-download".into())
            .spawn(move || {
                let started = Instant::now();
                let id = m.id;
                let r = svc.store.download(m, &cancel, |rx, total| {
                    if let Some(s) = svc.downloads.lock().unwrap().get_mut(id) {
                        s.received = rx;
                        s.total = total;
                    }
                    notify(id, rx, total);
                });
                let outcome = match r {
                    Ok(_) => {
                        svc.verified.lock().unwrap().insert(id.to_string());
                        svc.downloads.lock().unwrap().remove(id);
                        Ok(started.elapsed())
                    }
                    Err(e) => {
                        let msg = e.to_string();
                        if let Some(s) = svc.downloads.lock().unwrap().get_mut(id) {
                            s.active = false;
                            s.error = if e == ModelError::Cancelled { None } else { Some(msg.clone()) };
                        }
                        Err(msg)
                    }
                };
                done(id, outcome);
            })
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn cancel_download(&self, id: &str) -> bool {
        match self.downloads.lock().unwrap().get(id) {
            Some(s) if s.active => {
                s.cancel.cancel();
                true
            }
            _ => false,
        }
    }

    pub fn import_model(&self, path: &Path, accept_unknown: bool) -> Result<ImportOutcome, String> {
        let r = self.store.import(path, accept_unknown).map_err(|e| e.to_string())?;
        match &r {
            ImportOutcome::Installed { id } | ImportOutcome::InstalledCustom { id } => {
                self.verified.lock().unwrap().insert(id.clone());
                self.fit.lock().unwrap().remove(id);
            }
            ImportOutcome::NeedsConsent { .. } => {}
        }
        Ok(r)
    }

    pub fn delete_model(&self, id: &str) -> Result<(), String> {
        if self.downloads.lock().unwrap().get(id).map(|s| s.active).unwrap_or(false) {
            return Err("cancel the download first".into());
        }
        // Unload it (and the OCR pair if either half goes).
        let affected: Vec<String> = match models::purpose_of(id) {
            ModelPurpose::Llm | ModelPurpose::Embedding => vec![id.to_string()],
            _ => {
                let (d, r) = catalog::ocr_pair();
                vec![d.id.to_string(), r.id.to_string()]
            }
        };
        for a in &affected {
            if let Some((_, qid)) = self.loaded.lock().unwrap().remove(a) {
                let _ = self.call("unloadModel", json!({"modelId": qid}), CallOptions::timeout(Duration::from_secs(30)));
            }
        }
        self.store.delete(id).map_err(|e| e.to_string())?;
        self.verified.lock().unwrap().remove(id);
        self.fit.lock().unwrap().remove(id);
        self.downloads.lock().unwrap().remove(id);
        Ok(())
    }

    // ── self test ────────────────────────────────────────────────────────

    /// End-to-end check of the bundled engine with whatever models are
    /// installed: OCR on a synthetic image, Layer 3 on a synthetic sentence.
    /// Uses no user documents.
    pub fn self_test(self: &Arc<Self>, ocr_image: Option<&Path>) -> SelfTestReport {
        let mut rep = SelfTestReport::default();
        let t = Instant::now();
        match self.client().and_then(|c| c.ensure_started().map(|_| c)) {
            Ok(c) => {
                rep.sidecar = Some(format!("{} (ready in {} ms)", c.version().unwrap_or_default(), t.elapsed().as_millis()));
            }
            Err(e) => {
                rep.error = Some(e.to_string());
                return rep;
            }
        }
        if let Some(img) = ocr_image {
            if self.ocr_installed() {
                let t = Instant::now();
                match self.ocr(img) {
                    Ok(blocks) => {
                        rep.ocr_ms = Some(t.elapsed().as_millis() as u64);
                        rep.ocr_text = Some(blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join(" | "));
                    }
                    Err(e) => rep.ocr_error = Some(e),
                }
            } else {
                rep.ocr_error = Some("OCR models are not installed".into());
            }
        }
        if let Some(model) = self.embedding_model() {
            let t = Instant::now();
            let names = vec!["Maria Santos".to_string(), "SANTOS, Maria".to_string(), "Atlas Logistics Inc.".to_string()];
            match self.embed_texts(&model, &names) {
                Ok(v) => {
                    rep.embedding_ms = Some(t.elapsed().as_millis() as u64);
                    rep.embedding_dims = Some(v[0].len());
                }
                Err(e) => rep.embedding_error = Some(e),
            }
        }
        match self.layer3_model() {
            Some(id) => {
                let t = Instant::now();
                let sample = "Witness statement of Dr. Amara Okafor, born 14/02/1979, regarding the incident at Riverside clinic.";
                let session = Layer3Session {
                    service: Arc::clone(self),
                    model: id.clone(),
                    loaded_in: None,
                    on_tokens: Mutex::new(None),
                    prompt: llm_scanner::DEFAULT_PROMPT,
                    examples: Vec::new(),
                };
                let report = llm_scanner::scan(&session, sample, 0, &[], None);
                rep.layer3_model = Some(id);
                rep.layer3_ms = Some(t.elapsed().as_millis() as u64);
                if let Some(a) = report.aborted {
                    rep.layer3_error = Some(a);
                } else if report.failed_chunks > 0 {
                    rep.layer3_error = Some("the model returned no usable answer".into());
                }
                rep.layer3_entities = report.entities.iter().map(|e| format!("{}: {}", e.category.to_string(), e.text)).collect();
            }
            None => rep.layer3_error = Some("no LLM installed".into()),
        }
        rep
    }
}

impl crate::ocr::PageOcr for QvacService {
    fn ocr_image(&self, image: &Path) -> Result<Vec<crate::ocr::RawBlock>, crate::ocr::OcrFailure> {
        self.ocr_blocks(image, None)
    }
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SelfTestReport {
    pub sidecar: Option<String>,
    pub error: Option<String>,
    pub ocr_ms: Option<u64>,
    pub ocr_text: Option<String>,
    pub ocr_error: Option<String>,
    pub layer3_model: Option<String>,
    pub layer3_ms: Option<u64>,
    pub layer3_entities: Vec<String>,
    pub layer3_error: Option<String>,
    pub embedding_ms: Option<u64>,
    pub embedding_dims: Option<usize>,
    pub embedding_error: Option<String>,
}

/// The sidecar's embedding model as an entity-grouping [`Embedder`].
pub struct QvacEmbedder {
    pub service: Arc<QvacService>,
    pub model: String,
}

impl crate::entities::Embedder for QvacEmbedder {
    fn model_id(&self) -> String {
        self.model.clone()
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.service.embed_texts(&self.model, texts)
    }
}

/// A prepared Layer 3 run; implements the scanner's engine trait.
pub struct Layer3Session {
    service: Arc<QvacService>,
    pub model: String,
    /// Set when preparing the session loaded the model.
    pub loaded_in: Option<Duration>,
    on_tokens: Mutex<Option<client::EventFn>>,
    /// Prompt version (the eval harness compares versions).
    pub prompt: PromptVersion,
    /// Few-shot examples from earlier review decisions (§13; empty in the
    /// app unless measured to help).
    pub examples: Vec<crate::learning::Example>,
}

impl Layer3Session {
    /// Receive token-count progress while the model generates.
    pub fn set_token_callback(&self, cb: Option<client::EventFn>) {
        *self.on_tokens.lock().unwrap() = cb;
    }
}

impl Layer3Engine for Layer3Session {
    fn complete(&self, chunk_text: &str) -> Result<String, Layer3Error> {
        let cb = self.on_tokens.lock().unwrap().clone();
        self.service.complete_layer3(&self.model, self.prompt, &self.examples, chunk_text, cb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_sidecar_degrades_gracefully() {
        let d = std::env::temp_dir().join(format!("qvault-svc-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let svc = Arc::new(QvacService::new(&d, None, None));
        let caps = svc.capabilities(true);
        assert!(!caps.sidecar_bundled);
        assert!(!caps.sidecar_available);
        assert!(!caps.layer3_ready);
        assert!(caps.unavailable_reason.unwrap().contains("not bundled"));
        // Layer 3 is skipped with a reason, never an error that blocks a scan.
        svc.set_layer3(true, None).unwrap();
        let err = svc.prepare_layer3().err().unwrap();
        assert!(err.contains("no AI model installed"), "{}", err);
        assert!(svc.ocr(Path::new("/tmp/x.png")).is_err());
        // Settings persist.
        let again = QvacService::new(&d, None, None);
        assert!(again.settings().layer3_enabled);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn placeholder_sidecar_is_not_bundled() {
        let d = std::env::temp_dir().join(format!("qvault-ph-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        let prog = d.join("qvac-sidecar");
        let worker = d.join("worker.js");
        std::fs::write(&prog, b"").unwrap();
        std::fs::write(&worker, b"//").unwrap();
        assert!(SidecarPaths::checked(prog.clone(), worker.clone()).is_none());
        std::fs::write(&prog, b"#!/bin/sh").unwrap();
        assert!(SidecarPaths::checked(prog, worker).is_some());
        let _ = std::fs::remove_dir_all(d);
    }
}
