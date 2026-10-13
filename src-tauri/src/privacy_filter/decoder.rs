//! Privacy Filter's span decoding: a port of OpenAI's reference code
//! (`opf`, <https://github.com/openai/privacy-filter> at commit
//! `f7f00ca7fb869683eb732c010299d901457f19c3`, Apache-2.0; see
//! ATTRIBUTION.txt):
//!
//! * `ViterbiCRFDecoder` (`opf/_core/decoding.py`): constrained Viterbi over
//!   the BIOES token labels with the six transition biases of
//!   `viterbi_calibration.json`;
//! * `labels_to_spans`, `token_spans_to_char_spans` and
//!   `trim_char_spans_whitespace` (`opf/_core/spans.py`);
//! * `_select_non_overlapping_spans` (`opf/_core/runtime.py`).
//!
//! Arithmetic is f32, as in opf (log-softmax, then the Viterbi recursion), so
//! paths match the reference including tie-breaking (first index wins, like
//! `torch.max`/`argmax`). Offsets are UTF-8 bytes instead of Python
//! characters: a token boundary inside a multi-byte character is widened to
//! the whole character, which is exactly opf's char mapping expressed in bytes.
//! Golden tests: `testdata/viterbi_vectors.json` (generated with opf) and the
//! opt-in `privacy_filter_it` (every eval page through the real model).

use serde::{Deserialize, Serialize};

use super::tokenizer::Token;

/// Span labels of the published model (opf's label space "v2"), in the order
/// of the model's 33 output classes: `O`, then B/I/E/S for each.
pub const SPAN_LABELS: [&str; 8] = [
    "account_number",
    "private_address",
    "private_date",
    "private_email",
    "private_person",
    "private_phone",
    "private_url",
    "secret",
];
pub const NUM_CLASSES: usize = 1 + 4 * SPAN_LABELS.len();

const NEG_INF: f32 = -1e9;

/// Boundary tag of a class: None for background.
fn tag_of(class: usize) -> Option<u8> {
    (class > 0).then(|| [b'B', b'I', b'E', b'S'][(class - 1) % 4])
}

/// Span label of a class (0 = background, 1.. = SPAN_LABELS index + 1).
fn span_of(class: usize) -> usize {
    if class == 0 { 0 } else { (class - 1) / 4 + 1 }
}

/// Class index of the name opf uses ("O", "B-private_person", …).
pub fn class_name(class: usize) -> String {
    match tag_of(class) {
        None => "O".into(),
        Some(t) => format!("{}-{}", t as char, SPAN_LABELS[span_of(class) - 1]),
    }
}

/// The six transition biases (an "operating point").
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Biases {
    pub transition_bias_background_stay: f32,
    pub transition_bias_background_to_start: f32,
    pub transition_bias_inside_to_continue: f32,
    pub transition_bias_inside_to_end: f32,
    pub transition_bias_end_to_background: f32,
    pub transition_bias_end_to_start: f32,
}

impl Biases {
    /// Read `viterbi_calibration.json` with opf's rules: exactly
    /// `{"operating_points": {"default": {"biases": {<the six keys>}}}}`,
    /// all numeric.
    pub fn from_calibration_json(bytes: &[u8]) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| format!("calibration file: {}", e))?;
        let exact = |o: &serde_json::Value, keys: &[&str], what: &str| -> Result<(), String> {
            let obj = o.as_object().ok_or_else(|| format!("{} must be an object", what))?;
            let mut got: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
            got.sort();
            let mut want = keys.to_vec();
            want.sort();
            if got != want {
                return Err(format!("{} must contain exactly {:?}", what, want));
            }
            Ok(())
        };
        exact(&v, &["operating_points"], "calibration file")?;
        let ops = &v["operating_points"];
        exact(ops, &["default"], "operating_points")?;
        exact(&ops["default"], &["biases"], "operating_points.default")?;
        let b = &ops["default"]["biases"];
        const KEYS: [&str; 6] = [
            "transition_bias_background_stay",
            "transition_bias_background_to_start",
            "transition_bias_end_to_background",
            "transition_bias_end_to_start",
            "transition_bias_inside_to_continue",
            "transition_bias_inside_to_end",
        ];
        exact(b, &KEYS, "operating_points.default.biases")?;
        let get = |k: &str| -> Result<f32, String> {
            b[k].as_f64().map(|x| x as f32).ok_or_else(|| format!("{} must be numeric", k))
        };
        Ok(Biases {
            transition_bias_background_stay: get("transition_bias_background_stay")?,
            transition_bias_background_to_start: get("transition_bias_background_to_start")?,
            transition_bias_inside_to_continue: get("transition_bias_inside_to_continue")?,
            transition_bias_inside_to_end: get("transition_bias_inside_to_end")?,
            transition_bias_end_to_background: get("transition_bias_end_to_background")?,
            transition_bias_end_to_start: get("transition_bias_end_to_start")?,
        })
    }

    /// `self` with every field of `delta` added (Qvault's operating point on
    /// top of the published calibration).
    pub fn plus(&self, d: &Biases) -> Biases {
        Biases {
            transition_bias_background_stay: self.transition_bias_background_stay + d.transition_bias_background_stay,
            transition_bias_background_to_start: self.transition_bias_background_to_start + d.transition_bias_background_to_start,
            transition_bias_inside_to_continue: self.transition_bias_inside_to_continue + d.transition_bias_inside_to_continue,
            transition_bias_inside_to_end: self.transition_bias_inside_to_end + d.transition_bias_inside_to_end,
            transition_bias_end_to_background: self.transition_bias_end_to_background + d.transition_bias_end_to_background,
            transition_bias_end_to_start: self.transition_bias_end_to_start + d.transition_bias_end_to_start,
        }
    }
}

