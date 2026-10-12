//! Matching of approved redaction strings against extracted text.
//!
//! One matcher is used for three jobs, so they can never disagree:
//! - finding every occurrence of an approved string to redact (DOCX),
//! - counting occurrences on a PDF page before rasterizing it,
//! - the post-export leak check on the exported file.
//!
//! Matching is case-insensitive and ignores all whitespace (and soft hyphens /
//! zero-width characters), because different extractors disagree about where
//! spaces go. To keep short strings from matching inside unrelated words, a
//! match whose first (last) needle character is alphanumeric must not be
//! preceded (followed) by an alphanumeric character in the original text.
//! Extra whitespace in the haystack can only create boundaries, never remove
//! them, so extractors that insert spaces liberally err towards reporting a leak.
//!
//! `src/lib/redaction-match.ts` mirrors this logic for the frontend.

/// A normalized approved string.
#[derive(Debug, Clone)]
pub struct Needle {
    chars: Vec<char>,
    starts_alnum: bool,
    ends_alnum: bool,
}

impl Needle {
    /// Returns `None` if the string has no matchable characters.
    pub fn new(s: &str) -> Option<Self> {
        let mut chars = Vec::new();
        for c in s.chars() {
            fold(c, &mut |f| chars.push(f));
        }
        if chars.is_empty() {
            return None;
        }
        Some(Needle {
            starts_alnum: chars[0].is_alphanumeric(),
            ends_alnum: chars[chars.len() - 1].is_alphanumeric(),
            chars,
        })
    }

    /// The normalized form, usable as a key to group equal needles.
    pub fn key(&self) -> String {
        self.chars.iter().collect()
    }
}

fn is_ignorable(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '\u{00AD}' | '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{2060}' | '\u{FEFF}'
        )
}

/// Case-fold one character (and expand common Latin ligatures), skipping ignorable ones.
fn fold(c: char, out: &mut dyn FnMut(char)) {
    if is_ignorable(c) {
        return;
    }
    let expanded: Option<&str> = match c {
        '\u{FB00}' => Some("ff"),
        '\u{FB01}' => Some("fi"),
        '\u{FB02}' => Some("fl"),
        '\u{FB03}' => Some("ffi"),
        '\u{FB04}' => Some("ffl"),
        '\u{FB05}' | '\u{FB06}' => Some("st"),
        _ => None,
    };
    match expanded {
        Some(s) => s.chars().for_each(|x| out(x)),
        None => c.to_lowercase().for_each(|x| out(x)),
    }
}

/// A haystack folded for matching, remembering where each folded char came from.
pub struct Folded<'a> {
    src: &'a str,
    chars: Vec<char>,
    /// Byte range in `src` of the source character of each folded char.
    pos: Vec<(usize, usize)>,
}

impl<'a> Folded<'a> {
    pub fn new(src: &'a str) -> Self {
        let mut chars = Vec::with_capacity(src.len());
        let mut pos = Vec::with_capacity(src.len());
        for (b, c) in src.char_indices() {
            let e = b + c.len_utf8();
            fold(c, &mut |f| {
                chars.push(f);
                pos.push((b, e));
            });
        }
        Folded { src, chars, pos }
    }

    /// All non-overlapping matches, as byte ranges into the source string.
    pub fn find_all(&self, needle: &Needle) -> Vec<(usize, usize)> {
        let n = &needle.chars;
        let m = n.len();
        let mut out = Vec::new();
        if m == 0 || self.chars.len() < m {
            return out;
        }
        let mut i = 0;
        while i + m <= self.chars.len() {
            if self.chars[i..i + m] == n[..] {
                let start = self.pos[i].0;
                let end = self.pos[i + m - 1].1;
                let before_ok = !needle.starts_alnum
                    || !self.src[..start]
                        .chars()
                        .next_back()
                        .map_or(false, |c| c.is_alphanumeric());
                let after_ok = !needle.ends_alnum
                    || !self.src[end..]
                        .chars()
                        .next()
                        .map_or(false, |c| c.is_alphanumeric());
                if before_ok && after_ok {
                    out.push((start, end));
                    i += m;
                    continue;
                }
            }
            i += 1;
        }
        out
    }

    pub fn count(&self, needle: &Needle) -> usize {
        self.find_all(needle).len()
    }
}

/// Convenience: does `haystack` contain `needle` under the matching rules above?
pub fn contains(haystack: &str, needle: &Needle) -> bool {
    Folded::new(haystack).count(needle) > 0
}

// ─── OCR text ────────────────────────────────────────────────────────────────
//
// OCR text differs from a text layer in ways the strict matcher above does
// not forgive: dropped or swapped punctuation (`maria.santos meridian.com`,
// `Richardson;`), spaces after dots (`consulting. com`). For OCR'd pages we
// therefore also match on letters and digits only. This is used both to place
// boxes on scanned pages and to look for leaks in OCR of the exported pages,
// so it errs towards finding more, never fewer, occurrences.

/// Needles with fewer letters/digits than this use the strict matcher, so
/// that "Ana" or "$5" cannot match noise.
pub const LOOSE_MIN_ALNUM: usize = 4;

/// An approved string reduced to its case-folded letters and digits.
#[derive(Debug, Clone)]
pub struct LooseNeedle {
    chars: Vec<char>,
}

