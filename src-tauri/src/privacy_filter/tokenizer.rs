//! o200k tokenizer for OpenAI Privacy Filter, built from the model's own
//! `tokenizer.json` (part of the pinned model files), with exact UTF-8 byte
//! offsets for every token.
//!
//! The model was trained with `tiktoken`'s `o200k_base` encoding, and OpenAI's
//! reference code (`opf`) tokenizes with tiktoken. This is a port of
//! tiktoken's algorithm, so the ids are the same (golden test against
//! tiktoken on every eval page and a set of edge cases):
//!
//! 1. split the text into pieces with o200k's pre-tokenizer regex;
//! 2. a piece that is itself a vocabulary entry becomes one token;
//! 3. otherwise its bytes are merged pairwise, always merging the adjacent
//!    pair whose concatenation has the lowest rank (= token id), until no
//!    pair is in the vocabulary.
//!
//! `tokenizer.json` stores the vocabulary in GPT-2's byte-level alphabet; ids
//! equal tiktoken ranks (checked by the golden test), so the merge list is
//! not needed. Special tokens (`<|endoftext|>`, …) are never produced: text
//! that spells one is tokenized as ordinary text. (opf passes
//! `allowed_special="all"`; a document that literally contains such a marker
//! is the only case where the ids can differ, and treating document text as
//! text is the safer choice.)
//!
//! The regex crate has no look-ahead, so o200k's `\s+(?!\S)|\s+` alternatives
//! are implemented as `\s+` plus the equivalent rule: a run of 2+ whitespace
//! characters (with no line break) that is followed by a non-space character
//! gives its last character to the next piece.

use std::collections::HashMap;
use std::path::Path;

use regex::Regex;

/// Token id with the UTF-8 byte range it covers in the input text. Tokens
/// can split a multi-byte character; the ranges then do not sit on
/// character boundaries (span decoding widens them to whole characters).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub id: u32,
    pub start: usize,
    pub end: usize,
}

pub struct O200kTokenizer {
    ranks: HashMap<Vec<u8>, u32>,
    pre: Regex,
}

/// o200k_base's pre-tokenizer without the look-ahead branch (see module docs).
const PATTERN: &str = concat!(
    r"[^\r\n\p{L}\p{N}]?[\p{Lu}\p{Lt}\p{Lm}\p{Lo}\p{M}]*[\p{Ll}\p{Lm}\p{Lo}\p{M}]+(?i:'s|'t|'re|'ve|'m|'ll|'d)?",
    r"|[^\r\n\p{L}\p{N}]?[\p{Lu}\p{Lt}\p{Lm}\p{Lo}\p{M}]+[\p{Ll}\p{Lm}\p{Lo}\p{M}]*(?i:'s|'t|'re|'ve|'m|'ll|'d)?",
    r"|\p{N}{1,3}",
    r"| ?[^\s\p{L}\p{N}]+[\r\n/]*",
    r"|\s*[\r\n]+",
    r"|\s+",
);

/// GPT-2's byte-level alphabet: byte -> the char tokenizer.json uses for it.
fn byte_to_char_table() -> [char; 256] {
    let mut table = ['\0'; 256];
    let printable = |b: u32| (0x21..=0x7e).contains(&b) || (0xa1..=0xac).contains(&b) || (0xae..=0xff).contains(&b);
    let mut n = 0u32;
    for b in 0u32..256 {
        table[b as usize] = if printable(b) {
            char::from_u32(b).unwrap()
        } else {
            let c = char::from_u32(256 + n).unwrap();
            n += 1;
            c
        };
    }
    table
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenizerError {
    Io(String),
    Format(String),
}

impl std::fmt::Display for TokenizerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenizerError::Io(e) => write!(f, "cannot read tokenizer.json: {}", e),
            TokenizerError::Format(e) => write!(f, "tokenizer.json is not a usable o200k tokenizer: {}", e),
        }
    }
}

impl O200kTokenizer {
    pub fn from_file(path: &Path) -> Result<Self, TokenizerError> {
        let bytes = std::fs::read(path).map_err(|e| TokenizerError::Io(e.kind().to_string()))?;
        Self::from_json(&bytes)
    }

