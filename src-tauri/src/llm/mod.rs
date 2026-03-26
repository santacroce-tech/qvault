pub mod model_manager;
pub mod llm_scanner;

use std::sync::{Mutex, atomic::{AtomicBool, Ordering}};

/// Holds the loaded LLM model and its runtime state.
pub struct LlmState {
    #[cfg(feature = "llm")]
    pub runtime: Mutex<Option<LlmRuntime>>,
    pub enabled: AtomicBool,
    pub downloading: AtomicBool,
    pub download_progress: Mutex<f64>,
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
}
