//! Opt-in end-to-end OCR test with the real QVAC sidecar and OCR models.
//!
//! Runs the Phase 2 pipeline on the scanned fixture
//! (`tests/fixtures/demo-scanned.pdf`, built by tools/make-scanned-fixture.mjs)
//! and on the text-layer original (`landing/demo.pdf`), reports recall per
//! category, and exports the scanned document with everything approved.
//!
//!   QVAC_SIDECAR_IT=1 QVAC_MODELS_DIR=/path/to/models \
//!     cargo test --release --test ocr_pipeline_it -- --nocapture --test-threads 1
//!
//! Optional: QVAC_OCR_DPIS=150,200,300 (resolutions to measure; default: the
//! app default, which is the one the recall bar is asserted at),
//! QVAC_IT_LAYER3=1 (also run Layer 3; needs the Qwen model in the dir).
//!
//! Rasterisation: the app renders pages with PDF.js in the webview; here
//! poppler's `pdftoppm` stands in for it (both rasterise the same embedded
//! image), so this test needs `pdftoppm` on PATH. Plain `cargo test` skips it.
#![cfg(not(any(target_os = "android", target_os = "ios")))]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::time::Instant;

use app_lib::document::page_displayed_size as displayed_size;
use app_lib::document::pdf_text::extract_page_texts;
use app_lib::document::redactor::{export_flattened_pdf_ocr, OcrExport, PageRaster};
use app_lib::document::text_match::OcrNeedle;
use app_lib::llm::llm_scanner::Layer3Engine;
use app_lib::ocr::layout::{build_layout, PageLayout};
use app_lib::pipeline::types::PiiEntity;
use app_lib::qvac::{QvacService, SidecarPaths};
use app_lib::storage::database::RedactionRow;

const JURISDICTIONS: [&str; 5] = ["global", "us", "eu", "br", "de"];

/// The 25 entities the website lists for demo.pdf (landing/index.html,
/// "Extracted PII — Single Run Results"), with its category labels.
const KNOWN: [(&str, &str); 25] = [
    ("Person Name", "Maria Santos"),
    ("Person Name", "James Richardson"),
    ("Person Name", "Elena Kowalski"),
    ("Person Name", "Roberto Fernandes"),
    ("Organization", "Meridian Consulting Group LLC"),
    ("Organization", "Atlas Digital Solutions Inc"),
    ("Email", "maria.santos@meridian-consulting.com"),
    ("Email", "j.richardson@atlas-digital.com"),
    ("Email", "elena.kowalski@meridian-consulting.com"),
    ("Email", "r.fernandes@atlas-digital.com"),
    ("Phone", "(415) 555-0187"),
    ("Phone", "(212) 555-0234"),
    ("Address", "1420 Market Street, Suite 200, San Francisco, CA 94103"),
    ("Address", "350 Fifth Avenue, Floor 12, New York, NY 10118"),
    ("SSN", "412-68-9103"),
    ("EIN", "82-4751093"),
    ("Credit Card", "4532-8901-2345-6789"),
    ("Bank Account", "7823401956"),
    ("Financial", "$150,000.00"),
    ("Financial", "$85,000.00"),
    ("Financial", "$50,000.00"),
    ("Financial", "$15,000.00"),
    ("Date", "June 15, 2024"),
    ("Date", "December 31, 2025"),
    ("Date", "March 15, 1985"),
];

fn enabled() -> bool {
    std::env::var("QVAC_SIDECAR_IT").map(|v| v == "1").unwrap_or(false)
}

fn manifest() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sidecar() -> SidecarPaths {
    let program = std::env::var_os("QVAC_SIDECAR_BIN").map(PathBuf::from).unwrap_or_else(|| {
        manifest()
            .join("binaries")
            .join(format!("qvac-sidecar-{}{}", env!("QVAULT_TARGET_TRIPLE"), std::env::consts::EXE_SUFFIX))
    });
    let worker = std::env::var_os("QVAC_SIDECAR_WORKER")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest().join("resources").join("qvac-worker").join("worker.js"));
    SidecarPaths::checked(program.clone(), worker.clone())
        .unwrap_or_else(|| panic!("sidecar not built: {:?} / {:?} (run `npm run sidecar:build`)", program, worker))
}

