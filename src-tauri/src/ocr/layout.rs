//! From OCR blocks to Qvault's text + coordinate model.
//!
//! QVAC's OCR returns word-to-phrase boxes in pixels of the input image. This
//! module:
//!
//! 1. **Splits phrase boxes into word boxes**, proportionally by character
//!    count. This is an approximation (glyphs have different widths), so a
//!    word box cut out of a phrase is widened by [`SPLIT_MARGIN_CHARS`]
//!    average character widths on each side, clamped to the phrase box. The
//!    error is towards covering a little of the neighbouring word, never
//!    towards leaving part of the target visible.
//! 2. **Groups words into lines** (vertical overlap) in reading order.
//! 3. **Builds the page text and normalises it** for the detection layers:
//!    - a line-wrap hyphen is re-joined (`Kowal-` + `ski` → `Kowalski`; inside
//!      an e-mail / URL / ID the hyphen is kept: `atlas-` + `digital.com` →
//!      `atlas-digital.com`);
//!    - e-mails broken by OCR are repaired: a dropped `@` between a local part
//!      and a domain that sit apart on the line (`maria.santos`
//!      `meridian-consulting.com`), a stray glyph in its place
//!      (`r.fernandes~`), a dropped dot after an initial (`j richardson@`),
//!      spaces after dots or around the `@` (`consulting. com`,
//!      `r.fernandes@ atlas…`), and an address split across two lines;
//!    - a `$` read as `S` or `8` in front of an amount is restored where that
//!      is unambiguous (`S50,000.00`, the mis-grouped `8150,000.00`); after
//!      "Dollars (" an amount without a currency sign gets a `$` inserted
//!      (`Dollars (885,000.00)` → `Dollars ($885,000.00)`), digits unchanged,
//!      so Layer 1 detects it and the box still covers every glyph.
//! 4. **Keeps a map from every character of the normalised text to a box**
//!    on the page (or to no box, for inserted separators). The map is stored
//!    as consecutive segments, so the frontend can draw review highlights and
//!    the exporter can place redaction boxes from the same data.
//!
//! Boxes are stored in page points (origin top-left of the page as
//! displayed), so they do not depend on the DPI the page was rendered at.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use super::RawBlock;
use crate::document::text_match::OcrNeedle;

/// Widen word boxes cut out of a phrase box by this many average character
/// widths on each side.
pub const SPLIT_MARGIN_CHARS: f64 = 1.0;
/// Two words on a line whose gap exceeds this many line heights are treated
/// as separate columns / table cells (joined with a newline, not a space).
pub const COLUMN_GAP_LINES: f64 = 3.0;
/// A gap of at least this many line heights between a local part and a domain
/// is evidence of a dropped `@` (an ordinary word space is ~0.3).
pub const AT_GAP_LINES: f64 = 0.45;

/// A word (or inserted character) box on the page, in points.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WordBox {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
    /// Line number (0-based, reading order); boxes of one match on the same
    /// line are merged into one redaction rectangle.
    pub line: u32,
}

/// A run of the page text that maps to one box (or to none).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Segment {
    pub text: String,
    /// Index into [`PageLayout::boxes`]; `None` for separators.
    pub box_index: Option<u32>,
}

/// OCR result for one page, in Qvault's model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageLayout {
    /// Size of the page as displayed, in points; the boxes are relative to it.
    pub width_pt: f64,
    pub height_pt: f64,
    /// Rendering resolution used for OCR.
    pub dpi: u32,
    /// Number of blocks the OCR engine returned.
    pub blocks: usize,
    /// Normalised page text: the concatenation of all segment texts.
    pub text: String,
    pub boxes: Vec<WordBox>,
    pub segments: Vec<Segment>,
}

impl PageLayout {
    /// Byte range in `text` of each segment.
    pub fn segment_ranges(&self) -> Vec<(usize, usize)> {
        let mut at = 0;
        self.segments
            .iter()
            .map(|s| {
                let r = (at, at + s.text.len());
                at = r.1;
                r
            })
            .collect()
    }

    /// Boxes (deduplicated) of the characters in `text[start..end]`.
    pub fn boxes_for_range(&self, start: usize, end: usize) -> Vec<u32> {
        let mut out = Vec::new();
        for (r, s) in self.segment_ranges().iter().zip(&self.segments) {
            if r.0 < end && start < r.1 {
                if let Some(b) = s.box_index {
                    if !out.contains(&b) {
                        out.push(b);
                    }
                }
            }
        }
        out
    }

