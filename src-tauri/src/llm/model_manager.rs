use std::path::{Path, PathBuf};

// GLiNER large v2.1 — ONNX-exported on Hugging Face Hub.
// Span-based zero-shot NER, ~440M params (DeBERTa-v3-large backbone).
#[cfg(feature = "llm")]
const MODEL_REPO: &str = "onnx-community/gliner_large-v2.1";
const MODEL_FILENAME: &str = "onnx/model.onnx";
const TOKENIZER_FILENAME: &str = "tokenizer.json";
const CONFIG_FILENAME: &str = "gliner_config.json";

/// Local file names (flattened — we don't preserve the `onnx/` subdir).
const LOCAL_MODEL_FILE: &str = "gliner_large_v2.1.onnx";
const LOCAL_TOKENIZER_FILE: &str = "gliner_tokenizer.json";
const LOCAL_CONFIG_FILE: &str = "gliner_config.json";

pub fn model_dir(app_data: &Path) -> PathBuf {
    app_data.join("models")
}

pub fn model_path(app_data: &Path) -> PathBuf {
    model_dir(app_data).join(LOCAL_MODEL_FILE)
}

pub fn tokenizer_path(app_data: &Path) -> PathBuf {
    model_dir(app_data).join(LOCAL_TOKENIZER_FILE)
}

pub fn config_path(app_data: &Path) -> PathBuf {
    model_dir(app_data).join(LOCAL_CONFIG_FILE)
}

pub fn is_model_available(app_data: &Path) -> bool {
    model_path(app_data).exists() && tokenizer_path(app_data).exists()
}

/// Download the ONNX model, tokenizer, and config from Hugging Face Hub.
#[cfg(feature = "llm")]
pub fn download_model(
    app_data: &Path,
    llm_state: &super::LlmState,
) -> Result<(), String> {
    use std::sync::atomic::Ordering;

    let dir = model_dir(app_data);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create models dir: {}", e))?;

    llm_state.downloading.store(true, Ordering::Relaxed);
    *llm_state.download_progress.lock().unwrap() = 0.0;

    let result = (|| -> Result<(), String> {
        let api = hf_hub::api::sync::Api::new()
            .map_err(|e| format!("Failed to init HF API: {}", e))?;
        let repo = api.model(MODEL_REPO.to_string());

        log::info!("Downloading GLiNER tokenizer from {}", MODEL_REPO);
        *llm_state.download_progress.lock().unwrap() = 0.02;
        let tok_src = repo
            .get(TOKENIZER_FILENAME)
            .map_err(|e| format!("Failed to download tokenizer: {}", e))?;
        std::fs::copy(&tok_src, tokenizer_path(app_data))
            .map_err(|e| format!("Failed to copy tokenizer: {}", e))?;

        // Config is optional but helpful (max_width, label format, etc.).
        *llm_state.download_progress.lock().unwrap() = 0.05;
        if let Ok(cfg_src) = repo.get(CONFIG_FILENAME) {
            let _ = std::fs::copy(&cfg_src, config_path(app_data));
        }

        *llm_state.download_progress.lock().unwrap() = 0.10;
        log::info!("Downloading GLiNER ONNX model (~440MB) from {}", MODEL_REPO);
        let model_src = repo
            .get(MODEL_FILENAME)
            .map_err(|e| format!("Failed to download model: {}", e))?;
        std::fs::copy(&model_src, model_path(app_data))
            .map_err(|e| format!("Failed to copy model: {}", e))?;

        *llm_state.download_progress.lock().unwrap() = 1.0;
        log::info!("GLiNER model download complete");
        Ok(())
    })();

    llm_state.downloading.store(false, Ordering::Relaxed);
    result
}

/// Human-readable name of the active execution provider.
#[cfg(feature = "llm")]
pub fn device_name(rt: &super::LlmRuntime) -> &'static str {
    rt.device_label
}

/// Load the ONNX model and tokenizer.
#[cfg(feature = "llm")]
pub fn load_model(app_data: &Path) -> Result<super::LlmRuntime, String> {
    use ort::session::Session;
    use ort::session::builder::GraphOptimizationLevel;

    let mpath = model_path(app_data);
    let tpath = tokenizer_path(app_data);

    if !mpath.exists() {
        return Err("GLiNER model file not found. Please download it first.".to_string());
    }
    if !tpath.exists() {
        return Err("Tokenizer file not found. Please download it first.".to_string());
    }

    log::info!("Loading GLiNER ONNX session from {:?}", mpath);

    // Try GPU providers in order, fall back to CPU.
    let device_label: &'static str = "CPU";
    let builder = Session::builder()
        .map_err(|e| format!("Failed to create ORT session builder: {}", e))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| format!("Failed to set opt level: {}", e))?;

    #[cfg(feature = "cuda")]
    {
        use ort::execution_providers::CUDAExecutionProvider;
        if builder
            .clone()
            .with_execution_providers([CUDAExecutionProvider::default().build()])
            .is_ok()
        {
            builder = builder
                .with_execution_providers([CUDAExecutionProvider::default().build()])
                .map_err(|e| format!("CUDA EP setup failed: {}", e))?;
            device_label = "CUDA GPU";
        }
    }
    #[cfg(feature = "metal")]
    {
        use ort::execution_providers::CoreMLExecutionProvider;
        if let Ok(b) = builder
            .clone()
            .with_execution_providers([CoreMLExecutionProvider::default().build()])
        {
            builder = b;
            device_label = "CoreML GPU";
        }
    }

    let mut builder = builder;
    let session = builder
        .commit_from_file(&mpath)
        .map_err(|e| format!("Failed to load ONNX model: {}", e))?;

    log::info!("Loading tokenizer from {:?}", tpath);
    let tokenizer = tokenizers::Tokenizer::from_file(&tpath)
        .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

    log::info!("GLiNER model loaded successfully on {}", device_label);

    Ok(super::LlmRuntime {
        session,
        tokenizer,
        device_label,
    })
}
