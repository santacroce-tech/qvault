pub mod model_manager;
pub mod llm_scanner;

use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::time::{SystemTime, UNIX_EPOCH};

/// How long (in seconds) the model stays loaded after the last scan before auto-unloading.
const IDLE_TIMEOUT_SECS: u64 = 30;

/// Holds the loaded LLM model and its runtime state.
pub struct LlmState {
    #[cfg(feature = "llm")]
    pub runtime: Mutex<Option<LlmRuntime>>,
    pub enabled: AtomicBool,
    pub downloading: AtomicBool,
    pub download_progress: Mutex<f64>,
    /// Epoch seconds of last inference usage. Used for idle unload.
    pub last_used_at: AtomicU64,
    /// Whether the idle unload timer has been started.
    idle_timer_started: AtomicBool,
}

/// Loaded model + tokenizer, ready for inference.
#[cfg(feature = "llm")]
pub struct LlmRuntime {
    pub model: candle_transformers::models::quantized_qwen2::ModelWeights,
    pub tokenizer: tokenizers::Tokenizer,
    pub device: candle_core::Device,
}

impl LlmState {
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "llm")]
            runtime: Mutex::new(None),
            enabled: AtomicBool::new(false),
            downloading: AtomicBool::new(false),
            download_progress: Mutex::new(0.0),
            last_used_at: AtomicU64::new(0),
            idle_timer_started: AtomicBool::new(false),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, val: bool) {
        self.enabled.store(val, Ordering::Relaxed);
    }

    pub fn is_downloading(&self) -> bool {
        self.downloading.load(Ordering::Relaxed)
    }

    #[cfg(feature = "llm")]
    pub fn is_model_loaded(&self) -> bool {
        self.runtime.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    /// Mark the model as recently used (resets idle timer).
    pub fn touch(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        self.last_used_at.store(now, Ordering::Relaxed);
    }

    /// Start the idle unload background timer (only starts once).
    /// Checks every 10 seconds whether the model has been idle for IDLE_TIMEOUT_SECS.
    #[cfg(feature = "llm")]
    pub fn start_idle_timer(self: &Arc<Self>) {
        if self.idle_timer_started.swap(true, Ordering::Relaxed) {
            return; // already started
        }

        let state = Arc::clone(self);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;

                let last_used = state.last_used_at.load(Ordering::Relaxed);
                if last_used == 0 {
                    continue; // never used yet
                }

                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                if now.saturating_sub(last_used) >= IDLE_TIMEOUT_SECS {
                    let was_loaded = {
                        let mut guard = state.runtime.lock().unwrap();
                        if guard.is_some() {
                            *guard = None;
                            true
                        } else {
                            false
                        }
                    };
                    if was_loaded {
                        log::info!("LLM model unloaded after {}s idle", IDLE_TIMEOUT_SECS);
                        state.last_used_at.store(0, Ordering::Relaxed);
                    }
                }
            }
        });
    }
}
