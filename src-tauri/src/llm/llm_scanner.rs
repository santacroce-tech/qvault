#[cfg(feature = "llm")]
use crate::pipeline::types::{PiiCategory, PiiEntity};

#[cfg(feature = "llm")]
use ndarray::{Array1, Array2, Array3};

/// Labels presented to GLiNER. Mapped back to PiiCategory in `map_category`.
#[cfg(feature = "llm")]
const LABELS: &[&str] = &[
    "person",
    "email",
    "phone number",
    "address",
    "date of birth",
    "company",
    "social security number",
    "bank account",
    "credit card",
    "medical record number",
];

/// Maximum span width (in words). GLiNER large default is 12.
#[cfg(feature = "llm")]
const MAX_WIDTH: usize = 12;

/// Score threshold for accepting a span.
#[cfg(feature = "llm")]
const THRESHOLD: f32 = 0.5;

/// Words per chunk. GLiNER context window is 384 subword tokens; ~150 words is safe.
#[cfg(feature = "llm")]
const MAX_WORDS_PER_CHUNK: usize = 150;
#[cfg(feature = "llm")]
const WORD_OVERLAP: usize = 20;

#[cfg(feature = "llm")]
pub type ProgressFn = Box<dyn Fn(usize, &str) + Send>;

/// Run GLiNER-based PII detection on a page of text.
#[cfg(feature = "llm")]
pub fn scan(
    runtime: &mut super::LlmRuntime,
    text: &str,
    page: u32,
    existing_ranges: &[(usize, usize)],
    on_progress: Option<&ProgressFn>,
) -> Vec<PiiEntity> {
    if text.trim().is_empty() {
        return vec![];
    }

    let words = tokenize_words(text);
    if words.is_empty() {
        return vec![];
    }

    let chunks = chunk_words(&words);
    let mut all_entities: Vec<PiiEntity> = Vec::new();

    for (i, chunk) in chunks.iter().enumerate() {
        if let Some(cb) = &on_progress {
            cb(i, &format!("chunk {}/{}", i + 1, chunks.len()));
        }
        match infer_chunk(runtime, chunk) {
            Ok(spans) => {
                for sp in spans {
                    let start_char = chunk[sp.start_word].start;
                    let end_char = chunk[sp.end_word].end;
                    if start_char >= end_char || end_char > text.len() {
                        continue;
                    }
                    if existing_ranges.iter().any(|(es, ee)| start_char < *ee && *es < end_char) {
                        continue;
                    }
                    if all_entities.iter().any(|e| start_char < e.end && e.start < end_char) {
                        continue;
                    }
                    let category = map_category(sp.label);
                    all_entities.push(PiiEntity {
                        id: uuid::Uuid::new_v4().to_string(),
                        text: text[start_char..end_char].to_string(),
                        start: start_char,
                        end: end_char,
                        page,
                        category: category.clone(),
                        layer: "llm".to_string(),
                        confidence: sp.score as f64,
                        replacement: category.replacement_label().to_string(),
                    });
                }
            }
            Err(e) => log::warn!("GLiNER inference failed for chunk {}: {}", i, e),
        }
    }

    if let Some(cb) = on_progress {
        cb(all_entities.len(), "done");
    }

    all_entities
}

/// A whitespace-split word with its byte offsets in the original text.
#[cfg(feature = "llm")]
#[derive(Debug, Clone)]
struct Word {
    text: String,
    start: usize,
    end: usize,
}

#[cfg(feature = "llm")]
fn tokenize_words(text: &str) -> Vec<Word> {
    let mut words = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if let Some(s) = start.take() {
                words.push(Word {
                    text: text[s..i].to_string(),
                    start: s,
                    end: i,
                });
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        words.push(Word {
            text: text[s..].to_string(),
            start: s,
            end: text.len(),
        });
    }
    words
}

#[cfg(feature = "llm")]
fn chunk_words(words: &[Word]) -> Vec<Vec<Word>> {
    if words.len() <= MAX_WORDS_PER_CHUNK {
        return vec![words.to_vec()];
    }
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < words.len() {
        let end = (start + MAX_WORDS_PER_CHUNK).min(words.len());
        chunks.push(words[start..end].to_vec());
        if end >= words.len() {
            break;
        }
        start = end - WORD_OVERLAP;
    }
    chunks
}

#[cfg(feature = "llm")]
struct DetectedSpan {
    start_word: usize,
    end_word: usize,
    label: &'static str,
    score: f32,
}