    /// Rectangles (in points) covering every occurrence of `needle`: one
    /// rectangle per occurrence and line, the union of its boxes there.
    /// Returns (occurrence index, rect) pairs.
    pub fn occurrence_rects(&self, needle: &OcrNeedle) -> Vec<(u32, [f64; 4])> {
        let mut out = Vec::new();
        for (occ, (s, e)) in needle.find_all(&self.text).into_iter().enumerate() {
            let mut per_line: Vec<(u32, [f64; 4])> = Vec::new();
            for b in self.boxes_for_range(s, e) {
                let wb = self.boxes[b as usize];
                match per_line.iter_mut().find(|(l, _)| *l == wb.line) {
                    Some((_, r)) => {
                        r[0] = r[0].min(wb.x0);
                        r[1] = r[1].min(wb.y0);
                        r[2] = r[2].max(wb.x1);
                        r[3] = r[3].max(wb.y1);
                    }
                    None => per_line.push((wb.line, [wb.x0, wb.y0, wb.x1, wb.y1])),
                }
            }
            out.extend(per_line.into_iter().map(|(_, r)| (occ as u32, r)));
        }
        out
    }

    /// Byte length of the text, for checks.
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }
}

// ─── Step 1: words ───────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct Word {
    text: String,
    /// Pixels: x0, y0, x1, y1.
    b: [f64; 4],
}

fn valid_bbox(b: &[f64; 4]) -> bool {
    b.iter().all(|v| v.is_finite()) && b[2] > b[0] && b[3] > b[1]
}

/// Split each block into words at whitespace, giving each word a share of the
/// block's width proportional to its position in the block's characters.
fn split_words(blocks: &[RawBlock]) -> Vec<Word> {
    let mut out = Vec::new();
    for blk in blocks {
        if !valid_bbox(&blk.bbox) {
            continue;
        }
        let chars: Vec<char> = blk.text.chars().collect();
        let n = chars.len();
        let words: Vec<(usize, usize)> = {
            let mut v = Vec::new();
            let mut i = 0;
            while i < n {
                if chars[i].is_whitespace() {
                    i += 1;
                    continue;
                }
                let s = i;
                while i < n && !chars[i].is_whitespace() {
                    i += 1;
                }
                v.push((s, i));
            }
            v
        };
        if words.is_empty() {
            continue;
        }
        let [x0, y0, x1, y1] = blk.bbox;
        if words.len() == 1 {
            let (s, e) = words[0];
            out.push(Word { text: chars[s..e].iter().collect(), b: blk.bbox });
            continue;
        }
        let w = x1 - x0;
        let per_char = w / n as f64;
        let margin = per_char * SPLIT_MARGIN_CHARS;
        for (s, e) in words {
            let wx0 = (x0 + per_char * s as f64 - margin).max(x0);
            let wx1 = (x0 + per_char * e as f64 + margin).min(x1);
            out.push(Word { text: chars[s..e].iter().collect(), b: [wx0, y0, wx1, y1] });
        }
    }
    out
}

// ─── Step 2: lines ───────────────────────────────────────────────────────────