impl LooseNeedle {
    /// `None` when the string has fewer than [`LOOSE_MIN_ALNUM`] letters/digits.
    pub fn new(s: &str) -> Option<Self> {
        let mut chars = Vec::new();
        for c in s.chars() {
            fold(c, &mut |f| {
                if f.is_alphanumeric() {
                    chars.push(f)
                }
            });
        }
        (chars.len() >= LOOSE_MIN_ALNUM).then_some(LooseNeedle { chars })
    }
}

/// A haystack reduced to folded letters/digits, with source positions.
pub struct LooseFolded<'a> {
    src: &'a str,
    chars: Vec<char>,
    pos: Vec<(usize, usize)>,
}

impl<'a> LooseFolded<'a> {
    pub fn new(src: &'a str) -> Self {
        let mut chars = Vec::new();
        let mut pos = Vec::new();
        for (b, c) in src.char_indices() {
            let e = b + c.len_utf8();
            fold(c, &mut |f| {
                if f.is_alphanumeric() {
                    chars.push(f);
                    pos.push((b, e));
                }
            });
        }
        LooseFolded { src, chars, pos }
    }

    /// Non-overlapping matches (byte ranges into the source). A match must not
    /// continue a word: the source characters just outside it must not be
    /// letters or digits.
    pub fn find_all(&self, needle: &LooseNeedle) -> Vec<(usize, usize)> {
        let n = &needle.chars;
        let m = n.len();
        let mut out = Vec::new();
        let mut i = 0;
        while m > 0 && i + m <= self.chars.len() {
            if self.chars[i..i + m] == n[..] {
                let start = self.pos[i].0;
                let end = self.pos[i + m - 1].1;
                let before_ok = !self.src[..start].chars().next_back().map_or(false, |c| c.is_alphanumeric());
                let after_ok = !self.src[end..].chars().next().map_or(false, |c| c.is_alphanumeric());
                if before_ok && after_ok {
                    out.push((start, end));
                    i += m;
                    continue;
                }
            }
            i += 1;
        }
        out
    }
}

/// A needle for OCR text: loose (letters/digits only) when long enough,
/// strict otherwise.
#[derive(Debug, Clone)]
pub enum OcrNeedle {
    Loose(LooseNeedle),
    Strict(Needle),
}

impl OcrNeedle {
    pub fn new(s: &str) -> Option<Self> {
        LooseNeedle::new(s).map(OcrNeedle::Loose).or_else(|| Needle::new(s).map(OcrNeedle::Strict))
    }

    /// Every occurrence in `haystack` (byte ranges). The union of strict and
    /// loose matches is not needed: a strict match of a long needle is always
    /// a loose match too, unless it sits inside a longer word.
    pub fn find_all(&self, haystack: &str) -> Vec<(usize, usize)> {
        match self {
            OcrNeedle::Loose(n) => LooseFolded::new(haystack).find_all(n),
            OcrNeedle::Strict(n) => Folded::new(haystack).find_all(n),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(s: &str) -> Needle {
        Needle::new(s).unwrap()
    }

    #[test]
    fn whitespace_and_case_insensitive() {
        assert!(contains("Client: JOHN\n  smith.", &n("John Smith")));
        assert!(contains("by JohnSmith,", &n("John Smith")));
        assert!(contains("SSN 123 - 45 - 6789", &n("123-45-6789")));
        assert!(contains("of\u{00AD}fice", &n("office")));
        assert!(contains("o\u{FB03}ce", &n("office")));
    }

    #[test]
    fn word_boundaries() {
        assert!(!contains("Banana", &n("Ana")));
        assert!(contains("Ana's", &n("Ana")));
        assert!(contains("Ban ana", &n("Ana")), "inserted spaces only add boundaries");
        assert!(!contains("1234-56789", &n("456")));
        // Needle edges that are not alphanumeric need no boundary.
        assert!(contains("x@a.com", &n("@a.com")));
    }

    #[test]
    fn ranges_map_back_to_source_bytes() {
        let s = "é John  Smith and john smith";
        let f = Folded::new(s);
        let hits = f.find_all(&n("john smith"));
        assert_eq!(hits.len(), 2);
        assert_eq!(&s[hits[0].0..hits[0].1], "John  Smith");
        assert_eq!(&s[hits[1].0..hits[1].1], "john smith");
    }

    #[test]
    fn empty_needles_are_rejected() {
        assert!(Needle::new(" \n\t").is_none());
    }

    #[test]
    fn ocr_matching_ignores_punctuation_but_keeps_word_edges() {
        let n = OcrNeedle::new("j.richardson@atlas-digital.com").unwrap();
        assert!(matches!(n, OcrNeedle::Loose(_)));
        let s = "email: j richardson@atlas- digital.com; phone";
        let hits = n.find_all(s);
        assert_eq!(hits.len(), 1);
        assert_eq!(&s[hits[0].0..hits[0].1], "j richardson@atlas- digital.com");
        // Inside a longer word: no match.
        assert!(n.find_all("xj.richardson@atlas-digital.com").is_empty());
        assert_eq!(OcrNeedle::new("Maria Santos").unwrap().find_all("by Maria Santos; and MARIA-SANTOS").len(), 2);
        assert!(OcrNeedle::new("Maria Santos").unwrap().find_all("Mariana Santos").is_empty());
        // Short needles fall back to the strict matcher.
        let short = OcrNeedle::new("$5").unwrap();
        assert!(matches!(short, OcrNeedle::Strict(_)));
        assert!(short.find_all("pay 5 now").is_empty());
        assert_eq!(short.find_all("pay $5 now").len(), 1);
    }
}
