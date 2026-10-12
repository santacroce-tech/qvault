//! Redacted PDF export.
//!
//! Every page that carries an approved redaction is replaced by a flat image
//! of itself with opaque black boxes burned into the pixels. The page images
//! are rendered by PDF.js in the frontend (it is already bundled for the
//! viewer, so we don't have to ship a pdfium binary per platform), but this
//! module owns everything that matters for safety:
//!
//! - it refuses to export if any approved redaction has no rectangle, or if a
//!   page to be rasterized contains more occurrences of an approved string than
//!   the frontend covered, or if a page that is *not* rasterized contains an
//!   approved string (a likely missed detection the user must review);
//! - it burns the boxes, encodes the image and swaps the page for a page whose
//!   only content is that image (old content streams, fonts, XObjects and
//!   annotations of the page are dropped);
//! - it strips document-wide data that can carry text: Info, XMP metadata,
//!   outlines, forms, name trees (embedded files, JavaScript), open actions,
//!   structure tree, non-link annotations on untouched pages;
//! - after saving, it re-reads the output and fails (deleting the file) if any
//!   approved string is still extractable.
//!
//! Pages without approved redactions keep their original content stream.
//!
//! **Scanned (OCR'd) pages** ([`OcrExport`]): their text exists only as
//! pixels, so the checks above cannot see it. For those pages this module
//! also places the boxes itself, from the stored OCR layout (every
//! occurrence of every approved string, loose OCR matching), refuses if an
//! approved string appears on a scanned page without approved redactions,
//! and after saving runs OCR again on the burned page images it actually
//! wrote, failing the export if any approved string is still recognisable.
//! Nothing is ever added to a page but the image: no OCR text layer.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};
use serde::Deserialize;

use super::pdf_text::{extract_page_text, extract_page_texts};
use super::text_match::{Folded, Needle, OcrNeedle};
use crate::ocr::layout::{build_layout, PageLayout};
use crate::ocr::{OcrFailure, PageOcr};
use crate::storage::database::RedactionRow;

/// A rectangle to black out, in pixel coordinates of the page image
/// (origin top-left).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PixelRect {
    /// The approved redaction whose text this rectangle covers.
    pub redaction_id: String,
    /// Which occurrence (0-based, in page text order) of that redaction's text
    /// on this page the rectangle belongs to. Several rects can share one.
    pub occurrence: u32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A rendered page (PNG or JPEG) plus the rectangles to burn into it.
#[derive(Debug, Clone)]
pub struct PageRaster {
    /// 0-based page index.
    pub page_index: u32,
    pub image: Vec<u8>,
    pub rects: Vec<PixelRect>,
}

#[derive(Debug, Clone)]
pub struct PdfExportSummary {
    pub page_count: usize,
    /// 0-based indexes of pages that were rasterized.
    pub rasterized_pages: Vec<u32>,
    /// 0-based indexes of burned pages that were re-read with OCR.
    pub ocr_verified_pages: Vec<u32>,
}

/// The OCR side of an export.
pub struct OcrExport<'a> {
    /// Stored OCR layouts of the pages that were OCR'd (0-based index).
    pub layouts: BTreeMap<u32, PageLayout>,
    /// Engine used to re-read burned pages. `None` means OCR is not
    /// available now; the export is then refused if a scanned page would
    /// have to be verified.
    pub engine: Option<&'a dyn PageOcr>,
    /// App-private directory for the temporary page images.
    pub work_dir: PathBuf,
    /// Called with (pages verified, pages to verify).
    pub on_progress: Option<&'a (dyn Fn(usize, usize) + Sync)>,
}

const IMAGE_NAME: &[u8] = b"QvRedactedPage";
const JPEG_QUALITY: u8 = 90;

struct Approved<'a> {
    row: &'a RedactionRow,
    key: String,
}

/// Distinct approved strings, keyed by their normalized form.
struct NeedleSet<'a> {
    needles: BTreeMap<String, Needle>,
    /// A category label per key, for error messages (never the text itself).
    category: HashMap<String, &'a str>,
}

fn pages_list(pages: &BTreeSet<u32>) -> String {
    pages.iter().map(|p| (p + 1).to_string()).collect::<Vec<_>>().join(", ")
}

/// Export `pdf_bytes` with `redactions` applied, writing to `output_path`.
///
/// Only rows with status `approved` are considered. On any error nothing is
/// left at `output_path`.
pub fn export_flattened_pdf(
    pdf_bytes: &[u8],
    redactions: &[RedactionRow],
    pages: &[PageRaster],
    output_path: &Path,
) -> Result<PdfExportSummary, String> {
    export_flattened_pdf_ocr(pdf_bytes, redactions, pages, output_path, None)
}