fn group_lines(mut words: Vec<Word>) -> Vec<Vec<Word>> {
    words.sort_by(|a, b| {
        let ca = (a.b[1] + a.b[3]) / 2.0;
        let cb = (b.b[1] + b.b[3]) / 2.0;
        ca.partial_cmp(&cb).unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut lines: Vec<(f64, f64, Vec<Word>)> = Vec::new(); // (y0, y1, words)
    for w in words {
        let h = w.b[3] - w.b[1];
        let center = (w.b[1] + w.b[3]) / 2.0;
        // Join the most recent line whose band contains this word's centre
        // and overlaps it by at least half the smaller height.
        let fits = |l: &(f64, f64, Vec<Word>)| {
            let overlap = w.b[3].min(l.1) - w.b[1].max(l.0);
            center >= l.0 && center <= l.1 && overlap >= 0.5 * h.min(l.1 - l.0)
        };
        match lines.iter_mut().rev().take(3).find(|l| fits(l)) {
            Some(l) => {
                l.0 = l.0.min(w.b[1]);
                l.1 = l.1.max(w.b[3]);
                l.2.push(w);
            }
            None => lines.push((w.b[1], w.b[3], vec![w])),
        }
    }
    lines.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    lines
        .into_iter()
        .map(|(_, _, mut ws)| {
            ws.sort_by(|a, b| a.b[0].partial_cmp(&b.b[0]).unwrap_or(std::cmp::Ordering::Equal));
            ws
        })
        .collect()
}

// ─── Step 3: text with a character map ───────────────────────────────────────

#[derive(Debug, Clone)]
struct Cell {
    ch: char,
    /// Box index, or None for an inserted separator.
    b: Option<u32>,
    /// For a separator between two words on one line: gap in line heights.
    gap: Option<f64>,
}

struct Builder {
    /// Pixel boxes (x0, y0, x1, y1, line).
    boxes: Vec<([f64; 4], u32)>,
    cells: Vec<Cell>,
}

impl Builder {
    fn sep(ch: char, gap: Option<f64>) -> Cell {
        Cell { ch, b: None, gap }
    }

    /// A box for an inserted character that stands for a glyph between two
    /// boxed cells on the same line (e.g. a dropped `@`).
    fn gap_box(&mut self, before: Option<u32>, after: Option<u32>) -> Option<u32> {
        let (a, la) = self.boxes[before? as usize];
        let (b, lb) = self.boxes[after? as usize];
        if la != lb || b[0] < a[2] {
            return None;
        }
        self.boxes.push(([a[2], a[1].min(b[1]), b[0], a[3].max(b[3])], la));
        Some(self.boxes.len() as u32 - 1)
    }
}

/// Edits to apply to the cell vector (indices into it).
#[derive(Debug, Clone, Copy)]
enum Edit {
    Delete,
    Replace(char),
    /// Insert a character before the cell, mapped to that cell's box.
    InsertBefore(char),
}

fn apply(builder: &mut Builder, mut edits: Vec<(usize, Edit)>) {
    if edits.is_empty() {
        return;
    }
    edits.sort_by_key(|e| e.0);
    edits.dedup_by_key(|e| e.0);
    let mut out = Vec::with_capacity(builder.cells.len());
    let mut it = edits.into_iter().peekable();
    let cells = std::mem::take(&mut builder.cells);
    for (i, c) in cells.iter().enumerate() {
        match it.peek() {
            Some(&(j, e)) if j == i => {
                it.next();
                match e {
                    Edit::Delete => {}
                    Edit::InsertBefore(ch) => {
                        out.push(Cell { ch, b: c.b, gap: None });
                        out.push(c.clone());
                    }
                    Edit::Replace(ch) => {
                        let b = match c.b {
                            Some(b) => Some(b),
                            None => {
                                let before = cells[..i].iter().rev().find_map(|c| c.b);
                                let after = cells[i + 1..].iter().find_map(|c| c.b);
                                builder.gap_box(before, after)
                            }
                        };
                        out.push(Cell { ch, b, gap: None });
                    }
                }
            }
            _ => out.push(c.clone()),
        }
    }
    builder.cells = out;
}

/// The cells as a string plus, for each byte offset, the cell index.
fn cell_string(cells: &[Cell]) -> (String, Vec<usize>) {
    let mut s = String::with_capacity(cells.len());
    let mut at = Vec::with_capacity(cells.len());
    for (i, c) in cells.iter().enumerate() {
        s.push(c.ch);
        for _ in 0..c.ch.len_utf8() {
            at.push(i);
        }
    }
    at.push(cells.len());
    (s, at)
}

const TLDS: &str = "com|org|net|edu|gov|mil|int|info|biz|law|legal|io|co|ai|app|dev|eu|us|uk|ca|au|nz|ie|de|at|ch|fr|be|nl|lu|es|pt|br|it|mx|ar|cl|co|pe|jp|in|za";

fn re_missing_at() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(&format!(
            r"(?P<local>[A-Za-z0-9][A-Za-z0-9_%+-]*(?:\. ?[A-Za-z0-9][A-Za-z0-9_%+-]*)*)(?P<junk>[~°©®*]?)(?P<sep> )(?P<domain>[a-z0-9][a-z0-9-]*(?:(?:\. ?|- ?)[a-z0-9][a-z0-9-]*)*\. ?(?:{}))\b",
            TLDS
        ))
        .unwrap()
    })
}

