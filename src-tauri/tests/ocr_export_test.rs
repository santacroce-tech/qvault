//! Export of scanned (OCR'd) pages without real OCR models: a fake engine
//! "reads" words drawn as grey blocks and stops reading them once they are
//! burned black. Covers the OCR side of `export_flattened_pdf_ocr`: boxes
//! placed from the OCR layout, missed occurrences on scanned pages without
//! redactions, the OCR-of-export leak check, and the refusals.
//!
//! The real-model version of these checks is `ocr_pipeline_it.rs` (opt-in).
extern crate app_lib;

use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use app_lib::document::pdf_text::extract_page_texts;
use app_lib::document::redactor::{export_flattened_pdf_ocr, OcrExport, PageRaster};
use app_lib::document::text_match::OcrNeedle;
use app_lib::ocr::image_import::{image_to_pdf, ImageKind};
use app_lib::ocr::layout::{build_layout, PageLayout};
use app_lib::ocr::{OcrFailure, PageOcr, RawBlock};
use app_lib::storage::database::RedactionRow;
use image::{DynamicImage, GrayImage, Luma};
use lopdf::{Document, Object};

/// Words on the page as (text, [x0, y0, x1, y1] in fractions of the page).
type Words = Vec<(&'static str, [f64; 4])>;

fn page1() -> Words {
    vec![
        ("Client:", [0.10, 0.10, 0.20, 0.13]),
        ("Jane", [0.21, 0.10, 0.27, 0.13]),
        ("Doe,", [0.28, 0.10, 0.34, 0.13]),
        ("phone", [0.36, 0.10, 0.44, 0.13]),
        ("555-867-5309.", [0.45, 0.10, 0.62, 0.13]),
        ("Thank", [0.10, 0.20, 0.18, 0.23]),
        ("you.", [0.19, 0.20, 0.25, 0.23]),
    ]
}

fn page2(with_name: bool) -> Words {
    let mut w = vec![("Signed", [0.10, 0.10, 0.20, 0.13])];
    if with_name {
        w.push(("Jane", [0.21, 0.10, 0.27, 0.13]));
        w.push(("Doe", [0.28, 0.10, 0.34, 0.13]));
    }
    w
}

const W: u32 = 850; // 100 DPI Letter
const H: u32 = 1100;
const DPI: u32 = 100;

fn draw(words: &Words) -> GrayImage {
    let mut g = GrayImage::from_pixel(W, H, Luma([255]));
    for (_, f) in words {
        let (x0, y0, x1, y1) = ((f[0] * W as f64) as u32, (f[1] * H as f64) as u32, (f[2] * W as f64) as u32, (f[3] * H as f64) as u32);
        for y in y0..y1 {
            for x in x0..x1 {
                g.put_pixel(x, y, Luma([110]));
            }
        }
    }
    g
}

fn png(g: &GrayImage) -> Vec<u8> {
    let mut out = Vec::new();
    DynamicImage::ImageLuma8(g.clone()).write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

/// Reads a word when its area is grey "ink"; burned (black) or blank areas
/// read as nothing. Which word list applies is chosen by a marker pixel row
/// (page 2 has a dark pixel at the bottom-left corner).
struct FakeOcr {
    pages: Vec<Words>,
    calls: AtomicUsize,
    /// Pretend the engine still reads everything (to test the leak check).
    always_read: bool,
}

impl PageOcr for FakeOcr {
    fn ocr_image(&self, path: &Path) -> Result<Vec<RawBlock>, OcrFailure> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let img = image::open(path).map_err(|e| OcrFailure::Failed(e.to_string()))?.to_luma8();
        let (w, h) = img.dimensions();
        let page = if img.get_pixel(1, h - 2)[0] < 128 { 1 } else { 0 };
        let mut out = Vec::new();
        for (text, f) in &self.pages[page] {
            let b = [f[0] * w as f64, f[1] * h as f64, f[2] * w as f64, f[3] * h as f64];
            let mut sum = 0u64;
            let mut n = 0u64;
            for y in b[1] as u32 + 2..b[3] as u32 - 2 {
                for x in b[0] as u32 + 2..b[2] as u32 - 2 {
                    sum += img.get_pixel(x, y)[0] as u64;
                    n += 1;
                }
            }
            let mean = sum / n.max(1);
            if self.always_read || (40..230).contains(&mean) {
                out.push(RawBlock { text: text.to_string(), bbox: b, confidence: Some(0.9) });
            }
        }
        Ok(out)
    }
}

fn row(id: &str, page: i32, text: &str, category: &str) -> RedactionRow {
    RedactionRow {
        id: id.into(),
        document_id: "doc".into(),
        page,
        start_offset: 0,
        end_offset: text.len() as i32,
        original_text: text.into(),
        replacement: "[X]".into(),
        category: category.into(),
        detection_layer: "regex".into(),
        confidence: 1.0,
        status: "approved".into(),
        reviewed_at: None,
    }
}

struct Fixture {
    pdf: Vec<u8>,
    images: Vec<Vec<u8>>,
    layouts: BTreeMap<u32, PageLayout>,
    engine: FakeOcr,
    dir: PathBuf,
}

/// A two-page image-only PDF (built through the image import path, as for
/// an uploaded TIFF), OCR'd with the fake engine at 100 DPI.
fn fixture(test: &str, name_on_page2: bool) -> Fixture {
    let pages = vec![page1(), page2(name_on_page2)];
    let mut imgs: Vec<GrayImage> = pages.iter().map(draw).collect();
    imgs[1].put_pixel(1, H - 2, Luma([0])); // page marker for the fake engine
    // Build a 2-page TIFF and import it.
    let mut tiff = Cursor::new(Vec::new());
    {
        let mut enc = tiff::encoder::TiffEncoder::new(&mut tiff).unwrap();
        for g in &imgs {
            let mut p = enc.new_image::<tiff::encoder::colortype::Gray8>(W, H).unwrap();
            p.resolution(tiff::tags::ResolutionUnit::Inch, tiff::encoder::Rational { n: DPI, d: 1 });
            p.write_data(g.as_raw()).unwrap();
        }
    }
    let pdf = image_to_pdf(&tiff.into_inner(), ImageKind::Tiff).unwrap().pdf;
    let engine = FakeOcr { pages, calls: AtomicUsize::new(0), always_read: false };
    let dir = std::env::temp_dir().join(format!("qvault-ocr-export-{}-{}", std::process::id(), test));
    std::fs::create_dir_all(&dir).unwrap();
    let images: Vec<Vec<u8>> = imgs.iter().map(png).collect();
    let mut layouts = BTreeMap::new();
    for (i, bytes) in images.iter().enumerate() {
        let p = dir.join(format!("in-{}.png", i));
        std::fs::write(&p, bytes).unwrap();
        let blocks = engine.ocr_image(&p).unwrap();
        layouts.insert(i as u32, build_layout(&blocks, W, H, 612.0, 792.0, DPI));
    }
    engine.calls.store(0, Ordering::SeqCst);
    Fixture { pdf, images, layouts, engine, dir }
}

fn raster(f: &Fixture, page: u32) -> PageRaster {
    PageRaster { page_index: page, image: f.images[page as usize].clone(), rects: Vec::new() }
}

fn run(f: &Fixture, rows: &[RedactionRow], rasters: &[PageRaster], engine: Option<&dyn PageOcr>) -> (Result<app_lib::document::redactor::PdfExportSummary, String>, PathBuf) {
    let out = f.dir.join("out.pdf");
    let _ = std::fs::remove_file(&out);
    let ocr = OcrExport { layouts: f.layouts.clone(), engine, work_dir: f.dir.join("work"), on_progress: None };
    (export_flattened_pdf_ocr(&f.pdf, rows, rasters, &out, Some(&ocr)), out)
}

#[test]
fn layout_text_feeds_detection() {
    let f = fixture("detect", false);
    let text = &f.layouts[&0].text;
    assert!(text.contains("Jane Doe,"), "{}", text);
    let found = app_lib::pipeline::run_pipeline_no_llm(text, 0, &["global", "us"]);
    assert!(found.iter().any(|e| e.text.contains("555-867-5309")), "{:?}", found);
}

#[test]
fn scanned_page_is_burned_from_ocr_boxes_and_verified_by_ocr() {
    let f = fixture("ok", false);
    let rows = vec![row("a", 0, "Jane Doe", "PersonName"), row("b", 0, "555-867-5309", "PhoneNumber")];
    let (res, out) = run(&f, &rows, &[raster(&f, 0)], Some(&f.engine));
    let summary = res.expect("export");
    assert_eq!(summary.rasterized_pages, vec![0]);
    assert_eq!(summary.ocr_verified_pages, vec![0]);
    assert_eq!(f.engine.calls.load(Ordering::SeqCst), 1, "the burned page was OCR'd");

    // No text layer anywhere, and OCR of the exported page finds nothing.
    let doc = Document::load(&out).unwrap();
    assert!(extract_page_texts(&doc).iter().all(|t| t.trim().is_empty()));
    let page_ids: Vec<_> = doc.get_pages().values().copied().collect();
    let img_bytes = doc
        .objects
        .values()
        .find_map(|o| match o {
            Object::Stream(s) if s.dict.get(b"Filter").and_then(Object::as_name).ok() == Some(b"DCTDecode") => Some(s.content.clone()),
            _ => None,
        })
        .unwrap();
    let p = f.dir.join("exported.jpg");
    std::fs::write(&p, &img_bytes).unwrap();
    let blocks = f.engine.ocr_image(&p).unwrap();
    let text: String = blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join(" ");
    assert_eq!(text, "Client: phone Thank you.");
    for r in &rows {
        assert!(OcrNeedle::new(&r.original_text).unwrap().find_all(&text).is_empty());
    }
    // Page 2 untouched (still the original image, nothing added).
    assert_eq!(page_ids.len(), 2);
    let _ = std::fs::remove_dir_all(&f.dir);
}

#[test]
fn approved_text_on_scanned_page_without_redaction_is_refused() {
    let f = fixture("missed", true);
    let rows = vec![row("a", 0, "Jane Doe", "PersonName")];
    let (res, out) = run(&f, &rows, &[raster(&f, 0)], Some(&f.engine));
    let err = res.unwrap_err();
    assert!(err.contains("page(s) 2"), "{}", err);
    assert!(err.contains("PersonName"), "{}", err);
    assert!(!err.contains("Jane"), "never echo the PII: {}", err);
    assert!(!out.exists());
}

#[test]
fn text_still_readable_after_burn_fails_the_export() {
    let mut f = fixture("leak", false);
    f.engine.always_read = true;
    let rows = vec![row("a", 0, "Jane Doe", "PersonName")];
    let (res, out) = run(&f, &rows, &[raster(&f, 0)], Some(&f.engine));
    let err = res.unwrap_err();
    assert!(err.contains("still readable by OCR"), "{}", err);
    assert!(err.contains("page(s) 1"), "{}", err);
    assert!(!err.contains("Jane"), "{}", err);
    assert!(!out.exists(), "nothing is left behind");
}

#[test]
fn scanned_pages_cannot_be_exported_without_ocr_to_verify() {
    let f = fixture("noengine", false);
    let rows = vec![row("a", 0, "Jane Doe", "PersonName")];
    let (res, out) = run(&f, &rows, &[raster(&f, 0)], None);
    let err = res.unwrap_err();
    assert!(err.contains("OCR is not") && err.contains("available"), "{}", err);
    assert!(!out.exists());
}

#[test]
fn missing_page_image_or_low_resolution_is_refused() {
    let f = fixture("noraster", false);
    let rows = vec![row("a", 0, "Jane Doe", "PersonName")];
    // No image for the page with the redaction: cannot be placed.
    let (res, _) = run(&f, &rows, &[], Some(&f.engine));
    assert!(res.unwrap_err().contains("could not be located"));
    // Rendered at half the OCR resolution.
    let small = image::imageops::resize(&image::load_from_memory(&f.images[0]).unwrap().to_luma8(), W / 2, H / 2, image::imageops::FilterType::Triangle);
    let r = PageRaster { page_index: 0, image: png(&small), rects: Vec::new() };
    let (res, _) = run(&f, &rows, &[r], Some(&f.engine));
    assert!(res.unwrap_err().contains("below its OCR resolution"));
}

#[test]
fn occurrence_rects_cover_every_occurrence() {
    let f = fixture("rects", true);
    let n = OcrNeedle::new("Jane Doe").unwrap();
    let r0 = f.layouts[&0].occurrence_rects(&n);
    assert_eq!(r0.len(), 1);
    // Points on a Letter page: the union of the "Jane" and "Doe," boxes.
    let [x0, y0, x1, y1] = r0[0].1;
    assert!((x0 - 0.21 * 612.0).abs() < 1.0 && (x1 - 0.34 * 612.0).abs() < 1.0, "{:?}", r0);
    assert!((y0 - 0.10 * 792.0).abs() < 1.0 && (y1 - 0.13 * 792.0).abs() < 1.0, "{:?}", r0);
    assert_eq!(f.layouts[&1].occurrence_rects(&n).len(), 1);
    let _ = std::fs::remove_dir_all(&f.dir);
}
