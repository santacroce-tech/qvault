//! Opt-in integration test: spawns the real QVAC sidecar and runs OCR on a
//! fixture image plus a small Layer 3 completion.
//!
//! Skipped unless `QVAC_SIDECAR_IT=1`. Needs:
//!   * the sidecar built (`npm run sidecar:build`), or QVAC_SIDECAR_BIN +
//!     QVAC_SIDECAR_WORKER pointing at one (e.g. inside a built .app);
//!   * QVAC_MODELS_DIR containing the catalog files `craft_mlt_25k.gguf`,
//!     `latin_g2.gguf` and `qwen2.5-0.5b-instruct-q4_k_m.gguf`.
//!
//!   QVAC_SIDECAR_IT=1 QVAC_MODELS_DIR=/path/to/models cargo test --test qvac_sidecar_it -- --nocapture
#![cfg(not(any(target_os = "android", target_os = "ios")))]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use app_lib::llm::llm_scanner::Layer3Engine;
use app_lib::pipeline::{self, Layer3Status};
use app_lib::qvac::{QvacService, SidecarPaths};

fn enabled() -> bool {
    std::env::var("QVAC_SIDECAR_IT").map(|v| v == "1").unwrap_or(false)
}

fn sidecar() -> SidecarPaths {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let program = std::env::var_os("QVAC_SIDECAR_BIN").map(PathBuf::from).unwrap_or_else(|| {
        manifest
            .join("binaries")
            .join(format!("qvac-sidecar-{}{}", env!("QVAULT_TARGET_TRIPLE"), std::env::consts::EXE_SUFFIX))
    });
    let worker = std::env::var_os("QVAC_SIDECAR_WORKER")
        .map(PathBuf::from)
        .unwrap_or_else(|| manifest.join("resources").join("qvac-worker").join("worker.js"));
    SidecarPaths::checked(program.clone(), worker.clone())
        .unwrap_or_else(|| panic!("sidecar not built: {:?} / {:?} (run `npm run sidecar:build`)", program, worker))
}

#[test]
fn real_sidecar_ocr_and_layer3() {
    if !enabled() {
        eprintln!("skipping: set QVAC_SIDECAR_IT=1 (and QVAC_MODELS_DIR) to run");
        return;
    }
    let models = PathBuf::from(std::env::var("QVAC_MODELS_DIR").expect("QVAC_MODELS_DIR"));
    let app_data = std::env::temp_dir().join(format!("qvault-it-{}", std::process::id()));
    std::fs::create_dir_all(&app_data).unwrap();
    let svc = Arc::new(QvacService::new(&app_data, Some(&models), Some(sidecar())));

    // Capability probe: sidecar starts, fit verdict for the installed LLM.
    let t = Instant::now();
    let caps = svc.capabilities(true);
    eprintln!("probe {} ms: running={} version={:?}", t.elapsed().as_millis(), caps.sidecar_running, caps.sidecar_version);
    assert!(caps.sidecar_running, "{:?}", caps.unavailable_reason);
    assert!(caps.ocr_installed, "OCR models missing from QVAC_MODELS_DIR");
    let llm = caps.models.iter().find(|m| m.id == "qwen2.5-0.5b-instruct-q4_k_m").unwrap();
    assert!(llm.installed, "Qwen2.5-0.5B missing from QVAC_MODELS_DIR");
    let fit = llm.fit.as_ref().expect("fit verdict");
    eprintln!("fit: {} ({:?})", fit.verdict, fit.evidence);
    assert!(["likely-fits", "likely-too-large", "unknown"].contains(&fit.verdict.as_str()));

    // OCR on the fixture image.
    let img = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/selftest/ocr-sample.png");
    let t = Instant::now();
    let blocks = svc.ocr(&img).expect("ocr");
    let text: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    eprintln!("ocr {} ms: {:?}", t.elapsed().as_millis(), text);
    let joined = text.join(" ");
    assert!(joined.contains("Jane"), "{}", joined);
    assert!(joined.contains("867"), "{}", joined);
    for b in &blocks {
        let bb = b.bbox.expect("bbox");
        assert!(bb[0] >= 0.0 && bb[2] <= 900.0 && bb[1] >= 0.0 && bb[3] <= 220.0, "bbox in image pixels: {:?}", bb);
    }

    // Layer 3 through the pipeline.
    svc.set_layer3(true, Some("qwen2.5-0.5b-instruct-q4_k_m".into())).unwrap();
    let session = svc.prepare_layer3().expect("prepare layer 3");
    eprintln!("layer3 model load: {:?}", session.loaded_in);
    let page = "This Agreement is entered into by Jonathan P. Whitmore, residing at 4417 Birchwood Avenue, \
                Portland, Oregon 97205, and Atlas Digital Solutions Inc., represented by Sarah Lindqvist.";
    let t = Instant::now();
    let engine: &dyn Layer3Engine = &session;
    let (entities, status) = pipeline::run_pipeline(page, 0, &["global", "us"], Some(engine), None);
    eprintln!("layer3 {} ms: {:?}", t.elapsed().as_millis(), status);
    for e in &entities {
        eprintln!("  [{}] {:?} {:?} {:.2}", e.layer, e.category, e.text, e.confidence);
        assert_eq!(&page[e.start..e.end], e.text);
    }
    assert!(matches!(status, Layer3Status::Ran { failed_chunks: 0, .. }), "{:?}", status);
    let llm_hits: Vec<_> = entities.iter().filter(|e| e.layer == "llm").collect();
    assert!(!llm_hits.is_empty(), "Layer 3 found nothing");
    assert!(llm_hits.iter().all(|e| e.confidence >= 0.5 && e.confidence < 0.95), "Layer 3 confidence stays below Layer 1");

    // Self-test path used by the app.
    let rep = svc.self_test(Some(&img));
    eprintln!("self-test: {:?}", rep);
    assert!(rep.error.is_none() && rep.ocr_error.is_none() && rep.layer3_error.is_none() && rep.embedding_error.is_none());

    svc.shutdown();
    let _ = std::fs::remove_dir_all(&app_data);
}

