pub mod types;
pub mod regex_scanner;
pub mod heuristic_scanner;
pub mod patterns;

use types::PiiEntity;
use regex_scanner::RegexScanner;
use heuristic_scanner::HeuristicScanner;

/// Run the PII detection pipeline. Layers 1 (regex) and 2 (heuristic) always run.
/// Layer 3 (LLM) runs only if an LlmRuntime is provided.
#[cfg(feature = "llm")]
pub fn run_pipeline(
    text: &str,
    page: u32,
    jurisdictions: &[&str],
    llm_runtime: Option<&mut crate::llm::LlmRuntime>,
) -> Vec<PiiEntity> {
    let mut entities = run_layers_1_2(text, page, jurisdictions);

    // Layer 3: LLM-based detection (optional)
    if let Some(runtime) = llm_runtime {
        let existing_ranges: Vec<(usize, usize)> = entities.iter()
            .map(|e| (e.start, e.end))
            .collect();
        let mut llm_entities = crate::llm::llm_scanner::scan(runtime, text, page, &existing_ranges);
        entities.append(&mut llm_entities);
    }

    entities.sort_by_key(|e| e.start);
    entities
}

/// Run the PII detection pipeline without LLM support (no `llm` feature).
#[cfg(not(feature = "llm"))]
pub fn run_pipeline(text: &str, page: u32, jurisdictions: &[&str]) -> Vec<PiiEntity> {
    let mut entities = run_layers_1_2(text, page, jurisdictions);
    entities.sort_by_key(|e| e.start);
    entities
}

/// Convenience: run Layers 1+2 only (no LLM). Useful for tests and non-LLM callers.
pub fn run_pipeline_no_llm(text: &str, page: u32, jurisdictions: &[&str]) -> Vec<PiiEntity> {
    let mut entities = run_layers_1_2(text, page, jurisdictions);
    entities.sort_by_key(|e| e.start);
    entities
}

/// Layers 1 and 2: regex + heuristic detection.
fn run_layers_1_2(text: &str, page: u32, jurisdictions: &[&str]) -> Vec<PiiEntity> {
    // Layer 1: Regex-based detection (structured data: emails, phones, SSNs, dates, money)
    let scanner = RegexScanner::new(jurisdictions);
    let mut entities = scanner.scan(text, page);

    // Collect matched ranges from Layer 1 so Layer 2 doesn't overlap
    let existing_ranges: Vec<(usize, usize)> = entities.iter()
        .map(|e| (e.start, e.end))
        .collect();

    // Layer 2: Heuristic name detection (person names in legal documents)
    let heuristic = HeuristicScanner::new();
    let mut name_entities = heuristic.scan(text, page, &existing_ranges);

    entities.append(&mut name_entities);
    entities
}