/// opf's `ViterbiCRFDecoder` for the 33-class label space.
pub struct Viterbi {
    start: [f32; NUM_CLASSES],
    end: [f32; NUM_CLASSES],
    trans: [[f32; NUM_CLASSES]; NUM_CLASSES],
}

impl Viterbi {
    pub fn new(b: &Biases) -> Self {
        let mut start = [NEG_INF; NUM_CLASSES];
        let mut end = [NEG_INF; NUM_CLASSES];
        let mut trans = [[NEG_INF; NUM_CLASSES]; NUM_CLASSES];
        for i in 0..NUM_CLASSES {
            let tag = tag_of(i);
            if matches!(tag, Some(b'B') | Some(b'S')) || i == 0 {
                start[i] = 0.0;
            }
            if matches!(tag, Some(b'E') | Some(b'S')) || i == 0 {
                end[i] = 0.0;
            }
            for j in 0..NUM_CLASSES {
                if valid_transition(i, j) {
                    trans[i][j] = transition_bias(b, i, j);
                }
            }
        }
        Viterbi { start, end, trans }
    }

    /// Best label path for `[n, NUM_CLASSES]` log-probabilities (row-major).
    pub fn decode(&self, logprobs: &[f32]) -> Vec<usize> {
        let n = logprobs.len() / NUM_CLASSES;
        if n == 0 {
            return vec![];
        }
        let row = |t: usize| &logprobs[t * NUM_CLASSES..(t + 1) * NUM_CLASSES];
        let mut scores = [0f32; NUM_CLASSES];
        for j in 0..NUM_CLASSES {
            scores[j] = row(0)[j] + self.start[j];
        }
        let mut back: Vec<[u8; NUM_CLASSES]> = Vec::with_capacity(n.saturating_sub(1));
        for t in 1..n {
            let mut next = [0f32; NUM_CLASSES];
            let mut bp = [0u8; NUM_CLASSES];
            for j in 0..NUM_CLASSES {
                let mut best = scores[0] + self.trans[0][j];
                let mut arg = 0usize;
                for i in 1..NUM_CLASSES {
                    let s = scores[i] + self.trans[i][j];
                    if s > best {
                        best = s;
                        arg = i;
                    }
                }
                next[j] = best + row(t)[j];
                bp[j] = arg as u8;
            }
            scores = next;
            back.push(bp);
        }
        if !scores.iter().any(|s| s.is_finite()) {
            return argmax(logprobs);
        }
        let mut last = 0usize;
        let mut best = scores[0] + self.end[0];
        for j in 1..NUM_CLASSES {
            let s = scores[j] + self.end[j];
            if s > best {
                best = s;
                last = j;
            }
        }
        let mut path = vec![0usize; n];
        path[n - 1] = last;
        for t in (0..n - 1).rev() {
            last = back[t][last] as usize;
            path[t] = last;
        }
        path
    }
}

fn valid_transition(prev: usize, next: usize) -> bool {
    let (pt, nt) = (tag_of(prev), tag_of(next));
    let next_bg = next == 0;
    if prev == 0 {
        return next_bg || matches!(nt, Some(b'B') | Some(b'S'));
    }
    match pt {
        Some(b'E') | Some(b'S') => next_bg || matches!(nt, Some(b'B') | Some(b'S')),
        Some(b'B') | Some(b'I') => span_of(prev) == span_of(next) && matches!(nt, Some(b'I') | Some(b'E')),
        _ => false,
    }
}

