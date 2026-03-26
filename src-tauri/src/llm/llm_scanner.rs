#[cfg(feature = "llm")]
use crate::pipeline::types::{PiiCategory, PiiEntity};

#[cfg(feature = "llm")]
use candle_core::Tensor;
#[cfg(feature = "llm")]
use candle_transformers::generation::{LogitsProcessor, Sampling};

#[cfg(feature = "llm")]
const SYSTEM_PROMPT: &str = r#"You are a PII detector for legal documents. Find personally identifiable information in the text below that is NOT already marked with [REDACTED ...] tags. Return ONLY a JSON array of objects with "text" (the exact substring from the document) and "category" (one of: PersonName, Email, PhoneNumber, Address, DateOfBirth, CompanyName, SocialSecurityNumber, BankAccount, CreditCard, MedicalRecordNumber). If no additional PII found, return []. Do not include any explanation, only the JSON array."#;

#[cfg(feature = "llm")]
/// Maximum characters per chunk (roughly ~1500 tokens for English).
const MAX_CHUNK_CHARS: usize = 6000;
#[cfg(feature = "llm")]
/// Overlap between chunks to avoid missing entities at boundaries.
const CHUNK_OVERLAP: usize = 500;
#[cfg(feature = "llm")]
/// Max tokens to generate in the response.
const MAX_RESPONSE_TOKENS: usize = 512;

/// Run LLM-based PII detection on a page of text.
/// Returns additional PiiEntity values that do not overlap with `existing_ranges`.
#[cfg(feature = "llm")]
pub fn scan(
    runtime: &mut super::LlmRuntime,
    text: &str,
    page: u32,
    existing_ranges: &[(usize, usize)],
) -> Vec<PiiEntity> {
    if text.trim().is_empty() {
        return vec![];
    }

    let chunks = chunk_text(text);
    let mut all_entities = Vec::new();

    for chunk in &chunks {
        match run_inference(runtime, &chunk.text) {
            Ok(response) => {
                let parsed = parse_llm_response(&response);
                for item in parsed {
                    let entities = map_to_entities(&item, text, page, chunk.offset, existing_ranges, &all_entities);
                    all_entities.extend(entities);
                }
            }
            Err(e) => {
                log::warn!("LLM inference failed for chunk: {}", e);
            }
        }
    }

    all_entities
}

#[cfg(feature = "llm")]
/// A chunk of text with its offset into the original page.
struct TextChunk {
    text: String,
    offset: usize,
}

#[cfg(feature = "llm")]
/// Split text into overlapping chunks if it exceeds MAX_CHUNK_CHARS.
fn chunk_text(text: &str) -> Vec<TextChunk> {
    if text.len() <= MAX_CHUNK_CHARS {
        return vec![TextChunk {
            text: text.to_string(),
            offset: 0,
        }];
    }

    let mut chunks = Vec::new();
    let mut start = 0;

    while start < text.len() {
        let end = (start + MAX_CHUNK_CHARS).min(text.len());
        // Try to break at a word boundary
        let actual_end = if end < text.len() {
            text[..end]
                .rfind(|c: char| c.is_whitespace())
                .unwrap_or(end)
        } else {
            end
        };

        chunks.push(TextChunk {
            text: text[start..actual_end].to_string(),
            offset: start,
        });

        if actual_end >= text.len() {
            break;
        }
        start = actual_end.saturating_sub(CHUNK_OVERLAP);
    }

    chunks
}

