//! PDF text extraction for redaction checks.
//!
//! `lopdf::Document::extract_text` ignores ToUnicode CMaps (so it returns junk
//! for most Type0/CID fonts), skips the `'` and `"` operators and never looks
//! inside Form XObjects. That is not good enough for a leak check, so this
//! module does its own walk. It is deliberately generous with spaces: the
//! matcher in `text_match` ignores whitespace, and extra spaces only make it
//! report more matches, never fewer.
//!
//! Known blind spots: text drawn as vector paths or inside images, Type0 fonts
//! with no ToUnicode CMap, and glyph names outside the small table below.

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::{HashMap, HashSet};

const MAX_FORM_DEPTH: usize = 8;

/// Extract text from every page, in page order (index 0 = first page).
pub fn extract_page_texts(doc: &Document) -> Vec<String> {
    doc.get_pages()
        .values()
        .map(|&id| extract_page_text(doc, id))
        .collect()
}

/// Extract text from one page, including Form XObjects it draws.
pub fn extract_page_text(doc: &Document, page_id: ObjectId) -> String {
    let content = match doc.get_page_content(page_id) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let resources = page_resource_chain(doc, page_id);
    let mut out = String::new();
    let mut visited = HashSet::new();
    let mut fonts = FontCache::default();
    walk_content(doc, &content, &resources, &mut out, 0, &mut visited, &mut fonts);
    out
}

/// Resource dictionaries applying to a page, nearest first (page, then inherited).
fn page_resource_chain(doc: &Document, page_id: ObjectId) -> Vec<&Dictionary> {
    let (inline, ids) = doc.get_page_resources(page_id);
    let mut chain = Vec::new();
    if let Some(d) = inline {
        chain.push(d);
    }
    for id in ids {
        if let Ok(d) = doc.get_dictionary(id) {
            chain.push(d);
        }
    }
    chain
}

fn deref<'a>(doc: &'a Document, obj: &'a Object) -> &'a Object {
    match obj {
        Object::Reference(id) => doc.get_object(*id).unwrap_or(obj),
        _ => obj,
    }
}

fn lookup_resource<'a>(
    doc: &'a Document,
    chain: &[&'a Dictionary],
    category: &[u8],
    name: &[u8],
) -> Option<(Option<ObjectId>, &'a Object)> {
    for res in chain {
        let Ok(cat) = res.get(category) else { continue };
        let Ok(cat) = deref(doc, cat).as_dict() else { continue };
        if let Ok(entry) = cat.get(name) {
            let id = entry.as_reference().ok();
            return Some((id, deref(doc, entry)));
        }
    }
    None
}

#[derive(Default)]
struct FontCache {
    by_id: HashMap<ObjectId, std::rc::Rc<FontDecoder>>,
}

fn walk_content(
    doc: &Document,
    content: &[u8],
    resources: &[&Dictionary],
    out: &mut String,
    depth: usize,
    visited: &mut HashSet<ObjectId>,
    fonts: &mut FontCache,
) {
    // Inline images carry binary data the content parser cannot skip; without
    // this, one inline image hides all the text on its page.
    let cleaned = crate::ocr::classify::strip_inline_images(content);
    let Ok(content) = Content::decode(&cleaned) else {
        return;
    };
    let mut font: Option<std::rc::Rc<FontDecoder>> = None;
    for op in &content.operations {
        match op.operator.as_str() {
            "Tf" => {
                font = op
                    .operands
                    .first()
                    .and_then(|o| o.as_name().ok())
                    .and_then(|name| lookup_resource(doc, resources, b"Font", name))
                    .and_then(|(id, obj)| {
                        let dict = obj.as_dict().ok()?;
                        if let Some(id) = id {
                            if let Some(f) = fonts.by_id.get(&id) {
                                return Some(f.clone());
                            }
                            let f = std::rc::Rc::new(FontDecoder::new(doc, dict));
                            fonts.by_id.insert(id, f.clone());
                            Some(f)
                        } else {
                            Some(std::rc::Rc::new(FontDecoder::new(doc, dict)))
                        }
                    });
            }
            "Tj" | "'" | "\"" => {
                if let Some(Object::String(bytes, _)) = op.operands.last() {
                    decode_into(font.as_deref(), bytes, out);
                }
                out.push(' ');
            }
            "TJ" => {
                if let Some(Object::Array(items)) = op.operands.first() {
                    for item in items {
                        match item {
                            Object::String(bytes, _) => decode_into(font.as_deref(), bytes, out),
                            Object::Integer(i) if *i < -200 => out.push(' '),
                            Object::Real(r) if *r < -200.0 => out.push(' '),
                            _ => {}
                        }
                    }
                }
                out.push(' ');
            }
            "Td" | "TD" | "Tm" | "T*" | "BT" => out.push(' '),
            "ET" => out.push('\n'),
            "Do" => {
                if depth >= MAX_FORM_DEPTH {
                    continue;
                }
                let Some(name) = op.operands.first().and_then(|o| o.as_name().ok()) else {
                    continue;
                };
                let Some((id, obj)) = lookup_resource(doc, resources, b"XObject", name) else {
                    continue;
                };
                if let Some(id) = id {
                    if !visited.insert(id) {
                        continue;
                    }
                }
                let Ok(stream) = obj.as_stream() else { continue };
                if stream.dict.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Form") {
                    continue;
                }
                let data = stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone());
                let mut chain: Vec<&Dictionary> = Vec::new();
                if let Ok(r) = stream.dict.get(b"Resources") {
                    if let Ok(d) = deref(doc, r).as_dict() {
                        chain.push(d);
                    }
                }
                chain.extend_from_slice(resources);
                out.push(' ');
                walk_content(doc, &data, &chain, out, depth + 1, visited, fonts);
                out.push(' ');
            }
            _ => {}
        }
    }
}