/// [`export_flattened_pdf`] for documents with OCR'd pages.
pub fn export_flattened_pdf_ocr(
    pdf_bytes: &[u8],
    redactions: &[RedactionRow],
    pages: &[PageRaster],
    output_path: &Path,
    ocr: Option<&OcrExport>,
) -> Result<PdfExportSummary, String> {
    let result = export_inner(pdf_bytes, redactions, pages, output_path, ocr);
    if result.is_err() {
        let _ = std::fs::remove_file(output_path);
    }
    result
}

/// Occurrences of one approved string on one OCR'd page.
struct OcrHits {
    /// Number of occurrences in the page's OCR text.
    count: usize,
    /// (occurrence, rectangle in page points) — one per occurrence and line.
    rects: Vec<(u32, [f64; 4])>,
}

fn export_inner(
    pdf_bytes: &[u8],
    redactions: &[RedactionRow],
    pages: &[PageRaster],
    output_path: &Path,
    ocr: Option<&OcrExport>,
) -> Result<PdfExportSummary, String> {
    let mut doc = Document::load_mem(pdf_bytes).map_err(|e| format!("Failed to load PDF: {}", e))?;
    if doc.is_encrypted() {
        doc.decrypt("").map_err(|_| {
            "Export refused: this PDF is password-protected. Remove the password and upload it again."
                .to_string()
        })?;
    }
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let page_count = page_ids.len();

    // ── Approved redactions and their normalized strings ──
    let mut approved: Vec<Approved> = Vec::new();
    let mut set = NeedleSet { needles: BTreeMap::new(), category: HashMap::new() };
    let mut ocr_needles: BTreeMap<String, OcrNeedle> = BTreeMap::new();
    for row in redactions.iter().filter(|r| r.status == "approved") {
        let Some(needle) = Needle::new(&row.original_text) else { continue };
        let key = needle.key();
        set.category.entry(key.clone()).or_insert(row.category.as_str());
        set.needles.entry(key.clone()).or_insert(needle);
        if let Some(n) = OcrNeedle::new(&row.original_text) {
            ocr_needles.entry(key.clone()).or_insert(n);
        }
        approved.push(Approved { row, key });
    }
    let key_of_id: HashMap<&str, &str> =
        approved.iter().map(|a| (a.row.id.as_str(), a.key.as_str())).collect();

    // ── OCR'd pages: every occurrence of every approved string ──
    let no_layouts = BTreeMap::new();
    let layouts = ocr.map(|o| &o.layouts).unwrap_or(&no_layouts);
    if let Some(&p) = layouts.keys().find(|&&p| p as usize >= page_count) {
        return Err(format!("Export failed: OCR data for page {} but the document has {} pages.", p + 1, page_count));
    }
    let mut ocr_hits: BTreeMap<u32, BTreeMap<&str, OcrHits>> = BTreeMap::new();
    for (&idx, layout) in layouts {
        let hits = ocr_hits.entry(idx).or_default();
        for (key, needle) in &ocr_needles {
            let count = needle.find_all(&layout.text).len();
            if count > 0 {
                hits.insert(key.as_str(), OcrHits { count, rects: layout.occurrence_rects(needle) });
            }
        }
    }

    // ── Validate the rasters ──
    let mut rasters: BTreeMap<u32, &PageRaster> = BTreeMap::new();
    for p in pages {
        if p.page_index as usize >= page_count {
            return Err(format!(
                "Export failed: page image for page {} but the document has {} pages.",
                p.page_index + 1,
                page_count
            ));
        }
        if rasters.insert(p.page_index, p).is_some() {
            return Err(format!("Export failed: page {} was rendered twice.", p.page_index + 1));
        }
    }
    let rect_ok = |r: &PixelRect| {
        [r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite()) && r.width > 0.0 && r.height > 0.0
    };

    // ── 1. Every approved redaction must have at least one rect on its page ──
    let mut unmapped: Vec<(i32, &str)> = Vec::new();
    for a in &approved {
        let page = a.row.page.max(0) as u32;
        let covered = rasters.get(&page).map_or(false, |p| {
            a.row.page >= 0
                && (p.rects
                    .iter()
                    .any(|r| rect_ok(r) && key_of_id.get(r.redaction_id.as_str()) == Some(&a.key.as_str()))
                    || ocr_hits
                        .get(&page)
                        .and_then(|h| h.get(a.key.as_str()))
                        .map_or(false, |h| !h.rects.is_empty()))
        });
        if !covered {
            unmapped.push((a.row.page, a.row.id.as_str()));
        }
    }
    if !unmapped.is_empty() {
        let list = unmapped
            .iter()
            .map(|(p, id)| format!("page {} (id {})", p + 1, id))
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Export refused: {} approved redaction(s) could not be located on the page, so they \
             would not be covered: {}. Reject them, or select the text manually in advanced mode, \
             then export again.",
            unmapped.len(),
            list
        ));
    }

    // ── 2. Coverage check against the original text layer ──
    let original_texts: Vec<String> = page_ids.iter().map(|&id| extract_page_text(&doc, id)).collect();
    let mut short_pages = BTreeSet::new();
    let mut short_cats = BTreeSet::new();
    let mut missed_pages = BTreeSet::new();
    let mut missed_cats = BTreeSet::new();
    for (idx, text) in original_texts.iter().enumerate() {
        let folded = Folded::new(text);
        let raster = rasters.get(&(idx as u32));
        for (key, needle) in &set.needles {
            let found = folded.count(needle);
            if found == 0 {
                continue;
            }
            match raster {
                Some(p) => {
                    let covered: BTreeSet<u32> = p
                        .rects
                        .iter()
                        .filter(|r| rect_ok(r) && key_of_id.get(r.redaction_id.as_str()) == Some(&key.as_str()))
                        .map(|r| r.occurrence)
                        .collect();
                    if found > covered.len() {
                        short_pages.insert(idx as u32);
                        short_cats.insert(set.category[key]);
                    }
                }
                None => {
                    missed_pages.insert(idx as u32);
                    missed_cats.insert(set.category[key]);
                }
            }
        }
    }
    // Scanned pages: the boxes come from the OCR layout, so each occurrence
    // needs at least one box there; and a scanned page without approved
    // redactions must not contain approved text either.
    for (&idx, hits) in &ocr_hits {
        for (key, h) in hits {
            if rasters.contains_key(&idx) {
                let placed: BTreeSet<u32> = h.rects.iter().map(|(o, _)| *o).collect();
                if placed.len() < h.count {
                    short_pages.insert(idx);
                    short_cats.insert(set.category[*key]);
                }
            } else {
                missed_pages.insert(idx);
                missed_cats.insert(set.category[*key]);
            }
        }
    }
    if !short_pages.is_empty() {
        return Err(format!(
            "Export refused: page(s) {} contain more occurrences of approved text ({}) than could be \
             located for redaction. Review those pages and redact the remaining occurrences manually, \
             then export again.",
            pages_list(&short_pages),
            short_cats.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if !missed_pages.is_empty() {
        return Err(missed_message(&missed_pages, &missed_cats));
    }

    // ── 3. Rasterize pages with redactions ──
    for (&idx, raster) in &rasters {
        let page_id = page_ids[idx as usize];
        let (w_pt, h_pt) = displayed_size(&doc, page_id);
        let rects: Vec<&PixelRect> = raster
            .rects
            .iter()
            .filter(|r| rect_ok(r) && key_of_id.contains_key(r.redaction_id.as_str()))
            .collect();
        let ocr_rects = match layouts.get(&idx) {
            Some(layout) => ocr_pixel_rects(&raster.image, layout, ocr_hits.get(&idx), idx)?,
            None => Vec::new(),
        };
        let (jpeg, px_w, px_h) = burn_page(&raster.image, &rects, &ocr_rects, w_pt, h_pt, idx)?;
        replace_page_with_image(&mut doc, page_id, jpeg, px_w, px_h, w_pt, h_pt)?;
    }

    // ── 4. Strip document-wide data and annotations on untouched pages ──
    for (idx, &page_id) in page_ids.iter().enumerate() {
        if !rasters.contains_key(&(idx as u32)) {
            sanitize_untouched_page(&mut doc, page_id)?;
        }
    }
    strip_document_level(&mut doc)?;
    doc.prune_objects();

    doc.save(output_path)
        .map_err(|e| format!("Failed to save redacted PDF: {}", e))?;

    // ── 5. Verify the file we actually wrote ──
    let rasterized: BTreeSet<u32> = rasters.keys().copied().collect();
    verify_output(output_path, &set, &rasterized, page_count)?;

    // ── 6. Scanned pages: OCR the burned images we wrote ──
    let to_check: Vec<u32> = rasterized.iter().copied().filter(|p| layouts.contains_key(p)).collect();
    let ocr_verified_pages = match ocr {
        Some(o) if !to_check.is_empty() => verify_with_ocr(output_path, &to_check, layouts, &ocr_needles, &set, o)?,
        _ => Vec::new(),
    };

    Ok(PdfExportSummary { page_count, rasterized_pages: rasterized.into_iter().collect(), ocr_verified_pages })
}

/// Pixel rectangles (x0, y0, x1, y1) for an OCR'd page from its layout.
fn ocr_pixel_rects(
    image: &[u8],
    layout: &PageLayout,
    hits: Option<&BTreeMap<&str, OcrHits>>,
    idx: u32,
) -> Result<Vec<[f64; 4]>, String> {
    let (w, h) = image_dimensions(image)
        .ok_or_else(|| format!("Export failed: could not read the image of page {}.", idx + 1))?;
    if layout.width_pt <= 0.0 || layout.height_pt <= 0.0 {
        return Err(format!("Export failed: bad OCR data for page {}.", idx + 1));
    }
    // The burned page is OCR'd again after export, so it must be rendered
    // at (about) the resolution it was scanned at.
    let dpi = w as f64 / layout.width_pt * 72.0;
    if dpi < layout.dpi as f64 * 0.9 {
        return Err(format!(
            "Export failed: scanned page {} was rendered at {:.0} DPI, below its OCR resolution ({} DPI).",
            idx + 1,
            dpi,
            layout.dpi
        ));
    }
    let sx = w as f64 / layout.width_pt;
    let sy = h as f64 / layout.height_pt;
    Ok(hits
        .into_iter()
        .flat_map(|m| m.values())
        .flat_map(|h| h.rects.iter())
        .map(|(_, r)| [r[0] * sx, r[1] * sy, r[2] * sx, r[3] * sy])
        .collect())
}

fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
}

/// The image of a page we rasterized, as stored in the output (JPEG).
fn stored_page_image(doc: &Document, page_id: ObjectId) -> Option<Vec<u8>> {
    let page = doc.get_dictionary(page_id).ok()?;
    let res = page.get(b"Resources").ok()?.as_dict().ok()?;
    let xo = res.get(b"XObject").ok()?.as_dict().ok()?;
    let id = xo.get(IMAGE_NAME).ok()?.as_reference().ok()?;
    match doc.get_object(id).ok()? {
        Object::Stream(s) if s.dict.get(b"Filter").and_then(Object::as_name).ok() == Some(b"DCTDecode") => {
            Some(s.content.clone())
        }
        _ => None,
    }
}

/// Re-read burned scanned pages with OCR; fail if any approved string is
/// still recognisable. Returns the pages checked.
fn verify_with_ocr(
    path: &Path,
    pages: &[u32],
    layouts: &BTreeMap<u32, PageLayout>,
    needles: &BTreeMap<String, OcrNeedle>,
    set: &NeedleSet,
    ocr: &OcrExport,
) -> Result<Vec<u32>, String> {
    let list: BTreeSet<u32> = pages.iter().copied().collect();
    let engine = ocr.engine.ok_or_else(|| {
        format!(
            "Export refused: scanned page(s) {} must be checked with OCR after redaction, but OCR is not \
             available right now. Check the AI engine and OCR models in Settings, then export again.",
            pages_list(&list)
        )
    })?;
    let doc = Document::load(path).map_err(|e| format!("Export failed: output is not readable: {}", e))?;
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    std::fs::create_dir_all(&ocr.work_dir).map_err(|e| format!("Export failed: OCR work dir: {}", e.kind()))?;
    let mut leak_pages = BTreeSet::new();
    let mut leak_cats = BTreeSet::new();
    for (i, &idx) in pages.iter().enumerate() {
        if let Some(cb) = ocr.on_progress {
            cb(i, pages.len());
        }
        let jpeg = stored_page_image(&doc, page_ids[idx as usize])
            .ok_or_else(|| format!("Export failed: internal check could not read redacted page {}.", idx + 1))?;
        let (w, h) = image_dimensions(&jpeg)
            .ok_or_else(|| format!("Export failed: internal check could not decode redacted page {}.", idx + 1))?;
        let file = ocr.work_dir.join(format!("verify-{}.jpg", uuid::Uuid::new_v4()));
        let result = match write_private(&file, &jpeg) {
            Ok(()) => engine.ocr_image(&file),
            Err(e) => Err(OcrFailure::Failed(e)),
        };
        let _ = std::fs::remove_file(&file);
        let blocks = result.map_err(|e| {
            format!(
                "Export failed: redacted scanned page {} could not be checked with OCR ({}). Nothing was exported.",
                idx + 1,
                e
            )
        })?;
        let dpi = layouts.get(&idx).map(|l| l.dpi).unwrap_or(0);
        let text = build_layout(&blocks, w, h, w as f64, h as f64, dpi).text;
        for (key, needle) in needles {
            if !needle.find_all(&text).is_empty() {
                leak_pages.insert(idx);
                leak_cats.insert(set.category[key]);
            }
        }
    }
    if let Some(cb) = ocr.on_progress {
        cb(pages.len(), pages.len());
    }
    if !leak_pages.is_empty() {
        log::error!("PDF export verification: OCR still reads approved text on page(s) {}", pages_list(&leak_pages));
        return Err(format!(
            "Export failed: approved text ({}) is still readable by OCR on redacted scanned page(s) {}. \
             Nothing was exported. Review those pages (advanced mode lets you add a redaction by \
             selecting the OCR text), then export again.",
            leak_cats.into_iter().collect::<Vec<_>>().join(", "),
            pages_list(&leak_pages)
        ));
    }
    Ok(pages.to_vec())
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)
        .and_then(|mut f| f.write_all(bytes))
        .map_err(|e| format!("cannot write temporary page image: {}", e.kind()))
}

