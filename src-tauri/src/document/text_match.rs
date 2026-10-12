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
}