fn re_email() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r"(?:\b(?P<init>[A-Za-z])(?P<isep> ))?(?P<local>[A-Za-z0-9][A-Za-z0-9_%+-]*(?:\. ?[A-Za-z0-9][A-Za-z0-9_%+-]*)*)(?P<pre> )?@(?P<post> )?(?P<domain>[A-Za-z0-9][A-Za-z0-9-]*(?:(?:\. ?|- ?)[A-Za-z0-9][A-Za-z0-9-]*)*\. ?[A-Za-z]{2,24})\b",
        )
        .unwrap()
    })
}

/// Within one line: a dropped (or misread) `@` between a local part and a
/// domain. Needs evidence beyond the words themselves: either a stray glyph
/// where the `@` was, or a gap wider than a word space and a local part that
/// looks like one (contains `.`, `_` or a digit).
fn repair_missing_at(builder: &mut Builder, line_start: usize, line_end: usize) {
    let (s, at) = cell_string(&builder.cells[line_start..line_end]);
    let mut edits = Vec::new();
    for c in re_missing_at().captures_iter(&s) {
        let (local, junk, sep, domain) = (&c["local"], &c["junk"], c.name("sep").unwrap(), &c["domain"]);
        if local.contains('@') || domain.contains('@') {
            continue;
        }
        // The local part must not continue a longer token on its left.
        let start = c.get(0).unwrap().start();
        if s[..start].chars().next_back().map_or(false, |ch| ch == '@' || ch.is_alphanumeric()) {
            continue;
        }
        let sep_cell = line_start + at[sep.start()];
        let gap = builder.cells[sep_cell].gap.unwrap_or(0.0);
        let looks_local = local.contains(['.', '_']) || local.chars().any(|ch| ch.is_ascii_digit());
        if !junk.is_empty() {
            let junk_cell = line_start + at[c.name("junk").unwrap().start()];
            edits.push((junk_cell, Edit::Replace('@')));
            edits.push((sep_cell, Edit::Delete));
        } else if gap >= AT_GAP_LINES && looks_local {
            edits.push((sep_cell, Edit::Replace('@')));
        }
    }
    apply(builder, edits);
}

/// Whole text: tidy e-mail addresses that contain an `@` (spaces after dots
/// and hyphens, a dropped dot after a one-letter initial).
fn repair_emails(builder: &mut Builder) {
    let (s, at) = cell_string(&builder.cells);
    let mut edits = Vec::new();
    for c in re_email().captures_iter(&s) {
        if let (Some(_), Some(isep)) = (c.name("init"), c.name("isep")) {
            let cell = at[isep.start()];
            if builder.cells[cell].b.is_none() && builder.cells[cell].ch == ' ' {
                edits.push((cell, Edit::Replace('.')));
            }
        }
        for part in ["local", "domain"] {
            let m = c.name(part).unwrap();
            for (off, ch) in m.as_str().char_indices() {
                if ch == ' ' {
                    edits.push((at[m.start() + off], Edit::Delete));
                }
            }
        }
        for part in ["pre", "post"] {
            if let Some(m) = c.name(part) {
                if builder.cells[at[m.start()]].b.is_none() {
                    edits.push((at[m.start()], Edit::Delete));
                }
            }
        }
    }
    apply(builder, edits);
}

fn re_amount_s() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // `S50,000.00`: S never starts an amount with decimals.
    R.get_or_init(|| Regex::new(r"(?:^|[\s(\[])(?P<sym>S)\d{1,3}(?:,\d{3})*\.\d{2}\b").unwrap())
}

fn re_amount_8() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    // `8150,000.00`: four digits before the first group separator is not a
    // valid grouping, so the leading 8 is the currency sign.
    R.get_or_init(|| Regex::new(r"(?:^|[\s(\[])(?P<sym>8)\d{3}(?:,\d{3})+(?:\.\d{2})?\b").unwrap())
}

fn re_amount_after_dollars() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r"(?i:\b(?:dollars|d[oó]lares)) ?\((?P<num>\d{1,3}(?:,\d{3})+\.\d{2})\)").unwrap()
    })
}

/// Whole text: restore currency signs OCR misread (see the module docs).
fn repair_currency(builder: &mut Builder) {
    let (s, at) = cell_string(&builder.cells);
    let mut edits = Vec::new();
    for re in [re_amount_s(), re_amount_8()] {
        for c in re.captures_iter(&s) {
            edits.push((at[c.name("sym").unwrap().start()], Edit::Replace('$')));
        }
    }
    apply(builder, edits);
    let (s, at) = cell_string(&builder.cells);
    let edits = re_amount_after_dollars()
        .captures_iter(&s)
        .map(|c| (at[c.name("num").unwrap().start()], Edit::InsertBefore('$')))
        .collect();
    apply(builder, edits);
}

