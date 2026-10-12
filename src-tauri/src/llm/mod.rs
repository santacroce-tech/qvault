//! Layer 3 (LLM-assisted detection). The model runs in the QVAC sidecar; this
//! module holds the transport-agnostic chunking, prompting, validation and
//! offset-mapping logic.
pub mod llm_scanner;