    /// Parse `tokenizer.json` (Hugging Face format, byte-level BPE).
    pub fn from_json(bytes: &[u8]) -> Result<Self, TokenizerError> {
        #[derive(serde::Deserialize)]
        struct Model {
            #[serde(rename = "type")]
            kind: String,
            vocab: HashMap<String, u32>,
        }
        #[derive(serde::Deserialize)]
        struct File {
            model: Model,
        }
        let f: File = serde_json::from_slice(bytes).map_err(|e| TokenizerError::Format(e.to_string()))?;
        if f.model.kind != "BPE" {
            return Err(TokenizerError::Format(format!("model type {}", f.model.kind)));
        }
        let table = byte_to_char_table();
        let mut inverse: HashMap<char, u8> = HashMap::with_capacity(256);
        for (b, c) in table.iter().enumerate() {
            inverse.insert(*c, b as u8);
        }
        let mut ranks = HashMap::with_capacity(f.model.vocab.len());
        for (tok, id) in f.model.vocab {
            let mut bytes = Vec::with_capacity(tok.len());
            for c in tok.chars() {
                match inverse.get(&c) {
                    Some(b) => bytes.push(*b),
                    None => return Err(TokenizerError::Format("vocabulary entry outside the byte-level alphabet".into())),
                }
            }
            ranks.insert(bytes, id);
        }
        // Every single byte must be a token, or some text could not be encoded.
        for b in 0u8..=255 {
            if !ranks.contains_key(&vec![b]) {
                return Err(TokenizerError::Format(format!("byte {:#04x} has no token", b)));
            }
        }
        Ok(Self { ranks, pre: Regex::new(PATTERN).expect("o200k pre-tokenizer regex") })
    }

    pub fn vocab_size(&self) -> usize {
        self.ranks.len()
    }