fn missed_message(pages: &BTreeSet<u32>, cats: &BTreeSet<&str>) -> String {
    format!(
        "Export refused: text approved for redaction ({}) also appears on page(s) {}, which have no \
         approved redaction. These are likely missed detections. Review page(s) {}, redact the \
         remaining occurrences (advanced mode lets you select text), then export again.",
        cats.iter().copied().collect::<Vec<_>>().join(", "),
        pages_list(pages),
        pages_list(pages)
    )
}

// ─── Page geometry ───────────────────────────────────────────────────────────

pub(crate) fn inherited<'a>(doc: &'a Document, page_id: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut current = doc.get_dictionary(page_id).ok()?;
    for _ in 0..32 {
        if let Ok(v) = current.get(key) {
            return Some(match v {
                Object::Reference(id) => doc.get_object(*id).ok()?,
                o => o,
            });
        }
        let parent = current.get(b"Parent").and_then(Object::as_reference).ok()?;
        current = doc.get_dictionary(parent).ok()?;
    }
    None
}

pub(crate) fn as_rect(doc: &Document, obj: &Object) -> Option<[f64; 4]> {
    let arr = obj.as_array().ok()?;
    if arr.len() != 4 {
        return None;
    }
    let mut v = [0.0; 4];
    for (i, o) in arr.iter().enumerate() {
        let o = match o {
            Object::Reference(id) => doc.get_object(*id).ok()?,
            o => o,
        };
        v[i] = match o {
            Object::Integer(n) => *n as f64,
            Object::Real(r) => *r as f64,
            _ => return None,
        };
    }
    Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
}