/// Build the GLiNER input prompt:
///   <<ENT>> label1 <<ENT>> label2 ... <<SEP>> word1 word2 ... wordN
/// then run the ONNX session and decode span logits.
#[cfg(feature = "llm")]
fn infer_chunk(
    runtime: &mut super::LlmRuntime,
    words: &[Word],
) -> Result<Vec<DetectedSpan>, String> {
    use ort::value::Value;

    // 1) Build the prompt token sequence with explicit per-token alignment.
    //    We tokenize each piece separately so we can build words_mask precisely.
    let mut input_ids: Vec<i64> = Vec::new();
    let mut attention_mask: Vec<i64> = Vec::new();
    let mut words_mask: Vec<i64> = Vec::new(); // 0 for prompt/specials, word_idx+1 for first subword of each word

    let ent_id = special_id(runtime, "<<ENT>>")?;
    let sep_id = special_id(runtime, "<<SEP>>")?;
    let cls_id = special_id(runtime, "[CLS]").unwrap_or(1);
    let sep_eos_id = special_id(runtime, "[SEP]").unwrap_or(2);

    // Leading [CLS]
    input_ids.push(cls_id);
    attention_mask.push(1);
    words_mask.push(0);

    // Entity-type prompt: <<ENT>> label tokens ...
    for label in LABELS {
        input_ids.push(ent_id);
        attention_mask.push(1);
        words_mask.push(0);

        let enc = runtime
            .tokenizer
            .encode(*label, false)
            .map_err(|e| format!("tokenize label failed: {}", e))?;
        for &id in enc.get_ids() {
            input_ids.push(id as i64);
            attention_mask.push(1);
            words_mask.push(0);
        }
    }

    // <<SEP>> separator
    input_ids.push(sep_id);
    attention_mask.push(1);
    words_mask.push(0);

    // Words: tokenize each, mark first subword with (word_idx + 1).
    for (w_idx, w) in words.iter().enumerate() {
        let enc = runtime
            .tokenizer
            .encode(w.text.as_str(), false)
            .map_err(|e| format!("tokenize word failed: {}", e))?;
        let ids = enc.get_ids();
        if ids.is_empty() {
            continue;
        }
        for (k, &id) in ids.iter().enumerate() {
            input_ids.push(id as i64);
            attention_mask.push(1);
            words_mask.push(if k == 0 { (w_idx + 1) as i64 } else { 0 });
        }
    }

    // Trailing [SEP]
    input_ids.push(sep_eos_id);
    attention_mask.push(1);
    words_mask.push(0);

    let seq_len = input_ids.len();
    let num_words = words.len();
    let num_classes = LABELS.len();

    // 2) Build span_idx and span_mask: candidate spans (start, end) over words.
    let mut span_starts: Vec<i64> = Vec::new();
    let mut span_ends: Vec<i64> = Vec::new();
    let mut span_mask: Vec<i64> = Vec::new();
    for s in 0..num_words {
        for w in 0..MAX_WIDTH {
            let e = s + w;
            span_starts.push(s as i64);
            span_ends.push(e as i64);
            span_mask.push(if e < num_words { 1 } else { 0 });
        }
    }
    let num_spans = span_starts.len();

    // 3) Build ndarray inputs. Batch dim = 1.
    let input_ids_arr = Array2::from_shape_vec((1, seq_len), input_ids)
        .map_err(|e| format!("input_ids shape: {}", e))?;
    let attn_arr = Array2::from_shape_vec((1, seq_len), attention_mask)
        .map_err(|e| format!("attention_mask shape: {}", e))?;
    let words_mask_arr = Array2::from_shape_vec((1, seq_len), words_mask)
        .map_err(|e| format!("words_mask shape: {}", e))?;
    let text_lengths_arr = Array2::from_shape_vec((1, 1), vec![num_words as i64])
        .map_err(|e| format!("text_lengths shape: {}", e))?;

    // span_idx shape: [1, num_spans, 2]
    let mut span_idx_flat: Vec<i64> = Vec::with_capacity(num_spans * 2);
    for i in 0..num_spans {
        span_idx_flat.push(span_starts[i]);
        span_idx_flat.push(span_ends[i]);
    }
    let span_idx_arr = ndarray::Array3::from_shape_vec((1, num_spans, 2), span_idx_flat)
        .map_err(|e| format!("span_idx shape: {}", e))?;
    let span_mask_arr = Array2::from_shape_vec((1, num_spans), span_mask)
        .map_err(|e| format!("span_mask shape: {}", e))?;

    // 4) Run inference. Bind by name — input order varies by export.
    let inputs = ort::inputs![
        "input_ids" => Value::from_array(input_ids_arr).map_err(|e| format!("{}", e))?,
        "attention_mask" => Value::from_array(attn_arr).map_err(|e| format!("{}", e))?,
        "words_mask" => Value::from_array(words_mask_arr).map_err(|e| format!("{}", e))?,
        "text_lengths" => Value::from_array(text_lengths_arr).map_err(|e| format!("{}", e))?,
        "span_idx" => Value::from_array(span_idx_arr).map_err(|e| format!("{}", e))?,
        "span_mask" => Value::from_array(span_mask_arr.clone()).map_err(|e| format!("{}", e))?,
    ];

    let outputs = runtime
        .session
        .run(inputs)
        .map_err(|e| format!("ONNX run failed: {}", e))?;

    // 5) Output is logits with shape [1, num_words, MAX_WIDTH, num_classes]
    //    (or [1, num_spans, num_classes] depending on the export).
    let (_, output_value) = outputs
        .iter()
        .next()
        .ok_or_else(|| "no output from session".to_string())?;
    let tensor = output_value
        .try_extract_array::<f32>()
        .map_err(|e| format!("extract logits: {}", e))?;
    let logits = tensor.view();

    let mut detected = Vec::new();
    let shape = logits.shape();
    // Shape A: [1, words, width, classes]
    if shape.len() == 4 && shape[1] == num_words && shape[2] == MAX_WIDTH && shape[3] == num_classes {
        for s in 0..num_words {
            for w in 0..MAX_WIDTH {
                let e = s + w;
                if e >= num_words {
                    continue;
                }
                for c in 0..num_classes {
                    let raw = logits[[0, s, w, c]];
                    let score = sigmoid(raw);
                    if score >= THRESHOLD {
                        detected.push(DetectedSpan {
                            start_word: s,
                            end_word: e,
                            label: LABELS[c],
                            score,
                        });
                    }
                }
            }
        }
    }
    // Shape B: [1, num_spans, num_classes]
    else if shape.len() == 3 && shape[1] == num_spans && shape[2] == num_classes {
        for (i, &m) in span_mask_arr.iter().enumerate() {
            if m == 0 {
                continue;
            }
            let s = span_starts[i] as usize;
            let e = span_ends[i] as usize;
            for c in 0..num_classes {
                let score = sigmoid(logits[[0, i, c]]);
                if score >= THRESHOLD {
                    detected.push(DetectedSpan {
                        start_word: s,
                        end_word: e,
                        label: LABELS[c],
                        score,
                    });
                }
            }
        }
    } else {
        return Err(format!(
            "unexpected logits shape {:?}; expected [1,{},{},{}] or [1,{},{}]",
            shape, num_words, MAX_WIDTH, num_classes, num_spans, num_classes
        ));
    }

    // Greedy non-max suppression: keep highest-scoring span per overlap.
    detected.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let mut kept: Vec<DetectedSpan> = Vec::new();
    for sp in detected {
        let overlaps = kept
            .iter()
            .any(|k| sp.start_word <= k.end_word && k.start_word <= sp.end_word);
        if !overlaps {
            kept.push(sp);
        }
    }
    Ok(kept)
}