fn last_token(s: &str) -> &str {
    s.rsplit(char::is_whitespace).next().unwrap_or("")
}

/// How to join line `prev` to the next line starting with `next`.
#[derive(Debug, PartialEq)]
enum Join {
    /// Newline.
    Break,
    /// Drop the line-end hyphen and join directly (word hyphenation).
    Dehyphenate,
    /// Join directly, keeping everything (e-mail / URL / ID wrap).
    Direct,
    /// Join with an inserted `@` (local part / domain on two lines).
    At,
}

fn join_rule(prev: &str, next: &str) -> Join {
    let tail = last_token(prev);
    let head = next.split(char::is_whitespace).next().unwrap_or("");
    let head_first = head.chars().next();
    let lower_start = head_first.map_or(false, |c| c.is_lowercase() || c.is_ascii_digit());
    let technical = |t: &str| t.contains(['@', '.', '/', ':']) || t.chars().any(|c| c.is_ascii_digit());
    if tail.ends_with('-') && tail.len() > 1 && lower_start {
        let before = &tail[..tail.len() - 1];
        if technical(before) || technical(head) {
            return Join::Direct;
        }
        if before.chars().last().map_or(false, |c| c.is_alphabetic()) {
            return Join::Dehyphenate;
        }
    }
    // An address cut after the '@' or in its domain, or a line starting with '@'.
    if head.starts_with('@') && tail.chars().last().map_or(false, |c| c.is_alphanumeric()) {
        return Join::Direct;
    }
    if let Some((_, dom)) = tail.rsplit_once('@') {
        if lower_start && (dom.is_empty() || dom.ends_with(['.', '-']) || !dom.contains('.')) {
            return Join::Direct;
        }
    }
    // A local part at the end of a line and a domain at the start of the next.
    if !tail.contains('@') && !head.contains('@') && lower_start {
        let local_ok = {
            let t = tail.trim_start_matches(['(', '<', '[', '"']);
            !t.is_empty()
                && t.contains('.')
                && t.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._%+-".contains(c))
                && !t.ends_with('.')
        };
        if local_ok && re_missing_at().is_match(&format!("{} {}", tail.trim_start_matches(['(', '<', '[', '"']), head)) {
            return Join::At;
        }
    }
    Join::Break
}