fn service(tag: &str) -> (Arc<QvacService>, PathBuf) {
    let models = PathBuf::from(std::env::var("QVAC_MODELS_DIR").expect("QVAC_MODELS_DIR"));
    let app_data = std::env::temp_dir().join(format!("qvault-ocr-it-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&app_data).unwrap();
    let svc = Arc::new(QvacService::new(&app_data, Some(&models), Some(sidecar())));
    assert!(svc.ocr_installed(), "OCR models missing from QVAC_MODELS_DIR");
    (svc, app_data)
}

fn dpis() -> Vec<u32> {
    std::env::var("QVAC_OCR_DPIS")
        .unwrap_or_else(|_| app_lib::ocr::DEFAULT_OCR_DPI.to_string())
        .split(',')
        .map(|s| s.trim().parse().expect("QVAC_OCR_DPIS"))
        .collect()
}

/// Rasterise every page of `pdf` at `dpi` (PNG), like the viewer does.
fn rasterize(pdf: &Path, dpi: u32, dir: &Path) -> Vec<PathBuf> {
    std::fs::create_dir_all(dir).unwrap();
    let prefix = dir.join("p");
    let st = Command::new("pdftoppm")
        .args(["-r", &dpi.to_string(), "-png"])
        .arg(pdf)
        .arg(&prefix)
        .status()
        .expect("pdftoppm (poppler) is needed for this test");
    assert!(st.success());
    let mut pages: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map_or(false, |e| e == "png"))
        .collect();
    pages.sort();
    pages
}

fn alnum(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

struct Scan {
    texts: Vec<String>,
    entities: Vec<Vec<PiiEntity>>,
    layouts: BTreeMap<u32, PageLayout>,
    images: Vec<PathBuf>,
    ocr_ms: Vec<u128>,
}

fn detect(text: &str, page: u32, layer3: Option<&dyn Layer3Engine>) -> Vec<PiiEntity> {
    app_lib::pipeline::run_pipeline(text, page, &JURISDICTIONS, layer3, None).0
}

fn scan_scanned(svc: &QvacService, dpi: u32, dir: &Path, layer3: Option<&dyn Layer3Engine>) -> Scan {
    let pdf_path = manifest().join("tests/fixtures/demo-scanned.pdf");
    let doc = lopdf::Document::load(&pdf_path).unwrap();
    let page_ids: Vec<_> = doc.get_pages().values().copied().collect();
    let classes = app_lib::ocr::classify::classify_pdf(&doc, &[]);
    assert!(classes.iter().all(|c| c.needs_ocr), "every scanned page must be classified for OCR: {:?}", classes);
    let images = rasterize(&pdf_path, dpi, &dir.join(format!("scan-{}", dpi)));
    assert_eq!(images.len(), page_ids.len());
    let mut s = Scan { texts: Vec::new(), entities: Vec::new(), layouts: BTreeMap::new(), images: images.clone(), ocr_ms: Vec::new() };
    for (i, img) in images.iter().enumerate() {
        let (w, h) = image::image_dimensions(img).unwrap();
        let (w_pt, h_pt) = displayed_size(&doc, page_ids[i]);
        let t = Instant::now();
        let blocks = svc.ocr_blocks(img, None).expect("ocr");
        s.ocr_ms.push(t.elapsed().as_millis());
        let layout = build_layout(&blocks, w, h, w_pt, h_pt, dpi);
        s.entities.push(detect(&layout.text, i as u32, layer3));
        s.texts.push(layout.text.clone());
        s.layouts.insert(i as u32, layout);
    }
    s
}

/// Does detection text `hay` contain `needle` as whole words, comparing
/// letters/digits only (so "$50,000.00" is not found inside "$150,000.00")?
fn covers(hay: &str, needle: &str) -> bool {
    match OcrNeedle::new(needle) {
        Some(n) => !n.find_all(hay).is_empty(),
        None => alnum(hay) == alnum(needle),
    }
}

/// Per-category recall of `reference` detections in `candidate`: a
/// reference detection counts as found when a candidate detection on the
/// same page contains it (see `covers`).
fn recall_by_category(reference: &[Vec<PiiEntity>], candidate: &[Vec<PiiEntity>]) -> BTreeMap<String, (usize, usize)> {
    let mut out: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (p, refs) in reference.iter().enumerate() {
        let seen: BTreeMap<(String, String), String> =
            refs.iter().map(|e| ((e.category.to_string(), alnum(&e.text)), e.text.clone())).collect();
        let cands: Vec<&str> = candidate.get(p).map(|v| v.iter().map(|e| e.text.as_str()).collect()).unwrap_or_default();
        for ((cat, key), text) in seen {
            if key.is_empty() {
                continue;
            }
            let e = out.entry(cat).or_default();
            e.1 += 1;
            if cands.iter().any(|c| covers(c, &text)) {
                e.0 += 1;
            }
        }
    }
    out
}

/// Recall of the 25 known entities, counted per (entity, page it appears on).
fn known_recall(page_texts: &[String], detections: &[Vec<PiiEntity>]) -> BTreeMap<&'static str, (usize, usize)> {
    let mut out: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
    for (cat, text) in KNOWN {
        for (p, page_text) in page_texts.iter().enumerate() {
            if !covers(page_text, text) {
                continue;
            }
            let e = out.entry(cat).or_default();
            e.1 += 1;
            if detections[p].iter().any(|d| covers(&d.text, text)) {
                e.0 += 1;
            }
        }
    }
    out
}

fn total<K>(m: &BTreeMap<K, (usize, usize)>) -> (usize, usize) {
    m.values().fold((0, 0), |a, v| (a.0 + v.0, a.1 + v.1))
}

fn pct(v: (usize, usize)) -> f64 {
    if v.1 == 0 { 100.0 } else { 100.0 * v.0 as f64 / v.1 as f64 }
}

fn print_table<K: std::fmt::Display + Ord>(title: &str, text: &BTreeMap<K, (usize, usize)>, scans: &[(u32, BTreeMap<K, (usize, usize)>)]) {
    eprintln!("\n{}", title);
    let mut header = format!("  {:<22} {:>12}", "category", "text layer");
    for (dpi, _) in scans {
        header.push_str(&format!(" {:>12}", format!("scan {} dpi", dpi)));
    }
    eprintln!("{}", header);
    for (k, v) in text {
        let mut line = format!("  {:<22} {:>12}", k.to_string(), format!("{}/{}", v.0, v.1));
        for (_, m) in scans {
            let s = m.get(k).copied().unwrap_or((0, 0));
            line.push_str(&format!(" {:>12}", format!("{}/{}", s.0, s.1)));
        }
        eprintln!("{}", line);
    }
    let t = total(text);
    let mut line = format!("  {:<22} {:>12}", "TOTAL", format!("{}/{} {:.0}%", t.0, t.1, pct(t)));
    for (_, m) in scans {
        let s = total(m);
        line.push_str(&format!(" {:>12}", format!("{}/{} {:.0}%", s.0, s.1, pct(s))));
    }
    eprintln!("{}", line);
}

#[test]
fn scanned_recall_vs_text_layer() {
    if !enabled() {
        eprintln!("skipping: set QVAC_SIDECAR_IT=1 (and QVAC_MODELS_DIR) to run");
        return;
    }
    let (svc, app_data) = service("recall");
    let layer3 = if std::env::var("QVAC_IT_LAYER3").map(|v| v == "1").unwrap_or(false) {
        svc.set_layer3(true, Some("qwen2.5-0.5b-instruct-q4_k_m".into())).unwrap();
        Some(svc.prepare_layer3().expect("prepare layer 3"))
    } else {
        None
    };
    let engine: Option<&dyn Layer3Engine> = layer3.as_ref().map(|s| s as &dyn Layer3Engine);

    // Text-layer baseline.
    let original = lopdf::Document::load(manifest().join("../landing/demo.pdf")).unwrap();
    let texts = extract_page_texts(&original);
    let text_entities: Vec<Vec<PiiEntity>> = texts.iter().enumerate().map(|(p, t)| detect(t, p as u32, engine)).collect();
    eprintln!("text layer: {} detections", text_entities.iter().map(|v| v.len()).sum::<usize>());

    let text_known = known_recall(&texts, &text_entities);
    let mut det_rows = Vec::new();
    let mut known_rows = Vec::new();
    let mut timing = Vec::new();
    for dpi in dpis() {
        let t = Instant::now();
        let scan = scan_scanned(&svc, dpi, &app_data, engine);
        let total_ms = t.elapsed().as_millis();
        eprintln!(
            "scan @ {} dpi: {} detections; OCR per page {:?} ms; pipeline total {} ms",
            dpi,
            scan.entities.iter().map(|v| v.len()).sum::<usize>(),
            scan.ocr_ms,
            total_ms
        );
        if std::env::var("QVAC_IT_DUMP").is_ok() {
            for (p, t) in scan.texts.iter().enumerate() {
                eprintln!("--- page {} OCR text @ {} dpi ---\n{}", p + 1, dpi, t);
            }
            for (p, ents) in scan.entities.iter().enumerate() {
                for e in ents {
                    eprintln!("  p{} [{}] {:?} {:?}", p + 1, e.layer, e.category.to_string(), e.text);
                }
            }
        }
        timing.push((dpi, scan.ocr_ms.clone()));
        det_rows.push((dpi, recall_by_category(&text_entities, &scan.entities)));
        known_rows.push((dpi, known_recall(&texts, &scan.entities)));
    }
    let text_self = recall_by_category(&text_entities, &text_entities);
    print_table("Recall of text-layer detections (found / total)", &text_self, &det_rows);
    print_table("Recall of the 25 known demo entities, per (entity, page)", &text_known, &known_rows);
    for (dpi, ms) in &timing {
        let sum: u128 = ms.iter().sum();
        eprintln!("OCR time @ {} dpi: {:?} ms per page, {:.1} s/page average", dpi, ms, sum as f64 / ms.len() as f64 / 1000.0);
    }

    // The bar: scanned recall >= 90 % of the text-layer recall, on both
    // measures. It is asserted at the app's default resolution; other
    // resolutions are reported. Measured 2026-10-12 (Apple Silicon, CPU):
    //   150 dpi: detections 88.6 %, known 67.6 % -> below the bar
    //   200 dpi: detections 97.7 %, known 79.4 % (text layer 79.4 %) -> passes
    //   300 dpi: detections 90.9 %, known 70.6 % (bar 71.5 %) -> 1 entity short
    // which is why the default is 200 DPI (docs/qvac-integration.md §10.2).
    let text_known_recall = pct(total(&text_known));
    let mut checked = false;
    for ((dpi, det), (_, known)) in det_rows.iter().zip(&known_rows) {
        let r_det = pct(total(det)); // text-layer recall of its own detections is 100 %
        let r_known = pct(total(known));
        let ok = r_det >= 90.0 && r_known >= 0.9 * text_known_recall;
        eprintln!(
            "@ {} dpi: detections {:.1}% (bar 90%), known entities {:.1}% vs text layer {:.1}% (bar {:.1}%) {}",
            dpi,
            r_det,
            r_known,
            text_known_recall,
            0.9 * text_known_recall,
            if ok { "OK" } else { "BELOW BAR" }
        );
        if *dpi != app_lib::ocr::DEFAULT_OCR_DPI {
            continue;
        }
        checked = true;
        assert!(r_det >= 90.0, "@ {} dpi: scanned recall of text-layer detections {:.1}% < 90%", dpi, r_det);
        assert!(
            r_known >= 0.9 * text_known_recall,
            "@ {} dpi: known-entity recall {:.1}% < 90% of text-layer {:.1}%",
            dpi,
            r_known,
            text_known_recall
        );
    }
    assert!(checked, "include the default DPI ({}) in QVAC_OCR_DPIS", app_lib::ocr::DEFAULT_OCR_DPI);
    svc.shutdown();
    let _ = std::fs::remove_dir_all(&app_data);
}

#[test]
fn scanned_export_leaves_nothing_readable() {
    if !enabled() {
        eprintln!("skipping: set QVAC_SIDECAR_IT=1 (and QVAC_MODELS_DIR) to run");
        return;
    }
    let (svc, app_data) = service("export");
    let dpi = app_lib::ocr::DEFAULT_OCR_DPI;
    let scan = scan_scanned(&svc, dpi, &app_data, None);

    // Approve every detection.
    let mut rows = Vec::new();
    for (p, ents) in scan.entities.iter().enumerate() {
        for e in ents {
            rows.push(RedactionRow {
                id: e.id.clone(),
                document_id: "doc".into(),
                page: p as i32,
                start_offset: e.start as i32,
                end_offset: e.end as i32,
                original_text: e.text.clone(),
                replacement: e.replacement.clone(),
                category: e.category.to_string(),
                detection_layer: e.layer.clone(),
                confidence: e.confidence,
                status: "approved".into(),
                reviewed_at: None,
            });
        }
    }
    assert!(rows.len() >= 20, "only {} detections on the scanned fixture", rows.len());
    let pages_with: BTreeSet<u32> = rows.iter().map(|r| r.page as u32).collect();
    let rasters: Vec<PageRaster> = pages_with
        .iter()
        .map(|&p| PageRaster { page_index: p, image: std::fs::read(&scan.images[p as usize]).unwrap(), rects: Vec::new() })
        .collect();

    let pdf = std::fs::read(manifest().join("tests/fixtures/demo-scanned.pdf")).unwrap();
    let out = app_data.join("demo-scanned_redacted.pdf");
    let work = svc.page_work_dir().unwrap();
    let ocr = OcrExport { layouts: scan.layouts.clone(), engine: Some(&*svc), work_dir: work.clone(), on_progress: None };
    let t = Instant::now();
    let summary = export_flattened_pdf_ocr(&pdf, &rows, &rasters, &out, Some(&ocr)).expect("export");
    eprintln!(
        "export with OCR verification: {} ms, rasterized {:?}, OCR-verified {:?}",
        t.elapsed().as_millis(),
        summary.rasterized_pages,
        summary.ocr_verified_pages
    );
    assert_eq!(summary.ocr_verified_pages, summary.rasterized_pages);
    assert!(std::fs::read_dir(&work).unwrap().next().is_none(), "temporary page images are deleted");

    let needles: Vec<(String, OcrNeedle)> =
        rows.iter().filter_map(|r| OcrNeedle::new(&r.original_text).map(|n| (r.category.clone(), n))).collect();

    // 1. Text extraction (own extractor + poppler).
    let doc = lopdf::Document::load(&out).unwrap();
    for t in extract_page_texts(&doc) {
        assert!(t.trim().is_empty(), "the export must not have a text layer");
    }
    let pdftotext = Command::new("pdftotext").arg(&out).arg("-").output().expect("pdftotext");
    let extracted = String::from_utf8_lossy(&pdftotext.stdout).to_string();
    assert!(extracted.trim().is_empty(), "pdftotext found text in the export");

    // 2. OCR of the exported file, rendered independently.
    let pages = rasterize(&out, dpi, &app_data.join("exported"));
    let mut leaks = Vec::new();
    for (i, img) in pages.iter().enumerate() {
        let (w, h) = image::image_dimensions(img).unwrap();
        let blocks = svc.ocr_blocks(img, None).expect("ocr of export");
        let text = build_layout(&blocks, w, h, w as f64, h as f64, dpi).text;
        let raw: String = blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join(" ");
        for (cat, n) in &needles {
            if !n.find_all(&text).is_empty() || !n.find_all(&raw).is_empty() {
                leaks.push(format!("page {} {}", i + 1, cat));
            }
        }
    }
    eprintln!("OCR of the exported file: {} approved string(s) still recognisable", leaks.len());
    assert!(leaks.is_empty(), "approved text still recognisable by OCR: {:?}", leaks);

    svc.shutdown();
    if std::env::var("QVAC_IT_KEEP").is_ok() {
        eprintln!("kept: {}", out.display());
    } else {
        let _ = std::fs::remove_dir_all(&app_data);
    }
}