fn transition_bias(b: &Biases, prev: usize, next: usize) -> f32 {
    let (pt, nt) = (tag_of(prev), tag_of(next));
    if prev == 0 {
        if next == 0 {
            return b.transition_bias_background_stay;
        }
        if matches!(nt, Some(b'B') | Some(b'S')) {
            return b.transition_bias_background_to_start;
        }
        return 0.0;
    }
    match pt {
        Some(b'B') | Some(b'I') => {
            if nt == Some(b'I') && span_of(prev) == span_of(next) {
                b.transition_bias_inside_to_continue
            } else if nt == Some(b'E') && span_of(prev) == span_of(next) {
                b.transition_bias_inside_to_end
            } else {
                0.0
            }
        }
        Some(b'E') | Some(b'S') => {
            if next == 0 {
                b.transition_bias_end_to_background
            } else if matches!(nt, Some(b'B') | Some(b'S')) {
                b.transition_bias_end_to_start
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

/// Row-wise log-softmax in f32 (`torch.nn.functional.log_softmax`).
pub fn log_softmax(logits: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(logits.len());
    for row in logits.chunks(NUM_CLASSES) {
        let max = row.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let sum: f32 = row.iter().map(|x| (x - max).exp()).sum();
        let lse = sum.ln();
        out.extend(row.iter().map(|x| x - max - lse));
    }
    out
}

/// Per-token argmax (opf's `argmax` decode mode); first maximum wins.
pub fn argmax(scores: &[f32]) -> Vec<usize> {
    scores
        .chunks(NUM_CLASSES)
        .map(|row| {
            let mut best = 0;
            for j in 1..row.len() {
                if row[j] > row[best] {
                    best = j;
                }
            }
            best
        })
        .collect()
}

/// opf's `labels_to_spans`: (span label index 1.., token start, token end).
pub fn labels_to_spans(labels: &[usize]) -> Vec<(usize, usize, usize)> {
    let mut spans = Vec::new();
    let mut current: Option<(usize, usize)> = None; // (span label, start token)
    for (t, &class) in labels.iter().enumerate() {
        let span = span_of(class);
        if span == 0 {
            if let Some((l, s)) = current.take() {
                spans.push((l, s, t));
            }
            continue;
        }
        match tag_of(class) {
            Some(b'S') => {
                if let Some((l, s)) = current.take() {
                    spans.push((l, s, t));
                }
                spans.push((span, t, t + 1));
            }
            Some(b'B') => {
                if let Some((l, s)) = current.take() {
                    spans.push((l, s, t));
                }
                current = Some((span, t));
            }
            Some(b'I') => match current {
                Some((l, _)) if l == span => {}
                _ => {
                    if let Some((l, s)) = current.take() {
                        spans.push((l, s, t));
                    }
                    current = Some((span, t));
                }
            },
            Some(b'E') => match current.take() {
                Some((l, s)) if l == span => spans.push((l, s, t + 1)),
                other => {
                    if let Some((l, s)) = other {
                        spans.push((l, s, t));
                    }
                    spans.push((span, t, t + 1));
                }
            },
            _ => {}
        }
    }
    if let Some((l, s)) = current {
        spans.push((l, s, labels.len()));
    }
    spans
}

/// Python's `str.isspace` (Rust's `is_whitespace` plus the four information
/// separators U+001C–U+001F, which Python also counts).
fn py_isspace(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// A decoded span: UTF-8 byte offsets into the page text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RawSpan {
    /// Index into [`SPAN_LABELS`].
    pub label: usize,
    pub start: usize,
    pub end: usize,
}

impl RawSpan {
    pub fn label_name(&self) -> &'static str {
        SPAN_LABELS[self.label]
    }
}

fn floor_char(text: &str, mut i: usize) -> usize {
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn ceil_char(text: &str, mut i: usize) -> usize {
    while i < text.len() && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Token label path → spans exactly as opf's `predict_text`: token spans,
/// widened to whole characters, whitespace-trimmed, then the left-to-right
/// non-overlapping selection (longest first at equal start).
pub fn spans_from_labels(text: &str, tokens: &[Token], labels: &[usize]) -> Vec<RawSpan> {
    let mut spans: Vec<RawSpan> = Vec::new();
    for (label, ts, te) in labels_to_spans(labels) {
        if !(ts < te && te <= tokens.len()) {
            continue;
        }
        let start = floor_char(text, tokens[ts].start);
        let end = ceil_char(text, tokens[te - 1].end);
        if end <= start {
            continue;
        }
        // trim whitespace
        let s = &text[start..end];
        let lead: usize = s.chars().take_while(|c| py_isspace(*c)).map(char::len_utf8).sum();
        if lead == s.len() {
            continue;
        }
        let tail: usize = s.chars().rev().take_while(|c| py_isspace(*c)).map(char::len_utf8).sum();
        spans.push(RawSpan { label: label - 1, start: start + lead, end: end - tail });
    }
    // _select_non_overlapping_spans: sort by (start, -len, label name).
    // SPAN_LABELS is in alphabetical order, so the index orders like the name.
    spans.sort_by(|a, b| a.start.cmp(&b.start).then((b.end - b.start).cmp(&(a.end - a.start))).then(a.label.cmp(&b.label)));
    let mut kept = Vec::new();
    let mut cursor = 0;
    for s in spans {
        if s.start < cursor || s.end <= s.start {
            continue;
        }
        cursor = s.end;
        kept.push(s);
    }
    kept
}

/// How token scores become labels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Decode {
    Viterbi(Biases),
    Argmax,
}

/// Logits `[tokens, NUM_CLASSES]` → spans.
pub fn decode(text: &str, tokens: &[Token], logits: &[f32], how: &Decode) -> Vec<RawSpan> {
    assert_eq!(logits.len(), tokens.len() * NUM_CLASSES, "one row of logits per token");
    let lp = log_softmax(logits);
    let labels = match how {
        Decode::Viterbi(b) => Viterbi::new(b).decode(&lp),
        Decode::Argmax => argmax(&lp),
    };
    spans_from_labels(text, tokens, &labels)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_layout_matches_the_model_config() {
        assert_eq!(NUM_CLASSES, 33);
        assert_eq!(class_name(0), "O");
        assert_eq!(class_name(1), "B-account_number");
        assert_eq!(class_name(4), "S-account_number");
        assert_eq!(class_name(17), "B-private_person");
        assert_eq!(class_name(20), "S-private_person");
        assert_eq!(class_name(32), "S-secret");
        let mut sorted = SPAN_LABELS.to_vec();
        sorted.sort();
        assert_eq!(sorted, SPAN_LABELS.to_vec(), "index order must equal name order (span selection tie-break)");
    }

    #[test]
    fn calibration_file_is_validated_like_opf() {
        let published = br#"{"operating_points":{"default":{"biases":{"transition_bias_background_stay":0.0,"transition_bias_background_to_start":0.0,"transition_bias_end_to_background":0.0,"transition_bias_end_to_start":0.0,"transition_bias_inside_to_continue":0.0,"transition_bias_inside_to_end":0.0}}}}"#;
        assert_eq!(Biases::from_calibration_json(published).unwrap(), Biases::default());
        let extra_point = br#"{"operating_points":{"default":{"biases":{}},"recall":{"biases":{}}}}"#;
        assert!(Biases::from_calibration_json(extra_point).is_err());
        let missing_key = br#"{"operating_points":{"default":{"biases":{"transition_bias_background_stay":0.0}}}}"#;
        assert!(Biases::from_calibration_json(missing_key).is_err());
        let not_number = String::from_utf8_lossy(published).replace("\"transition_bias_end_to_start\":0.0", "\"transition_bias_end_to_start\":\"0\"");
        assert!(Biases::from_calibration_json(not_number.as_bytes()).is_err());
    }

    /// The LCG of `gen_viterbi_vectors.py` (see testdata).
    struct Lcg(u64);
    impl Lcg {
        fn next24(&mut self) -> u64 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            self.0 >> 40
        }
    }

    fn lcg_logits(seed: u64, n: usize, background_weight: u64) -> Vec<f32> {
        let mut r = Lcg(seed);
        let mut out = Vec::with_capacity(n * NUM_CLASSES);
        for _ in 0..n {
            let mut row: Vec<f64> = (0..NUM_CLASSES).map(|_| (r.next24() as f64 / 16777216.0) * 8.0 - 4.0).collect();
            if r.next24() % 10 < background_weight {
                row[0] += 6.0;
            }
            out.extend(row.into_iter().map(|x| x as f32));
        }
        out
    }

    #[test]
    fn viterbi_matches_opf_reference_vectors() {
        let v: serde_json::Value = serde_json::from_str(include_str!("testdata/viterbi_vectors.json")).unwrap();
        let classes: Vec<String> = serde_json::from_value(v["classes"].clone()).unwrap();
        for (i, c) in classes.iter().enumerate() {
            assert_eq!(&class_name(i), c);
        }
        let cases = v["cases"].as_array().unwrap();
        assert!(cases.len() >= 100);
        let mut nontrivial = 0;
        for c in cases {
            let seed = c["seed"].as_u64().unwrap();
            let n = c["n"].as_u64().unwrap() as usize;
            let bw = c["background_weight"].as_u64().unwrap();
            let biases: Biases = serde_json::from_value(c["biases"].clone()).unwrap();
            let lp = log_softmax(&lcg_logits(seed, n, bw));
            let path = Viterbi::new(&biases).decode(&lp);
            let want: Vec<usize> = serde_json::from_value(c["path"].clone()).unwrap();
            assert_eq!(path, want, "seed {} biases {:?}", seed, biases);
            let am: Vec<usize> = serde_json::from_value(c["argmax"].clone()).unwrap();
            assert_eq!(argmax(&lp), am, "argmax seed {}", seed);
            // `spans` in the file are opf's labels_to_spans of the Viterbi path.
            let want_spans: Vec<(String, usize, usize)> = serde_json::from_value(c["spans"].clone()).unwrap();
            let vit_spans: Vec<(String, usize, usize)> = labels_to_spans(&path)
                .into_iter()
                .map(|(l, s, e)| (SPAN_LABELS[l - 1].to_string(), s, e))
                .collect();
            assert_eq!(vit_spans, want_spans, "spans seed {}", seed);
            if path != am {
                nontrivial += 1;
            }
        }
        assert!(nontrivial > 20, "the vectors exercise the constraints ({} differ from argmax)", nontrivial);
    }

    #[test]
    fn labels_to_spans_handles_broken_sequences_like_opf() {
        let b = |s: usize| 1 + 4 * s;
        let (i, e, s_) = (|s: usize| 2 + 4 * s, |s: usize| 3 + 4 * s, |s: usize| 4 + 4 * s);
        // B I E | S | O | I(without B) E | E(alone) | B then B
        let labels = vec![b(4), i(4), e(4), s_(1), 0, i(2), e(2), e(3), b(0), b(0), 0];
        assert_eq!(
            labels_to_spans(&labels),
            vec![(5, 0, 3), (2, 3, 4), (3, 5, 7), (4, 7, 8), (1, 8, 9), (1, 9, 10)]
        );
        // unterminated span at the end
        assert_eq!(labels_to_spans(&[0, b(4), i(4)]), vec![(5, 1, 3)]);
    }

    #[test]
    fn byte_spans_widen_split_characters_and_trim() {
        let text = "x São y";
        // tokens: "x", " S", "ã" split in two halves, "o", " y"
        let a = text.find('ã').unwrap();
        let tokens = vec![
            Token { id: 0, start: 0, end: 1 },
            Token { id: 0, start: 1, end: 3 },
            Token { id: 0, start: a, end: a + 1 },
            Token { id: 0, start: a + 1, end: a + 2 },
            Token { id: 0, start: a + 2, end: a + 3 },
            Token { id: 0, start: a + 3, end: text.len() },
        ];
        let person = 4; // private_person
        let b = 1 + 4 * person;
        // span = " S" + first half of "ã" → widened to " Sã", trimmed to "Sã"
        let labels = vec![0, b, b + 2, 0, 0, 0];
        let spans = spans_from_labels(text, &tokens, &labels);
        assert_eq!(spans.len(), 1);
        assert_eq!(&text[spans[0].start..spans[0].end], "Sã");
        assert_eq!(spans[0].label_name(), "private_person");
        // a span made only of whitespace disappears
        let ws = "a   b";
        let toks = vec![Token { id: 0, start: 0, end: 1 }, Token { id: 0, start: 1, end: 4 }, Token { id: 0, start: 4, end: 5 }];
        assert!(spans_from_labels(ws, &toks, &[0, 4, 0]).is_empty());
    }

    #[test]
    fn overlapping_spans_keep_the_leftmost_longest() {
        // Two tokens inside one 3-byte char map to the same char span.
        let text = "€";
        let toks = vec![Token { id: 0, start: 0, end: 1 }, Token { id: 0, start: 1, end: 3 }];
        let s = |sp: usize| 4 + 4 * sp;
        let spans = spans_from_labels(text, &toks, &[s(0), s(4)]);
        assert_eq!(spans, vec![RawSpan { label: 0, start: 0, end: 3 }]);
    }
}
