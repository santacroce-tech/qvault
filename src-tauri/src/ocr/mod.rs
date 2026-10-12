//! OCR for scanned PDF pages and images (Phase 2 of the QVAC integration).
//!
//! This module is platform-independent and has no knowledge of the sidecar:
//! - [`classify`] decides which PDF pages need OCR (no usable text layer, or
//!   mostly images);
//! - [`layout`] turns the sidecar's phrase boxes into word boxes, builds a
//!   normalised page text for the detection layers, and keeps a map from
//!   every character of that text back to a box on the page;
//! - [`image_import`] turns PNG / JPEG / TIFF uploads into image-only PDFs.
//!
//! The OCR engine itself is behind [`PageOcr`] (the QVAC sidecar in the app,
//! a fake in tests). Rasterisation happens in the frontend with PDF.js, which
//! is already bundled for the viewer; see docs/qvac-integration.md §10.

pub mod classify;
pub mod layout;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

use serde::{Deserialize, Serialize};

/// Default rasterisation resolution for OCR. See docs §10.2 for the 200 vs
/// 300 DPI measurements behind this value.
pub const DEFAULT_OCR_DPI: u32 = 300;
/// Resolutions offered in Settings.
pub const ALLOWED_OCR_DPI: [u32; 4] = [150, 200, 300, 400];

/// One block as returned by the OCR engine, in pixels of the input image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RawBlock {
    pub text: String,
    /// [x0, y0, x1, y1], origin top-left.
    pub bbox: [f64; 4],
    pub confidence: Option<f64>,
}

/// Why an OCR call failed.
#[derive(Debug, Clone, PartialEq)]
pub enum OcrFailure {
    /// The user cancelled.
    Cancelled,
    /// The engine or its models are missing / not usable.
    Unavailable(String),
    /// This page failed (timeout, crash, bad image).
    Failed(String),
}

impl std::fmt::Display for OcrFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OcrFailure::Cancelled => write!(f, "OCR cancelled"),
            OcrFailure::Unavailable(r) => write!(f, "OCR unavailable: {}", r),
            OcrFailure::Failed(r) => write!(f, "OCR failed: {}", r),
        }
    }
}

/// Anything that can OCR an image file (PNG or JPEG).
pub trait PageOcr: Send + Sync {
    fn ocr_image(&self, image: &Path) -> Result<Vec<RawBlock>, OcrFailure>;
}

/// Per-document OCR run state: cancellation and metadata for the audit log.
#[derive(Debug, Clone)]
pub struct OcrRun {
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub started: Instant,
    pub dpi: u32,
    pub pages_done: Vec<u32>,
    pub blocks: usize,
    pub ocr_ms: u128,
}

/// Runs in progress, keyed by document id.
#[derive(Default)]
pub struct OcrRuns {
    runs: Mutex<HashMap<String, OcrRun>>,
}

impl OcrRuns {
    /// The run for `doc`, created on first use.
    pub fn begin(&self, doc: &str, dpi: u32) -> OcrRun {
        let mut runs = self.runs.lock().unwrap();
        runs.entry(doc.to_string())
            .or_insert_with(|| OcrRun {
                cancel: Default::default(),
                started: Instant::now(),
                dpi,
                pages_done: Vec::new(),
                blocks: 0,
                ocr_ms: 0,
            })
            .clone()
    }

    pub fn record_page(&self, doc: &str, page: u32, blocks: usize, ms: u128) {
        if let Some(r) = self.runs.lock().unwrap().get_mut(doc) {
            r.pages_done.push(page);
            r.blocks += blocks;
            r.ocr_ms += ms;
        }
    }

    /// Ask the run for `doc` to stop. Returns false when nothing is running.
    pub fn cancel(&self, doc: &str) -> bool {
        match self.runs.lock().unwrap().get(doc) {
            Some(r) => {
                r.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                true
            }
            None => false,
        }
    }

    pub fn finish(&self, doc: &str) -> Option<OcrRun> {
        self.runs.lock().unwrap().remove(doc)
    }
}