/// Build the page layout from OCR blocks.
///
/// `img_w`/`img_h` are the pixel size of the image that was OCR'd and
/// `page_w_pt`/`page_h_pt` the size of the page it shows.
pub fn build_layout(blocks: &[RawBlock], img_w: u32, img_h: u32, page_w_pt: f64, page_h_pt: f64, dpi: u32) -> PageLayout {
    let lines = group_lines(split_words(blocks));
    let mut builder = Builder { boxes: Vec::new(), cells: Vec::new() };
    let mut line_ranges: Vec<(usize, usize)> = Vec::new();

    for (li, words) in lines.iter().enumerate() {
        let mut heights: Vec<f64> = words.iter().map(|w| w.b[3] - w.b[1]).collect();
        heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let line_h = heights[heights.len() / 2].max(1.0);
        let start = builder.cells.len();
        for (wi, w) in words.iter().enumerate() {
            if wi > 0 {
                let gap = (w.b[0] - words[wi - 1].b[2]).max(0.0) / line_h;
                builder.cells.push(Builder::sep(if gap > COLUMN_GAP_LINES { '\n' } else { ' ' }, Some(gap)));
            }
            builder.boxes.push((w.b, li as u32));
            let b = builder.boxes.len() as u32 - 1;
            builder.cells.extend(w.text.chars().map(|ch| Cell { ch, b: Some(b), gap: None }));
        }
        line_ranges.push((start, builder.cells.len()));
    }

    // Per-line repairs that need the gaps (process from the last line so
    // earlier ranges stay valid).
    for &(s, e) in line_ranges.iter().rev() {
        let before = builder.cells.len();
        repair_missing_at(&mut builder, s, e);
        debug_assert!(builder.cells.len() <= before);
    }

    // Join lines. Recompute line extents from the box line numbers.
    let mut joined: Vec<Cell> = Vec::with_capacity(builder.cells.len() + lines.len());
    let mut current_line: Option<u32> = None;
    let mut prev_line_text = String::new();
    let cells = std::mem::take(&mut builder.cells);
    // Split cells back into lines: a boxed cell's line number decides.
    let mut by_line: Vec<Vec<Cell>> = Vec::new();
    for c in cells {
        let l = c.b.map(|b| builder.boxes[b as usize].1);
        match (l, current_line) {
            (Some(l), Some(cur)) if l != cur => {
                by_line.push(vec![c]);
                current_line = Some(l);
            }
            (Some(l), None) => {
                by_line.push(vec![c]);
                current_line = Some(l);
            }
            _ => match by_line.last_mut() {
                Some(v) => v.push(c),
                None => by_line.push(vec![c]),
            },
        }
    }
    for line in by_line {
        let text: String = line.iter().map(|c| c.ch).collect();
        if !joined.is_empty() {
            match join_rule(&prev_line_text, &text) {
                Join::Break => joined.push(Builder::sep('\n', None)),
                Join::Direct => {}
                Join::Dehyphenate => {
                    joined.pop();
                }
                Join::At => joined.push(Builder::sep('@', None)),
            }
        }
        prev_line_text = text;
        joined.extend(line);
    }
    builder.cells = joined;
    repair_emails(&mut builder);
    repair_currency(&mut builder);

    // Segments and boxes in points.
    let sx = page_w_pt / img_w.max(1) as f64;
    let sy = page_h_pt / img_h.max(1) as f64;
    let boxes: Vec<WordBox> = builder
        .boxes
        .iter()
        .map(|(b, l)| WordBox { x0: b[0] * sx, y0: b[1] * sy, x1: b[2] * sx, y1: b[3] * sy, line: *l })
        .collect();
    let mut segments: Vec<Segment> = Vec::new();
    for c in &builder.cells {
        match segments.last_mut() {
            Some(s) if s.box_index == c.b => s.text.push(c.ch),
            _ => segments.push(Segment { text: c.ch.to_string(), box_index: c.b }),
        }
    }
    let text = segments.iter().map(|s| s.text.as_str()).collect();
    PageLayout { width_pt: page_w_pt, height_pt: page_h_pt, dpi, blocks: blocks.len(), text, boxes, segments }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blk(text: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> RawBlock {
        RawBlock { text: text.into(), bbox: [x0, y0, x1, y1], confidence: Some(0.9) }
    }

    /// Layout with page size = image size, so boxes stay in pixels.
    fn layout(blocks: &[RawBlock]) -> PageLayout {
        build_layout(blocks, 1000, 1000, 1000.0, 1000.0, 300)
    }

    #[test]
    fn phrase_boxes_split_proportionally_with_margin() {
        // 10 chars over 100 px: 10 px per char, margin 10 px each side.
        let l = layout(&[blk("ab cd efgh", 0.0, 0.0, 100.0, 20.0)]);
        assert_eq!(l.text, "ab cd efgh");
        assert_eq!(l.boxes.len(), 3);
        assert_eq!((l.boxes[0].x0, l.boxes[0].x1), (0.0, 30.0)); // [0,20] + margin, clamped
        assert_eq!((l.boxes[1].x0, l.boxes[1].x1), (20.0, 60.0)); // [30,50] ± 10
        assert_eq!((l.boxes[2].x0, l.boxes[2].x1), (50.0, 100.0)); // [60,100] − 10, clamped
        // A single-word block keeps its exact box.
        let l = layout(&[blk("Santos", 10.0, 0.0, 70.0, 20.0)]);
        assert_eq!((l.boxes[0].x0, l.boxes[0].x1), (10.0, 70.0));
    }

    #[test]
    fn char_map_covers_text_and_maps_back_to_boxes() {
        let l = layout(&[
            blk("Maria", 300.0, 100.0, 400.0, 130.0),
            blk("represented by", 0.0, 102.0, 280.0, 128.0),
            blk("Santos,", 410.0, 100.0, 520.0, 130.0),
            blk("next line", 0.0, 150.0, 200.0, 180.0),
        ]);
        assert_eq!(l.text, "represented by Maria Santos,\nnext line");
        let concat: String = l.segments.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(concat, l.text);
        let s = l.text.find("Maria Santos").unwrap();
        let boxes = l.boxes_for_range(s, s + "Maria Santos".len());
        assert_eq!(boxes.len(), 2);
        // One rectangle per line, spanning both words.
        let rects = l.occurrence_rects(&OcrNeedle::new("Maria Santos").unwrap());
        assert_eq!(rects, vec![(0, [300.0, 100.0, 520.0, 130.0])]);
        assert!(l.boxes.iter().all(|b| b.line <= 1));
    }

    #[test]
    fn coordinates_scale_from_image_pixels_to_points() {
        // 300 DPI image of a Letter page.
        let l = build_layout(&[blk("Jane", 300.0, 600.0, 450.0, 660.0)], 2550, 3300, 612.0, 792.0, 300);
        let b = l.boxes[0];
        assert!((b.x0 - 72.0).abs() < 1e-9 && (b.y0 - 144.0).abs() < 1e-9);
        assert!((b.x1 - 108.0).abs() < 1e-9 && (b.y1 - 158.4).abs() < 1e-9);
    }

    #[test]
    fn wide_gaps_become_newlines() {
        let l = layout(&[blk("Account", 0.0, 0.0, 100.0, 20.0), blk("7823401956", 400.0, 0.0, 550.0, 20.0)]);
        assert_eq!(l.text, "Account\n7823401956");
    }

    #[test]
    fn line_wrap_hyphenation() {
        let l = layout(&[blk("Elena Kowal-", 0.0, 0.0, 240.0, 20.0), blk("ski, Senior", 0.0, 30.0, 220.0, 50.0)]);
        assert_eq!(l.text, "Elena Kowalski, Senior");
        // The kept characters still map to both halves' boxes.
        let s = l.text.find("Kowalski").unwrap();
        assert_eq!(l.boxes_for_range(s, s + 8).len(), 2);
        // Capitalised continuation: a real hyphenated compound, kept on two lines.
        let l = layout(&[blk("Eighty-", 0.0, 0.0, 140.0, 20.0), blk("Five Dollars", 0.0, 30.0, 240.0, 50.0)]);
        assert_eq!(l.text, "Eighty-\nFive Dollars");
    }

    #[test]
    fn spike_emails_are_repaired() {
        // From the Phase 0 spike / the scanned fixture at 300 DPI (pixel
        // boxes as returned by QVAC OCR).
        let l = build_layout(
            &[
                blk("Officer,", 1575.0, 1198.0, 1719.0, 1252.0),
                blk("email:   j richardson@atlas-", 1741.0, 1197.0, 2254.0, 1254.0),
                blk("digital.com; phone: (212)", 294.0, 1253.0, 762.0, 1314.0),
            ],
            2550,
            3300,
            612.0,
            792.0,
            300,
        );
        assert_eq!(l.text, "Officer, email: j.richardson@atlas-digital.com; phone: (212)");

        // Dropped '@' with a wide gap, and a space after a dot in the domain.
        let l = layout(&[
            blk("maria.santos", 298.0, 948.0, 544.0, 993.0),
            blk("meridian-consulting. com;", 577.0, 941.0, 1052.0, 1002.0),
        ]);
        assert_eq!(l.text, "maria.santos@meridian-consulting.com;");
        // The inserted '@' gets a box over the gap, so redaction covers it.
        let s = l.text.find('@').unwrap();
        assert_eq!(l.boxes_for_range(s, s + 1).len(), 1);

        // Space after a dot in the local part.
        let l = layout(&[
            blk("(elena. kowalski", 295.0, 1996.0, 591.0, 2045.0),
            blk("meridian-consulting.com) .", 622.0, 1991.0, 1102.0, 2048.0),
        ]);
        assert!(l.text.starts_with("(elena.kowalski@meridian-consulting.com)"), "{}", l.text);

        // A stray glyph where the '@' was (200 DPI run).
        let l = layout(&[
            blk("(r.fernandes~", 500.0, 1367.0, 731.0, 1405.0),
            blk("atlas-digital.com).", 752.0, 1369.0, 971.0, 1402.0),
        ]);
        assert_eq!(l.text, "(r.fernandes@atlas-digital.com).");
    }

    #[test]
    fn spaces_around_at_and_after_dots_are_removed() {
        // PDF.js render at 200 DPI: '@' kept but split from the domain, and a
        // space after the dot in the local part.
        let l = layout(&[
            blk("Engineering (r.fernandes@", 197.0, 1363.0, 754.0, 1403.0),
            blk("atlas-digital.com) .", 771.0, 1366.0, 970.0, 1403.0),
        ]);
        assert_eq!(l.text, "Engineering (r.fernandes@atlas-digital.com) .");
        let l = layout(&[
            blk("maria. santos", 199.0, 632.0, 362.0, 662.0),
            blk("meridian-consulting.com; phone:", 385.0, 627.0, 865.0, 667.0),
        ]);
        assert_eq!(l.text, "maria.santos@meridian-consulting.com; phone:");
    }

    #[test]
    fn currency_sign_misreads() {
        let l = layout(&[blk("Dollars (S50,000.00) shall", 0.0, 0.0, 520.0, 30.0)]);
        assert_eq!(l.text, "Dollars ($50,000.00) shall");
        let l = layout(&[blk("Dollars (8150,000.00) for", 0.0, 0.0, 500.0, 30.0)]);
        assert_eq!(l.text, "Dollars ($150,000.00) for");
        // Ambiguous: digits are kept, a sign is inserted (mapped to the
        // number's box, so the burn-in covers the same glyphs).
        let l = layout(&[blk("Dollars (885,000.00) shall", 0.0, 0.0, 520.0, 30.0)]);
        assert_eq!(l.text, "Dollars ($885,000.00) shall");
        let s = l.text.find('$').unwrap();
        assert_eq!(l.boxes_for_range(s, s + 1), l.boxes_for_range(s + 1, s + 2));
        // Ordinary numbers are left alone.
        for t in ["Section 8 applies", "code S12", "8,000 units", "paid 8150 dollars", "(885,000.00) shares"] {
            assert_eq!(layout(&[blk(t, 0.0, 0.0, 400.0, 30.0)]).text, t);
        }
        // Detection now sees the amounts.
        let l = layout(&[blk("Fifty Thousand Dollars (S50,000.00) shall", 0.0, 0.0, 800.0, 30.0)]);
        let found = crate::pipeline::run_pipeline_no_llm(&l.text, 0, &["global", "us"]);
        assert!(found.iter().any(|e| e.text == "$50,000.00"), "{:?}", found);
    }

    #[test]
    fn no_at_is_invented_from_ordinary_words() {
        // Ordinary justified prose with a wide gap: no '@'.
        let l = layout(&[blk("the", 0.0, 0.0, 60.0, 20.0), blk("agreement. it", 80.0, 0.0, 300.0, 20.0)]);
        assert_eq!(l.text, "the agreement. it");
        // Normal word gap before a domain-like token: no '@' either.
        let l = layout(&[blk("visit.us", 0.0, 0.0, 100.0, 20.0), blk("example.com", 104.0, 0.0, 300.0, 20.0)]);
        assert_eq!(l.text, "visit.us example.com");
    }

    #[test]
    fn email_split_across_lines_without_hyphen() {
        let l = layout(&[blk("contact maria.santos@", 0.0, 0.0, 400.0, 20.0), blk("meridian.com today", 0.0, 30.0, 300.0, 50.0)]);
        assert_eq!(l.text, "contact maria.santos@meridian.com today");
        let l = layout(&[blk("contact maria.santos", 0.0, 0.0, 400.0, 20.0), blk("meridian.com today", 0.0, 30.0, 300.0, 50.0)]);
        assert_eq!(l.text, "contact maria.santos@meridian.com today");
    }

    #[test]
    fn detection_layers_see_repaired_text() {
        let l = layout(&[
            blk("email:", 0.0, 0.0, 100.0, 20.0),
            blk("maria.santos", 110.0, 0.0, 300.0, 20.0),
            blk("meridian-consulting. com;", 330.0, 0.0, 700.0, 20.0),
        ]);
        let found = crate::pipeline::run_pipeline_no_llm(&l.text, 0, &["global"]);
        let email = found.iter().find(|e| e.category.to_string() == "Email").expect("email detected");
        assert_eq!(email.text, "maria.santos@meridian-consulting.com");
        // The detection maps back to the three word boxes + the '@' gap box.
        assert_eq!(l.boxes_for_range(email.start, email.end).len(), 4);
    }

    #[test]
    fn empty_and_invalid_blocks_are_ignored() {
        let l = layout(&[blk("   ", 0.0, 0.0, 10.0, 10.0), blk("x", 5.0, 5.0, 5.0, 9.0), blk("ok", f64::NAN, 0.0, 1.0, 1.0)]);
        assert!(l.is_empty());
        assert!(l.segments.is_empty());
    }
}