/// Phase 4: the catalog embedding model through the real sidecar, and entity
/// grouping with it. Needs `qwen3-embedding-0.6b-q8_0.gguf` in QVAC_MODELS_DIR.
#[test]
fn real_sidecar_embeddings_and_grouping() {
    use app_lib::entities::resolve::ResolveConfig;
    use app_lib::entities::service;
    use app_lib::qvac::{catalog, QvacEmbedder};
    use app_lib::storage::database::Database;

    if !enabled() {
        eprintln!("skipping: set QVAC_SIDECAR_IT=1 (and QVAC_MODELS_DIR) to run");
        return;
    }
    let models = PathBuf::from(std::env::var("QVAC_MODELS_DIR").expect("QVAC_MODELS_DIR"));
    let app_data = std::env::temp_dir().join(format!("qvault-it-embed-{}", std::process::id()));
    std::fs::create_dir_all(&app_data).unwrap();
    let svc = Arc::new(QvacService::new(&app_data, Some(&models), Some(sidecar())));
    let model = svc.embedding_model().expect("embedding model missing from QVAC_MODELS_DIR");
    assert_eq!(model, catalog::embedding_model().id);
    let caps = svc.capabilities(false);
    assert!(caps.embedding_installed && caps.entity_embeddings_enabled);

    let t = Instant::now();
    let names: Vec<String> = ["Maria Santos", "SANTOS, Maria", "Atlas Logistics Inc."].iter().map(|s| s.to_string()).collect();
    let v = svc.embed_texts(&model, &names).expect("embed");
    eprintln!("embed (incl. load) {} ms, dim {}", t.elapsed().as_millis(), v[0].len());
    assert_eq!(v.len(), 3);
    let cos = app_lib::entities::resolve::cosine;
    assert!(cos(&v[0], &v[1]) > cos(&v[0], &v[2]));
    assert!((cos(&v[0], &v[0]) - 1.0).abs() < 1e-3);

    // Grouping with vectors stored in the database.
    let db = Database::open(&app_data.join("qvault.db")).unwrap();
    db.insert_document("d", "d.pdf", "pdf", "h", 1, "/x").unwrap();
    let rows = [("PersonName", "Maria Santos"), ("PersonName", "SANTOS, Maria"), ("PersonName", "M. Santos"), ("CompanyName", "Atlas Logistics Inc.")];
    for (k, (cat, text)) in rows.iter().enumerate() {
        db.insert_entity(&format!("e{}", k), "d", text, text, cat, 0, k as i32 * 20, k as i32 * 20 + 10, 0.9, "regex").unwrap();
    }
    let emb = QvacEmbedder { service: Arc::clone(&svc), model: model.clone() };
    let rep = service::regroup(&db, Some(&emb), Some(&model), &ResolveConfig::default()).unwrap();
    eprintln!("regroup: {:?}", rep);
    assert_eq!(rep.embedded, 4);
    assert!(rep.embedding_error.is_none());
    assert_eq!(rep.groups, 3);
    assert!(service::list_groups(&db).unwrap().iter().all(|g| g.variants.iter().all(|v| v.has_embedding)));

    let rep = svc.self_test(None);
    eprintln!("self-test: {:?}", rep);
    assert!(rep.embedding_error.is_none() && rep.embedding_dims.unwrap_or(0) > 0);

    // Turned off: no model is used (grouping falls back to rules).
    svc.set_entity_embeddings(false).unwrap();
    assert!(svc.embedding_model().is_none());
    svc.shutdown();
    let _ = std::fs::remove_dir_all(&app_data);
}