    /// Pre-tokenizer pieces as byte ranges (see module docs for the
    /// look-ahead rule).
    pub fn pieces(&self, text: &str) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut pos = 0;
        while pos < text.len() {
            let Some(m) = self.pre.find_at(text, pos) else { break };
            // Every character is matched by some branch; a gap would be a bug
            // in the pattern, so keep it as its own piece rather than lose text.
            if m.start() > pos {
                out.push((pos, m.start()));
            }
            let (start, mut end) = (m.start(), m.end());
            let piece = &text[start..end];
            if piece.chars().all(char::is_whitespace) && !piece.contains(['\r', '\n']) {
                let next_is_text = text[end..].chars().next().map(|c| !c.is_whitespace()).unwrap_or(false);
                if next_is_text && piece.chars().count() >= 2 {
                    end -= piece.chars().next_back().unwrap().len_utf8();
                }
            }
            out.push((start, end));
            pos = end;
        }
        out
    }

    pub fn encode(&self, text: &str) -> Vec<Token> {
        let mut out = Vec::new();
        for (s, e) in self.pieces(text) {
            let piece = &text.as_bytes()[s..e];
            if let Some(&id) = self.ranks.get(piece) {
                out.push(Token { id, start: s, end: e });
                continue;
            }
            let parts = self.byte_pair_merge(piece);
            for w in parts.windows(2) {
                let (a, b) = (w[0].0, w[1].0);
                let id = *self.ranks.get(&piece[a..b]).expect("merged part is a token");
                out.push(Token { id, start: s + a, end: s + b });
            }
        }
        out
    }

    /// tiktoken's `_byte_pair_merge`: boundaries of the merged parts.
    fn byte_pair_merge(&self, piece: &[u8]) -> Vec<(usize, u32)> {
        const MAX: u32 = u32::MAX;
        let rank = |a: usize, b: usize| -> u32 { self.ranks.get(&piece[a..b]).copied().unwrap_or(MAX) };
        let mut parts: Vec<(usize, u32)> = Vec::with_capacity(piece.len() + 1);
        let mut min = (MAX, usize::MAX);
        for i in 0..piece.len() - 1 {
            let r = rank(i, i + 2);
            if r < min.0 {
                min = (r, i);
            }
            parts.push((i, r));
        }
        parts.push((piece.len() - 1, MAX));
        parts.push((piece.len(), MAX));
        let get_rank = |parts: &Vec<(usize, u32)>, i: usize| -> u32 {
            if i + 3 < parts.len() {
                rank(parts[i].0, parts[i + 3].0)
            } else {
                MAX
            }
        };
        while min.0 != MAX {
            let i = min.1;
            if i > 0 {
                parts[i - 1].1 = get_rank(&parts, i - 1);
            }
            parts[i].1 = get_rank(&parts, i);
            parts.remove(i + 1);
            min = (MAX, usize::MAX);
            for (j, &(_, r)) in parts[..parts.len() - 1].iter().enumerate() {
                if r < min.0 {
                    min = (r, j);
                }
            }
        }
        parts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny byte-level vocabulary: every byte, plus a few merges.
    fn toy() -> O200kTokenizer {
        let table = byte_to_char_table();
        let mut vocab = serde_json::Map::new();
        for b in 0..256usize {
            vocab.insert(table[b].to_string(), serde_json::json!(b));
        }
        let enc = |s: &str| s.bytes().map(|b| table[b as usize]).collect::<String>();
        for (i, t) in ["ab", "abc", " a", "cd", "bc"].iter().enumerate() {
            vocab.insert(enc(t), serde_json::json!(256 + i));
        }
        let json = serde_json::json!({"model": {"type": "BPE", "vocab": vocab, "merges": []}});
        O200kTokenizer::from_json(json.to_string().as_bytes()).unwrap()
    }

    #[test]
    fn byte_level_alphabet_is_gpt2s() {
        let t = byte_to_char_table();
        assert_eq!(t[b'!' as usize], '!');
        assert_eq!(t[b' ' as usize], 'Ġ');
        assert_eq!(t[b'\n' as usize], 'Ċ');
        assert_eq!(t[0], 'Ā');
        let mut seen: Vec<char> = t.to_vec();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 256);
    }

    #[test]
    fn whole_piece_lookup_and_lowest_rank_merges() {
        let tok = toy();
        // "abcd": no whole-piece entry; pairs ab(256) bc(260) cd(259) → ab
        // first, then "ab"+"c" = abc(257), then abc+d not a token.
        let ids: Vec<u32> = tok.encode("abcd").iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![257, b'd' as u32]);
        // " ab": piece " ab" (space joins the word); " a"(258) vs ab(256): ab wins.
        let t = tok.encode(" ab");
        assert_eq!(t.iter().map(|t| t.id).collect::<Vec<_>>(), vec![b' ' as u32, 256]);
        assert_eq!((t[1].start, t[1].end), (1, 3));
    }

    #[test]
    fn offsets_cover_the_text_exactly() {
        let tok = toy();
        let text = "São  Paulo —\n\n  x  ";
        let toks = tok.encode(text);
        let mut pos = 0;
        for t in &toks {
            assert_eq!(t.start, pos);
            pos = t.end;
        }
        assert_eq!(pos, text.len());
        // 'ã' is two bytes and (in this toy vocabulary) two tokens, splitting the char.
        assert!(toks.iter().any(|t| !text.is_char_boundary(t.end)));
    }

    #[test]
    fn whitespace_lookahead_rule() {
        let tok = toy();
        let pieces = |s: &str| tok.pieces(s).into_iter().map(|(a, b)| s[a..b].to_string()).collect::<Vec<_>>();
        assert_eq!(pieces("a   b"), vec!["a", "  ", " b"]);
        assert_eq!(pieces("a b"), vec!["a", " b"]);
        assert_eq!(pieces("a   "), vec!["a", "   "]);
        assert_eq!(pieces("a \n  b"), vec!["a", " \n", " ", " b"]);
        assert_eq!(pieces("x\t\ty"), vec!["x", "\t", "\ty"]);
        assert_eq!(pieces("1234567"), vec!["123", "456", "7"]);
        assert_eq!(pieces("don't stop"), vec!["don't", " stop"]);
    }

    #[test]
    fn rejects_non_bpe_files() {
        assert!(O200kTokenizer::from_json(br#"{"model":{"type":"WordPiece","vocab":{}}}"#).is_err());
        assert!(O200kTokenizer::from_json(b"not json").is_err());
    }
}