/// Size in points of the page as displayed (crop box, after /Rotate), which is
/// what PDF.js renders at scale 1.
pub fn displayed_size(doc: &Document, page_id: ObjectId) -> (f64, f64) {
    let media = inherited(doc, page_id, b"MediaBox")
        .and_then(|o| as_rect(doc, o))
        .unwrap_or([0.0, 0.0, 612.0, 792.0]);
    let bx = match inherited(doc, page_id, b"CropBox").and_then(|o| as_rect(doc, o)) {
        Some(c) => {
            let i = [c[0].max(media[0]), c[1].max(media[1]), c[2].min(media[2]), c[3].min(media[3])];
            if i[2] > i[0] && i[3] > i[1] { i } else { media }
        }
        None => media,
    };
    let (w, h) = (bx[2] - bx[0], bx[3] - bx[1]);
    let rotate = inherited(doc, page_id, b"Rotate").and_then(|o| o.as_i64().ok()).unwrap_or(0);
    if rotate.rem_euclid(180) == 90 { (h, w) } else { (w, h) }
}

// ─── Burn-in ─────────────────────────────────────────────────────────────────

/// Decode the page image, flatten it onto white, burn opaque black boxes and
/// re-encode as JPEG. Returns (jpeg, width px, height px).
fn burn_page(
    image_bytes: &[u8],
    rects: &[&PixelRect],
    ocr_rects: &[[f64; 4]],
    page_w_pt: f64,
    page_h_pt: f64,
    page_idx: u32,
) -> Result<(Vec<u8>, u32, u32), String> {
    let img = image::load_from_memory(image_bytes)
        .map_err(|e| format!("Export failed: could not decode the image of page {}: {}", page_idx + 1, e))?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return Err(format!("Export failed: empty image for page {}.", page_idx + 1));
    }
    let img_ratio = w as f64 / h as f64;
    let page_ratio = page_w_pt / page_h_pt;
    if (img_ratio / page_ratio - 1.0).abs() > 0.03 {
        return Err(format!(
            "Export failed: the rendered image of page {} ({}x{} px) does not match the page shape.",
            page_idx + 1,
            w,
            h
        ));
    }

    let mut rgb = image::RgbImage::new(w, h);
    for (x, y, p) in rgba.enumerate_pixels() {
        let a = p[3] as u32;
        let blend = |c: u8| ((c as u32 * a + 255 * (255 - a)) / 255) as u8;
        rgb.put_pixel(x, y, image::Rgb([blend(p[0]), blend(p[1]), blend(p[2])]));
    }

    for r in rects {
        // Text spans from PDF.js stop at the baseline and are tight at the
        // ends, so pad more vertically (descenders) than horizontally.
        let pad_x = (r.height * 0.15).max(2.0);
        let pad_y = (r.height * 0.30).max(2.0);
        let x0 = (r.x - pad_x).floor().clamp(0.0, w as f64) as u32;
        let y0 = (r.y - pad_y).floor().clamp(0.0, h as f64) as u32;
        let x1 = (r.x + r.width + pad_x).ceil().clamp(0.0, w as f64) as u32;
        let y1 = (r.y + r.height + pad_y).ceil().clamp(0.0, h as f64) as u32;
        if x0 >= x1 || y0 >= y1 {
            return Err(format!(
                "Export refused: a redaction box on page {} lies outside the page image (id {}).",
                page_idx + 1,
                r.redaction_id
            ));
        }
        for y in y0..y1 {
            for x in x0..x1 {
                rgb.put_pixel(x, y, image::Rgb([0, 0, 0]));
            }
        }
    }

    // OCR boxes already enclose the glyphs (and split word boxes carry a
    // margin), so they get a small uniform pad only.
    for r in ocr_rects {
        if !r.iter().all(|v| v.is_finite()) || r[2] <= r[0] || r[3] <= r[1] {
            return Err(format!("Export refused: a redaction box on scanned page {} is invalid.", page_idx + 1));
        }
        let pad = ((r[3] - r[1]) * 0.08).max(2.0);
        let x0 = (r[0] - pad).floor().clamp(0.0, w as f64) as u32;
        let y0 = (r[1] - pad).floor().clamp(0.0, h as f64) as u32;
        let x1 = (r[2] + pad).ceil().clamp(0.0, w as f64) as u32;
        let y1 = (r[3] + pad).ceil().clamp(0.0, h as f64) as u32;
        if x0 >= x1 || y0 >= y1 {
            return Err(format!(
                "Export refused: a redaction box on scanned page {} lies outside the page image.",
                page_idx + 1
            ));
        }
        for y in y0..y1 {
            for x in x0..x1 {
                rgb.put_pixel(x, y, image::Rgb([0, 0, 0]));
            }
        }
    }

    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, JPEG_QUALITY)
        .encode_image(&rgb)
        .map_err(|e| format!("Export failed: could not encode page {}: {}", page_idx + 1, e))?;
    Ok((jpeg, w, h))
}