/// Run inference using the Qwen2 model with candle.
#[cfg(feature = "llm")]
fn run_inference(
    runtime: &mut super::LlmRuntime,
    text: &str,
) -> Result<String, String> {
    let prompt = format!(
        "<|im_start|>system\n{}<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
        SYSTEM_PROMPT, text
    );

    let tokens = runtime
        .tokenizer
        .encode(prompt.as_str(), true)
        .map_err(|e| format!("Tokenization failed: {}", e))?;
    let input_ids = tokens.get_ids();

    let input = Tensor::new(input_ids, &runtime.device)
        .map_err(|e| format!("Tensor creation failed: {}", e))?
        .unsqueeze(0)
        .map_err(|e| format!("Unsqueeze failed: {}", e))?;

    let logits = runtime
        .model
        .forward(&input, 0)
        .map_err(|e| format!("Forward pass failed: {}", e))?;
    let logits = logits
        .squeeze(0)
        .map_err(|e| format!("Squeeze failed: {}", e))?;

    // Use greedy sampling (temperature=0) for deterministic JSON output
    let mut logits_processor = LogitsProcessor::from_sampling(42, Sampling::ArgMax);

    let mut next_token = logits_processor
        .sample(&logits)
        .map_err(|e| format!("Sampling failed: {}", e))?;

    let eos_token = *runtime
        .tokenizer
        .get_vocab(true)
        .get("<|im_end|>")
        .unwrap_or(&0);

    let mut output_tokens = vec![next_token];

    for index in 0..MAX_RESPONSE_TOKENS {
        if next_token == eos_token {
            break;
        }

        let input = Tensor::new(&[next_token], &runtime.device)
            .map_err(|e| format!("Tensor failed: {}", e))?
            .unsqueeze(0)
            .map_err(|e| format!("Unsqueeze failed: {}", e))?;

        let logits = runtime
            .model
            .forward(&input, input_ids.len() + index)
            .map_err(|e| format!("Forward failed: {}", e))?;
        let logits = logits
            .squeeze(0)
            .map_err(|e| format!("Squeeze failed: {}", e))?;

        next_token = logits_processor
            .sample(&logits)
            .map_err(|e| format!("Sampling failed: {}", e))?;
        output_tokens.push(next_token);
    }

    let output = runtime
        .tokenizer
        .decode(&output_tokens, true)
        .map_err(|e| format!("Decode failed: {}", e))?;

    Ok(output)
}

#[cfg(feature = "llm")]
/// A raw PII item parsed from the LLM's JSON response.
#[derive(Debug)]
struct LlmPiiItem {
    text: String,
    category: String,
}

#[cfg(feature = "llm")]
/// Parse the LLM's JSON response. Tolerant of malformed output.
fn parse_llm_response(response: &str) -> Vec<LlmPiiItem> {
    // Find the JSON array in the response
    let trimmed = response.trim();

    // Try to find array boundaries
    let start = match trimmed.find('[') {
        Some(i) => i,
        None => return vec![],
    };
    let end = match trimmed.rfind(']') {
        Some(i) => i + 1,
        None => return vec![],
    };

    let json_str = &trimmed[start..end];

    // Try parsing as a JSON array
    let parsed: Result<Vec<serde_json::Value>, _> = serde_json::from_str(json_str);
    match parsed {
        Ok(arr) => arr
            .iter()
            .filter_map(|v| {
                let text = v.get("text")?.as_str()?.to_string();
                let category = v.get("category")?.as_str()?.to_string();
                if text.is_empty() {
                    return None;
                }
                Some(LlmPiiItem { text, category })
            })
            .collect(),
        Err(_) => {
            log::warn!("Failed to parse LLM JSON response: {}", json_str);
            vec![]
        }
    }
}

#[cfg(feature = "llm")]
/// Map an LLM category string to PiiCategory.
fn map_category(cat: &str) -> PiiCategory {
    match cat {
        "PersonName" => PiiCategory::PersonName,
        "Email" => PiiCategory::Email,
        "PhoneNumber" => PiiCategory::PhoneNumber,
        "Address" => PiiCategory::Address,
        "DateOfBirth" => PiiCategory::DateOfBirth,
        "CompanyName" => PiiCategory::CompanyName,
        "SocialSecurityNumber" => PiiCategory::SocialSecurityNumber,
        "BankAccount" => PiiCategory::BankAccount,
        "CreditCard" => PiiCategory::CreditCard,
        "MedicalRecordNumber" => PiiCategory::MedicalRecordNumber,
        _ => PiiCategory::PersonName, // fallback
    }
}

#[cfg(feature = "llm")]
/// Check if two ranges overlap.
fn ranges_overlap(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
    a_start < b_end && b_start < a_end
}