fn decode_into(font: Option<&FontDecoder>, bytes: &[u8], out: &mut String) {
    match font {
        Some(f) => f.decode(bytes, out),
        None => out.push_str(&Document::decode_text(None, bytes)),
    }
}

// ─── Fonts ───────────────────────────────────────────────────────────────────

struct FontDecoder {
    cmap: Option<ToUnicode>,
    /// Single-byte code → char for simple fonts without ToUnicode.
    simple: Option<[Option<char>; 256]>,
    two_byte: bool,
}

impl FontDecoder {
    fn new(doc: &Document, font: &Dictionary) -> Self {
        let subtype = font.get(b"Subtype").and_then(Object::as_name).unwrap_or(b"");
        let two_byte = subtype == b"Type0";
        let cmap = font
            .get(b"ToUnicode")
            .ok()
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_stream().ok())
            .map(|s| s.decompressed_content().unwrap_or_else(|_| s.content.clone()))
            .and_then(|data| ToUnicode::parse(&data));
        let simple = if two_byte { None } else { Some(simple_encoding(doc, font)) };
        FontDecoder { cmap, simple, two_byte }
    }

    fn decode(&self, bytes: &[u8], out: &mut String) {
        if let Some(cmap) = &self.cmap {
            let mut i = 0;
            while i < bytes.len() {
                let len = cmap.code_len(&bytes[i..], self.two_byte);
                let end = (i + len).min(bytes.len());
                let code = bytes[i..end].iter().fold(0u32, |a, b| (a << 8) | *b as u32);
                match cmap.lookup(len as u8, code) {
                    Some(s) => out.push_str(&s),
                    None => match &self.simple {
                        Some(table) if len == 1 => out.push(table[code as usize].unwrap_or('\u{FFFD}')),
                        _ => out.push('\u{FFFD}'),
                    },
                }
                i = end;
            }
        } else if let Some(table) = &self.simple {
            for b in bytes {
                out.push(table[*b as usize].unwrap_or('\u{FFFD}'));
            }
        } else {
            // Type0 without ToUnicode: codes are glyph ids we cannot map.
            for _ in bytes.chunks(2) {
                out.push('\u{FFFD}');
            }
        }
    }
}