/// Replace the page object in place (so references from elsewhere stay valid)
/// with a fresh dictionary whose only content is the image. Nothing from the
/// old page dictionary is carried over except Parent and UserUnit.
fn replace_page_with_image(
    doc: &mut Document,
    page_id: ObjectId,
    jpeg: Vec<u8>,
    px_w: u32,
    px_h: u32,
    w_pt: f64,
    h_pt: f64,
) -> Result<(), String> {
    let old = doc
        .get_dictionary(page_id)
        .map_err(|e| format!("Failed to read page: {}", e))?
        .clone();
    let parent = old.get(b"Parent").map_err(|_| "Page has no parent".to_string())?.clone();

    let image_id = doc.add_object(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => px_w as i64,
            "Height" => px_h as i64,
            "ColorSpace" => "DeviceRGB",
            "BitsPerComponent" => 8,
            "Filter" => "DCTDecode",
        },
        jpeg,
    ));
    let content = Content {
        operations: vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    Object::Real(w_pt as f32),
                    0.into(),
                    0.into(),
                    Object::Real(h_pt as f32),
                    0.into(),
                    0.into(),
                ],
            ),
            Operation::new("Do", vec![Object::Name(IMAGE_NAME.to_vec())]),
            Operation::new("Q", vec![]),
        ],
    };
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        content.encode().map_err(|e| format!("Failed to encode page content: {}", e))?,
    ));
    let bbox = || {
        Object::Array(vec![0.into(), 0.into(), Object::Real(w_pt as f32), Object::Real(h_pt as f32)])
    };
    let mut page = dictionary! {
        "Type" => "Page",
        "Parent" => parent,
        "MediaBox" => bbox(),
        "CropBox" => bbox(),
        "Rotate" => 0,
        "Resources" => dictionary! {
            "XObject" => dictionary! { IMAGE_NAME.to_vec() => Object::Reference(image_id) },
        },
        "Contents" => Object::Reference(content_id),
    };
    if let Ok(u) = old.get(b"UserUnit") {
        page.set("UserUnit", u.clone());
    }
    doc.objects.insert(page_id, Object::Dictionary(page));
    Ok(())
}