#[cfg(feature = "llm")]
fn special_id(runtime: &super::LlmRuntime, tok: &str) -> Result<i64, String> {
    runtime
        .tokenizer
        .token_to_id(tok)
        .map(|id| id as i64)
        .ok_or_else(|| format!("special token {} not in tokenizer vocab", tok))
}

#[cfg(feature = "llm")]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

#[cfg(feature = "llm")]
fn map_category(label: &str) -> PiiCategory {
    match label {
        "person" => PiiCategory::PersonName,
        "email" => PiiCategory::Email,
        "phone number" => PiiCategory::PhoneNumber,
        "address" => PiiCategory::Address,
        "date of birth" => PiiCategory::DateOfBirth,
        "company" => PiiCategory::CompanyName,
        "social security number" => PiiCategory::SocialSecurityNumber,
        "bank account" => PiiCategory::BankAccount,
        "credit card" => PiiCategory::CreditCard,
        "medical record number" => PiiCategory::MedicalRecordNumber,
        _ => PiiCategory::PersonName,
    }
}

// Suppress unused warnings on Array1/Array3 imports if cfg trims them.
#[cfg(feature = "llm")]
#[allow(dead_code)]
fn _type_anchors(_: Array1<i64>, _: Array3<i64>) {}

#[cfg(test)]
mod tests {
    #[cfg(feature = "llm")]
    use super::*;

    #[cfg(feature = "llm")]
    #[test]
    fn test_tokenize_words_basic() {
        let text = "John  Smith lives at 123 Main St.";
        let words = tokenize_words(text);
        assert_eq!(words.len(), 7);
        assert_eq!(words[0].text, "John");
        assert_eq!(&text[words[0].start..words[0].end], "John");
        assert_eq!(words[1].text, "Smith");
        assert_eq!(&text[words[1].start..words[1].end], "Smith");
    }

    #[cfg(feature = "llm")]
    #[test]
    fn test_chunk_words_short() {
        let words = vec![
            Word { text: "a".into(), start: 0, end: 1 },
            Word { text: "b".into(), start: 2, end: 3 },
        ];
        let chunks = chunk_words(&words);
        assert_eq!(chunks.len(), 1);
    }

    #[cfg(feature = "llm")]
    #[test]
    fn test_chunk_words_long() {
        let words: Vec<Word> = (0..400)
            .map(|i| Word {
                text: format!("w{}", i),
                start: i * 4,
                end: i * 4 + 3,
            })
            .collect();
        let chunks = chunk_words(&words);
        assert!(chunks.len() >= 3);
        // Chunks should overlap.
        assert_eq!(
            chunks[0][chunks[0].len() - WORD_OVERLAP].text,
            chunks[1][0].text
        );
    }

    #[cfg(feature = "llm")]
    #[test]
    fn test_sigmoid() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
        assert!(sigmoid(10.0) > 0.99);
        assert!(sigmoid(-10.0) < 0.01);
    }
}
