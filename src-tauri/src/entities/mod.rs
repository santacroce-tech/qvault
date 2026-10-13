//! Entity resolution (Phase 4): group the variants of one entity across
//! documents ("Maria Santos", "SANTOS, Maria", "M. Santos"), and use the
//! knowledge base to suggest occurrences no detection layer caught.
//!
//! * [`normalize`] / [`forms`]: per-category analysis and the comparison
//!   rules (string similarity, initials, honorifics, connectors, legal forms).
//! * [`resolve`]: grouping with automatic merges for unambiguous variants
//!   and proposals for the rest; optional embedding vectors only add or
//!   withdraw proposals.
//! * [`known`]: known-entity suggestions on a page.
//! * [`service`]: the database side (variants, groups, user decisions,
//!   vectors, suggestions with their explanation).
//!
//! Works on every platform without a model; on desktop an embedding model
//! in the QVAC sidecar can be used too. See docs/qvac-integration.md §12.

pub mod forms;
pub mod known;
pub mod normalize;
pub mod resolve;
pub mod service;

/// Something that turns entity strings into L2-normalised vectors (the QVAC
/// sidecar on desktop, a fake in tests).
pub trait Embedder: Send + Sync {
    /// Stable id of the model; vectors are stored with it and only vectors of
    /// the current model are compared.
    fn model_id(&self) -> String;
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
}

/// The text given to the embedding model for one variant. Each model gets
/// its documented query format, and the same format is used on both sides
/// of a comparison (symmetric similarity): nomic-embed-text-v2 (the catalog
/// model) a task prefix, Qwen3-Embedding (compared in the eval) an
/// instruction.
pub fn embedding_input(model_id: &str, category: &str, text: &str) -> String {
    let text = text.trim();
    if model_id.contains("nomic") {
        return format!("search_query: {}", text);
    }
    let what = match forms::kind_of(category) {
        forms::Kind::Person => "a person's name",
        forms::Kind::Company => "a company name",
        forms::Kind::Address => "a postal address",
        _ => "an identifier",
    };
    format!("Instruct: Identify the entity referred to by {}, ignoring titles, word order and abbreviations\nQuery: {}", what, text)
}