// ─── Stripping ───────────────────────────────────────────────────────────────

/// Keep only Link annotations (with a whitelisted, text-free dictionary) and
/// drop page-level metadata. The page content is left alone.
fn sanitize_untouched_page(doc: &mut Document, page_id: ObjectId) -> Result<(), String> {
    let annots: Vec<Object> = {
        let page = doc.get_dictionary(page_id).map_err(|e| format!("Failed to read page: {}", e))?;
        match page.get(b"Annots") {
            Ok(Object::Array(a)) => a.clone(),
            Ok(Object::Reference(id)) => doc
                .get_object(*id)
                .and_then(Object::as_array)
                .cloned()
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    };
    let mut kept = Vec::new();
    for a in annots {
        let dict = match &a {
            Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
            Object::Dictionary(d) => Some(d.clone()),
            _ => None,
        };
        if let Some(clean) = dict.and_then(|d| clean_link_annotation(doc, &d)) {
            kept.push(Object::Reference(doc.add_object(clean)));
        }
    }
    let page = doc
        .get_object_mut(page_id)
        .and_then(Object::as_dict_mut)
        .map_err(|e| format!("Failed to update page: {}", e))?;
    for key in [&b"Metadata"[..], b"PieceInfo", b"AA", b"StructParents", b"Thumb"] {
        page.remove(key);
    }
    if kept.is_empty() {
        page.remove(b"Annots");
    } else {
        page.set("Annots", Object::Array(kept));
    }
    Ok(())
}

fn clean_link_annotation(doc: &Document, d: &Dictionary) -> Option<Dictionary> {
    if d.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Link") {
        return None;
    }
    let mut out = dictionary! { "Type" => "Annot", "Subtype" => "Link" };
    for key in [&b"Rect"[..], b"Border", b"C", b"F", b"H", b"QuadPoints", b"Dest"] {
        if let Ok(v) = d.get(key) {
            out.set(key.to_vec(), v.clone());
        }
    }
    if let Ok(action) = d.get(b"A") {
        let action = match action {
            Object::Reference(id) => doc.get_dictionary(*id).ok()?,
            Object::Dictionary(a) => a,
            _ => return None,
        };
        let kind = action.get(b"S").and_then(Object::as_name).ok()?;
        let mut a = dictionary! { "S" => Object::Name(kind.to_vec()) };
        match kind {
            b"URI" => a.set("URI", action.get(b"URI").ok()?.clone()),
            b"GoTo" => a.set("D", action.get(b"D").ok()?.clone()),
            _ => return None, // Launch, JavaScript, GoToR, SubmitForm, …
        }
        out.set("A", a);
    } else if out.get(b"Dest").is_err() {
        return None;
    }
    out.get(b"Rect").ok()?;
    Some(out)
}