fn simple_encoding(doc: &Document, font: &Dictionary) -> [Option<char>; 256] {
    let mut base = "StandardEncoding".to_string();
    let mut differences: Vec<(u8, Vec<u8>)> = Vec::new();
    if let Ok(enc) = font.get(b"Encoding") {
        match deref(doc, enc) {
            Object::Name(n) => base = String::from_utf8_lossy(n).to_string(),
            Object::Dictionary(d) => {
                if let Ok(n) = d.get(b"BaseEncoding").and_then(Object::as_name) {
                    base = String::from_utf8_lossy(n).to_string();
                }
                if let Ok(arr) = d.get(b"Differences").and_then(Object::as_array) {
                    let mut code: i64 = 0;
                    for item in arr {
                        match item {
                            Object::Integer(i) => code = *i,
                            Object::Name(n) => {
                                if (0..256).contains(&code) {
                                    differences.push((code as u8, n.clone()));
                                }
                                code += 1;
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut table = [None; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        *slot = Document::decode_text(Some(&base), &[i as u8]).chars().next();
    }
    for (code, name) in differences {
        if let Some(c) = glyph_name_to_char(&name) {
            table[code as usize] = Some(c);
        }
    }
    table
}

fn glyph_name_to_char(name: &[u8]) -> Option<char> {
    let s = std::str::from_utf8(name).ok()?;
    let s = s.split('.').next().unwrap_or(s); // "a.sc" → "a"
    if s.chars().count() == 1 {
        return s.chars().next();
    }
    if let Some(hex) = s.strip_prefix("uni") {
        return u32::from_str_radix(hex.get(..4)?, 16).ok().and_then(char::from_u32);
    }
    if let Some(hex) = s.strip_prefix('u') {
        if (4..=6).contains(&hex.len()) {
            if let Ok(v) = u32::from_str_radix(hex, 16) {
                return char::from_u32(v);
            }
        }
    }
    Some(match s {
        "space" | "nbspace" => ' ',
        "zero" => '0',
        "one" => '1',
        "two" => '2',
        "three" => '3',
        "four" => '4',
        "five" => '5',
        "six" => '6',
        "seven" => '7',
        "eight" => '8',
        "nine" => '9',
        "period" => '.',
        "comma" => ',',
        "colon" => ':',
        "semicolon" => ';',
        "hyphen" | "minus" | "endash" | "emdash" => '-',
        "at" => '@',
        "underscore" => '_',
        "slash" => '/',
        "parenleft" => '(',
        "parenright" => ')',
        "plus" => '+',
        "numbersign" => '#',
        "ampersand" => '&',
        "quotesingle" | "quoteright" | "quoteleft" => '\'',
        "quotedbl" | "quotedblleft" | "quotedblright" => '"',
        "fi" => '\u{FB01}',
        "fl" => '\u{FB02}',
        "ff" => '\u{FB00}',
        "ffi" => '\u{FB03}',
        "ffl" => '\u{FB04}',
        _ => return None,
    })
}

// ─── ToUnicode CMaps ─────────────────────────────────────────────────────────

struct ToUnicode {
    /// (byte length, low, high)
    codespace: Vec<(u8, u32, u32)>,
    chars: HashMap<(u8, u32), String>,
    /// (byte length, low, high, destination)
    ranges: Vec<(u8, u32, u32, RangeDst)>,
}

enum RangeDst {
    /// First destination as UTF-16 code units; the last unit is incremented.
    Base(Vec<u16>),
    List(Vec<String>),
}

#[derive(Debug, PartialEq)]
enum Tok {
    Hex(Vec<u8>),
    Word(String),
    Open,
    Close,
}

fn tokenize(data: &[u8]) -> Vec<Tok> {
    let mut toks = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let c = data[i];
        match c {
            b'<' if data.get(i + 1) != Some(&b'<') => {
                let mut hex = Vec::new();
                i += 1;
                while i < data.len() && data[i] != b'>' {
                    if data[i].is_ascii_hexdigit() {
                        hex.push(data[i]);
                    }
                    i += 1;
                }
                if hex.len() % 2 == 1 {
                    hex.push(b'0');
                }
                let bytes = hex
                    .chunks(2)
                    .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap_or("00"), 16).unwrap_or(0))
                    .collect();
                toks.push(Tok::Hex(bytes));
                i += 1;
            }
            b'[' => {
                toks.push(Tok::Open);
                i += 1;
            }
            b']' => {
                toks.push(Tok::Close);
                i += 1;
            }
            b'%' => {
                while i < data.len() && data[i] != b'\n' && data[i] != b'\r' {
                    i += 1;
                }
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let start = i;
                while i < data.len()
                    && !data[i].is_ascii_whitespace()
                    && !matches!(data[i], b'<' | b'>' | b'[' | b']' | b'%')
                {
                    i += 1;
                }
                if i == start {
                    i += 1; // stray '>' etc.
                } else {
                    toks.push(Tok::Word(String::from_utf8_lossy(&data[start..i]).to_string()));
                }
            }
        }
    }
    toks
}

fn be(bytes: &[u8]) -> u32 {
    bytes.iter().take(4).fold(0u32, |a, b| (a << 8) | *b as u32)
}

fn utf16(bytes: &[u8]) -> Vec<u16> {
    bytes
        .chunks(2)
        .map(|p| if p.len() == 2 { u16::from_be_bytes([p[0], p[1]]) } else { p[0] as u16 })
        .collect()
}

impl ToUnicode {
    fn parse(data: &[u8]) -> Option<Self> {
        let toks = tokenize(data);
        let mut cm = ToUnicode { codespace: Vec::new(), chars: HashMap::new(), ranges: Vec::new() };
        let mut i = 0;
        while i < toks.len() {
            match &toks[i] {
                Tok::Word(w) if w == "begincodespacerange" => {
                    i += 1;
                    while i + 1 < toks.len() {
                        match (&toks[i], &toks[i + 1]) {
                            (Tok::Hex(lo), Tok::Hex(hi)) => {
                                cm.codespace.push((lo.len().clamp(1, 4) as u8, be(lo), be(hi)));
                                i += 2;
                            }
                            _ => break,
                        }
                    }
                }
                Tok::Word(w) if w == "beginbfchar" => {
                    i += 1;
                    while i + 1 < toks.len() {
                        match (&toks[i], &toks[i + 1]) {
                            (Tok::Hex(src), Tok::Hex(dst)) => {
                                cm.chars.insert(
                                    (src.len().clamp(1, 4) as u8, be(src)),
                                    String::from_utf16_lossy(&utf16(dst)),
                                );
                                i += 2;
                            }
                            (Tok::Hex(_), Tok::Word(_)) => i += 2, // glyph-name destinations: skip
                            _ => break,
                        }
                    }
                }
                Tok::Word(w) if w == "beginbfrange" => {
                    i += 1;
                    while i + 2 < toks.len() {
                        let (Tok::Hex(lo), Tok::Hex(hi)) = (&toks[i], &toks[i + 1]) else { break };
                        let len = lo.len().clamp(1, 4) as u8;
                        match &toks[i + 2] {
                            Tok::Hex(dst) => {
                                cm.ranges.push((len, be(lo), be(hi), RangeDst::Base(utf16(dst))));
                                i += 3;
                            }
                            Tok::Open => {
                                let mut list = Vec::new();
                                i += 3;
                                while i < toks.len() && toks[i] != Tok::Close {
                                    if let Tok::Hex(d) = &toks[i] {
                                        list.push(String::from_utf16_lossy(&utf16(d)));
                                    }
                                    i += 1;
                                }
                                i += 1;
                                cm.ranges.push((len, be(lo), be(hi), RangeDst::List(list)));
                            }
                            _ => break,
                        }
                    }
                }
                _ => i += 1,
            }
        }
        if cm.chars.is_empty() && cm.ranges.is_empty() {
            None
        } else {
            Some(cm)
        }
    }

    fn code_len(&self, rest: &[u8], two_byte_default: bool) -> usize {
        for &(len, lo, hi) in &self.codespace {
            let len = len as usize;
            if rest.len() >= len {
                let code = be(&rest[..len]);
                if code >= lo && code <= hi {
                    return len;
                }
            }
        }
        if let Some(min) = self.codespace.iter().map(|c| c.0 as usize).min() {
            return min;
        }
        if two_byte_default { 2 } else { 1 }
    }

    fn lookup(&self, len: u8, code: u32) -> Option<String> {
        if let Some(s) = self.chars.get(&(len, code)) {
            return Some(s.clone());
        }
        for (l, lo, hi, dst) in &self.ranges {
            if *l == len && code >= *lo && code <= *hi {
                let off = code - lo;
                return match dst {
                    RangeDst::Base(units) => {
                        let mut u = units.clone();
                        if let Some(last) = u.last_mut() {
                            *last = last.wrapping_add(off as u16);
                        }
                        Some(String::from_utf16_lossy(&u))
                    }
                    RangeDst::List(list) => list.get(off as usize).cloned(),
                };
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bfchar_and_bfrange() {
        let cmap = b"1 begincodespacerange <0000> <FFFF> endcodespacerange
            2 beginbfchar <0003> <0020> <0011> <004A> endbfchar
            2 beginbfrange <0044> <0046> <0061> <0050> <0051> [<006F> <0070>] endbfrange";
        let cm = ToUnicode::parse(cmap).unwrap();
        let f = FontDecoder { cmap: Some(cm), simple: None, two_byte: true };
        let mut out = String::new();
        f.decode(&[0x00, 0x11, 0x00, 0x50, 0x00, 0x03, 0x00, 0x45, 0x00, 0x51], &mut out);
        assert_eq!(out, "Jo bp");
    }

    #[test]
    fn glyph_names() {
        assert_eq!(glyph_name_to_char(b"a"), Some('a'));
        assert_eq!(glyph_name_to_char(b"uni00E9"), Some('é'));
        assert_eq!(glyph_name_to_char(b"at"), Some('@'));
    }
}