#[cfg(feature = "llm")]
/// Map an LlmPiiItem to PiiEntity values by finding it in the original text.
/// Skips matches that overlap with existing ranges or already-found entities.
fn map_to_entities(
    item: &LlmPiiItem,
    full_text: &str,
    page: u32,
    _chunk_offset: usize,
    existing_ranges: &[(usize, usize)],
    already_found: &[PiiEntity],
) -> Vec<PiiEntity> {
    let category = map_category(&item.category);
    let mut entities = Vec::new();

    // Find all occurrences of this text in the full page text
    for (start, _) in full_text.match_indices(&item.text) {
        let end = start + item.text.len();

        // Skip if overlaps with any existing Layer 1/2 detection
        let overlaps_existing = existing_ranges
            .iter()
            .any(|(es, ee)| ranges_overlap(start, end, *es, *ee));
        if overlaps_existing {
            continue;
        }

        // Skip if overlaps with an entity we already found in this LLM pass
        let overlaps_found = already_found
            .iter()
            .any(|e| ranges_overlap(start, end, e.start, e.end));
        if overlaps_found {
            continue;
        }

        // Determine confidence: lower if category was a fallback
        let confidence = if item.category == "PersonName" && category == PiiCategory::PersonName {
            0.70
        } else {
            0.70
        };

        entities.push(PiiEntity {
            id: uuid::Uuid::new_v4().to_string(),
            text: item.text.clone(),
            start,
            end,
            page,
            category: category.clone(),
            layer: "llm".to_string(),
            confidence,
            replacement: category.replacement_label().to_string(),
        });
    }

    entities
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_text_small() {
        let text = "Hello world, this is a small text.";
        let chunks = chunk_text(text);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].offset, 0);
        assert_eq!(chunks[0].text, text);
    }

    #[test]
    fn test_parse_llm_response_valid() {
        let response = r#"[{"text": "John Smith", "category": "PersonName"}, {"text": "123 Elm Street", "category": "Address"}]"#;
        let items = parse_llm_response(response);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].text, "John Smith");
        assert_eq!(items[0].category, "PersonName");
        assert_eq!(items[1].text, "123 Elm Street");
        assert_eq!(items[1].category, "Address");
    }

    #[test]
    fn test_parse_llm_response_empty() {
        let response = "[]";
        let items = parse_llm_response(response);
        assert_eq!(items.len(), 0);
    }

    #[test]
    fn test_parse_llm_response_garbage() {
        let response = "Sorry, I can't help with that.";
        let items = parse_llm_response(response);
        assert_eq!(items.len(), 0);
    }

    #[test]
    fn test_parse_llm_response_with_preamble() {
        let response = r#"Here are the results:
[{"text": "Jane Doe", "category": "PersonName"}]
Hope that helps!"#;
        let items = parse_llm_response(response);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].text, "Jane Doe");
    }

    #[test]
    fn test_map_to_entities_skips_overlapping() {
        let item = LlmPiiItem {
            text: "john@example.com".to_string(),
            category: "Email".to_string(),
        };
        let text = "Contact john@example.com for info.";
        let existing = vec![(8, 24)]; // already detected by regex
        let entities = map_to_entities(&item, text, 0, 0, &existing, &[]);
        assert_eq!(entities.len(), 0); // should be skipped
    }

    #[test]
    fn test_map_to_entities_finds_new() {
        let item = LlmPiiItem {
            text: "John Smith".to_string(),
            category: "PersonName".to_string(),
        };
        let text = "My neighbor John Smith lives nearby.";
        let existing: Vec<(usize, usize)> = vec![];
        let entities = map_to_entities(&item, text, 0, 0, &existing, &[]);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].text, "John Smith");
        assert_eq!(entities[0].layer, "llm");
        assert_eq!(entities[0].confidence, 0.70);
    }

    #[test]
    fn test_ranges_overlap() {
        assert!(ranges_overlap(0, 10, 5, 15));
        assert!(ranges_overlap(5, 15, 0, 10));
        assert!(!ranges_overlap(0, 5, 5, 10));
        assert!(!ranges_overlap(5, 10, 0, 5));
    }
}