const CATALOG_KEEP: &[&[u8]] = &[
    b"Type",
    b"Pages",
    b"PageLabels",
    b"PageLayout",
    b"PageMode",
    b"ViewerPreferences",
    b"OCProperties",
    b"Version",
];

fn strip_document_level(doc: &mut Document) -> Result<(), String> {
    let root = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|_| "PDF has no catalog".to_string())?;
    let catalog = doc
        .get_object_mut(root)
        .and_then(Object::as_dict_mut)
        .map_err(|e| format!("Failed to read catalog: {}", e))?;
    let keys: Vec<Vec<u8>> = catalog.iter().map(|(k, _)| k.clone()).collect();
    for k in keys {
        if !CATALOG_KEEP.contains(&k.as_slice()) {
            catalog.remove(&k);
        }
    }
    if let Ok(mode) = catalog.get(b"PageMode").and_then(Object::as_name) {
        if !matches!(mode, b"UseNone" | b"UseThumbs" | b"FullScreen") {
            catalog.set("PageMode", "UseNone");
        }
    }
    // XMP packets and private application data can hang off any object
    // (pages, images, fonts), not just the catalog.
    for obj in doc.objects.values_mut() {
        let dict = match obj {
            Object::Dictionary(d) => d,
            Object::Stream(s) => &mut s.dict,
            _ => continue,
        };
        dict.remove(b"Metadata");
        dict.remove(b"PieceInfo");
    }
    // Trailer: keep only what a fresh file needs. Info goes, and so do keys
    // left over from a cross-reference stream or an incremental update.
    let keys: Vec<Vec<u8>> = doc.trailer.iter().map(|(k, _)| k.clone()).collect();
    for k in keys {
        if !matches!(k.as_slice(), b"Root" | b"ID") {
            doc.trailer.remove(&k);
        }
    }
    Ok(())
}

// ─── Verification ────────────────────────────────────────────────────────────

