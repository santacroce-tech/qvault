use std::path::{Path, PathBuf};

// Qwen2.5-0.5B-Instruct GGUF (Q4_0 quantization, ~400MB)
#[cfg(feature = "llm")]
const MODEL_REPO: &str = "Qwen/Qwen2.5-0.5B-Instruct-GGUF";
const MODEL_FILENAME: &str = "qwen2.5-0.5b-instruct-q4_0.gguf";
#[cfg(feature = "llm")]
const TOKENIZER_REPO: &str = "Qwen/Qwen2.5-0.5B-Instruct";

/// Directory where models are cached.
pub fn model_dir(app_data: &Path) -> PathBuf {
    app_data.join("models")
}

/// Path to the downloaded GGUF model file.
pub fn model_path(app_data: &Path) -> PathBuf {
    model_dir(app_data).join(MODEL_FILENAME)
}

/// Path to the downloaded tokenizer.json.
pub fn tokenizer_path(app_data: &Path) -> PathBuf {
    model_dir(app_data).join("tokenizer.json")
}

/// Check if the model and tokenizer are both present on disk.
pub fn is_model_available(app_data: &Path) -> bool {
    model_path(app_data).exists() && tokenizer_path(app_data).exists()
}

/// Download the model GGUF file and tokenizer from Hugging Face Hub.
/// Updates `llm_state.download_progress` as download proceeds.
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
        // Use hf-hub to download both files
        let api = hf_hub::api::sync::Api::new()
            .map_err(|e| format!("Failed to init HF API: {}", e))?;

        // Download tokenizer
        log::info!("Downloading tokenizer from {}", TOKENIZER_REPO);
        *llm_state.download_progress.lock().unwrap() = 0.05;

        let tokenizer_src = api
            .model(TOKENIZER_REPO.to_string())
            .get("tokenizer.json")
            .map_err(|e| format!("Failed to download tokenizer: {}", e))?;

        std::fs::copy(&tokenizer_src, tokenizer_path(app_data))
            .map_err(|e| format!("Failed to copy tokenizer: {}", e))?;

        *llm_state.download_progress.lock().unwrap() = 0.1;

        // Download model GGUF
        log::info!("Downloading model from {}/{}", MODEL_REPO, MODEL_FILENAME);

        let model_src = api
            .model(MODEL_REPO.to_string())
            .get(MODEL_FILENAME)
            .map_err(|e| format!("Failed to download model: {}", e))?;

        std::fs::copy(&model_src, model_path(app_data))
            .map_err(|e| format!("Failed to copy model: {}", e))?;

        *llm_state.download_progress.lock().unwrap() = 1.0;
        log::info!("Model download complete");

        Ok(())
    })();

    llm_state.downloading.store(false, Ordering::Relaxed);
    result
}

/// Select the best available compute device: CUDA > Metal > CPU.
#[cfg(feature = "llm")]
fn best_device() -> candle_core::Device {
    // Try CUDA first (NVIDIA GPU)
    #[cfg(feature = "cuda")]
    {
        if let Ok(device) = candle_core::Device::new_cuda(0) {
            log::info!("CUDA GPU detected, using GPU acceleration");
            return device;
        }
    }

    // Try Metal (Apple Silicon / macOS GPU)
    #[cfg(feature = "metal")]
    {
        if let Ok(device) = candle_core::Device::new_metal(0) {
            log::info!("Metal GPU detected, using GPU acceleration");
            return device;
        }
    }

    log::info!("No GPU available, using CPU");
    candle_core::Device::Cpu
}

/// Return a human-readable string for the active device.
#[cfg(feature = "llm")]
pub fn device_name(device: &candle_core::Device) -> &'static str {
    match device {
        candle_core::Device::Cpu => "CPU",
        candle_core::Device::Cuda(_) => "CUDA GPU",
        candle_core::Device::Metal(_) => "Metal GPU",
    }
}

/// Load the GGUF model and tokenizer into memory, returning an LlmRuntime.
#[cfg(feature = "llm")]
pub fn load_model(app_data: &Path) -> Result<super::LlmRuntime, String> {
    use candle_core::quantized::gguf_file;
    use candle_transformers::models::quantized_qwen2::ModelWeights;

    let mpath = model_path(app_data);
    let tpath = tokenizer_path(app_data);

    if !mpath.exists() {
        return Err("Model file not found. Please download it first.".to_string());
    }
    if !tpath.exists() {
        return Err("Tokenizer file not found. Please download it first.".to_string());
    }

    log::info!("Loading model from {:?}", mpath);
    let device = best_device();
    log::info!("Using device: {:?}", device);

    let mut file = std::fs::File::open(&mpath)
        .map_err(|e| format!("Failed to open model file: {}", e))?;

    let model_content = gguf_file::Content::read(&mut file)
        .map_err(|e| format!("Failed to read GGUF: {}", e))?;

    let model = ModelWeights::from_gguf(model_content, &mut file, &device)
        .map_err(|e| format!("Failed to load model weights: {}", e))?;

    log::info!("Loading tokenizer from {:?}", tpath);
    let tokenizer = tokenizers::Tokenizer::from_file(&tpath)
        .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

    log::info!("LLM model loaded successfully");

    Ok(super::LlmRuntime {
        model,
        tokenizer,
        device,
    })
}
