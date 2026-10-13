//! Layer 3 with OpenAI Privacy Filter (docs/qvac-integration.md §15).
//!
//! [`openai/privacy-filter`](https://huggingface.co/openai/privacy-filter) is
//! a token classifier: for every o200k token it scores 33 BIOES labels over 8
//! PII types. The model runs in the QVAC sidecar through `@qvac/onnx`; this
//! module does everything around it, in Rust and on every platform:
//!
//! * [`tokenizer`]: o200k tokenization from the model's `tokenizer.json`, with
//!   exact byte offsets;
//! * [`decoder`]: OpenAI's constrained Viterbi decoder and span selection
//!   (ported from `opf`, Apache-2.0);
//! * [`retype`]: typing of the model's catch-all `account_number` label;
//! * [`scanner`]: one page in, Qvault detections out (label mapping,
//!   post-filters, confidence, dropping what Layers 1–2 already found).
//!
//! Detections carry `detection_layer = "privacy-filter"` so the review UI can
//! tell them from the language model's (`"llm"`); nothing is auto-applied.

pub mod decoder;
pub mod retype;
pub mod scanner;
pub mod tokenizer;

/// `detection_layer` of Privacy Filter detections.
pub const LAYER: &str = "privacy-filter";