fn verify_output(
    path: &Path,
    set: &NeedleSet,
    rasterized: &BTreeSet<u32>,
    page_count: usize,
) -> Result<(), String> {
    let doc = Document::load(path).map_err(|e| format!("Export failed: output is not readable: {}", e))?;
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    if page_ids.len() != page_count {
        return Err("Export failed: page count changed during export.".into());
    }

    // Structure: document-level carriers are gone.
    let root = doc.trailer.get(b"Root").and_then(Object::as_reference).map_err(|_| "no catalog")?;
    let catalog = doc.get_dictionary(root).map_err(|_| "no catalog")?;
    for key in [&b"Outlines"[..], b"AcroForm", b"Names", b"OpenAction", b"Metadata", b"StructTreeRoot", b"AA"] {
        if catalog.has(key) {
            return Err(format!(
                "Export failed: internal check found /{} in the output.",
                String::from_utf8_lossy(key)
            ));
        }
    }
    if doc.trailer.has(b"Info") {
        return Err("Export failed: internal check found document info in the output.".into());
    }
    for obj in doc.objects.values() {
        let dict = match obj {
            Object::Dictionary(d) => d,
            Object::Stream(s) => &s.dict,
            _ => continue,
        };
        let ty = dict.get(b"Type").and_then(Object::as_name).unwrap_or(b"");
        if matches!(ty, b"EmbeddedFile" | b"Filespec" | b"Metadata") || dict.has(b"JS") {
            return Err("Export failed: internal check found embedded data in the output.".into());
        }
    }

    // Rasterized pages: exactly one image, no fonts, no annotations, no text operators.
    for &idx in rasterized {
        let page_id = page_ids[idx as usize];
        let page = doc.get_dictionary(page_id).map_err(|_| "bad page")?;
        let bad = || format!("Export failed: internal check found leftover content on redacted page {}.", idx + 1);
        if page.has(b"Annots") {
            return Err(bad());
        }
        let res = page.get(b"Resources").and_then(Object::as_dict).map_err(|_| bad())?;
        if res.iter().any(|(k, _)| k.as_slice() != b"XObject") {
            return Err(bad());
        }
        let content = doc.get_page_content(page_id).map_err(|_| bad())?;
        let ops = Content::decode(&content).map_err(|_| bad())?;
        if ops.operations.iter().any(|o| !matches!(o.operator.as_str(), "q" | "Q" | "cm" | "Do")) {
            return Err(bad());
        }
    }

    // Text layer: no approved string anywhere.
    let texts = extract_page_texts(&doc);
    let mut leak_pages = BTreeSet::new();
    let mut missed_pages = BTreeSet::new();
    let mut missed_cats = BTreeSet::new();
    for (idx, text) in texts.iter().enumerate() {
        let folded = Folded::new(text);
        for (key, needle) in &set.needles {
            if folded.count(needle) > 0 {
                if rasterized.contains(&(idx as u32)) {
                    leak_pages.insert(idx as u32);
                } else {
                    missed_pages.insert(idx as u32);
                    missed_cats.insert(set.category[key]);
                }
            }
        }
    }
    if !leak_pages.is_empty() {
        log::error!("PDF export verification: text still extractable on redacted page(s) {}", pages_list(&leak_pages));
        return Err(format!(
            "Export failed: internal check found approved text still extractable on redacted page(s) {}. \
             Nothing was exported.",
            pages_list(&leak_pages)
        ));
    }
    if !missed_pages.is_empty() {
        log::warn!("PDF export verification: approved text on untouched page(s) {}", pages_list(&missed_pages));
        return Err(missed_message(&missed_pages, &missed_cats));
    }

    // Strings anywhere in the object graph (link URIs, labels, layer names…).
    let mut strings = String::new();
    for obj in doc.objects.values() {
        collect_strings(obj, &mut strings);
    }
    let folded = Folded::new(&strings);
    let mut cats = BTreeSet::new();
    for (key, needle) in &set.needles {
        if folded.count(needle) > 0 {
            cats.insert(set.category[key]);
        }
    }
    if !cats.is_empty() {
        return Err(format!(
            "Export refused: approved text ({}) still appears in links or other document data outside \
             the page text. Nothing was exported.",
            cats.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    Ok(())
}

fn collect_strings(obj: &Object, out: &mut String) {
    match obj {
        Object::String(bytes, _) => {
            if bytes.starts_with(&[0xFE, 0xFF]) {
                let units: Vec<u16> =
                    bytes[2..].chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                out.push_str(&String::from_utf16_lossy(&units));
            } else {
                out.extend(bytes.iter().map(|&b| b as char));
            }
            out.push('\n');
        }
        Object::Array(a) => a.iter().for_each(|o| collect_strings(o, out)),
        Object::Dictionary(d) => d.iter().for_each(|(_, o)| collect_strings(o, out)),
        Object::Stream(s) => s.dict.iter().for_each(|(_, o)| collect_strings(o, out)),
        _ => {}
    }
}
