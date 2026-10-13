//! Opt-in golden tests for Privacy Filter with the real model files
//! (docs/qvac-integration.md §15).
//!
//!   QVAULT_PF_MODEL_DIR=/dir/with/model_quantized.onnx,model_quantized.onnx_data,tokenizer.json,viterbi_calibration.json \
//!   cargo test --release --test privacy_filter_it -- --nocapture --test-threads 1
//!
//! * `tokenizer_matches_tiktoken`: token ids equal tiktoken's o200k_base on
//!   every eval page and a set of edge cases (`tests/fixtures/privacy-filter/o200k_ids.json`).
//! * `spans_match_opf_reference`: every eval page through the real sidecar
//!   (`@qvac/onnx`) and Qvault's tokenizer and decoder gives exactly the
//!   spans of OpenAI's opf decoding of the same engine's logits
//!   (`reference_spans_qvac_onnx.json`), at the published operating point and
//!   at a recall-leaning one; agreement with opf on Python onnxruntime's
//!   logits (`reference_spans.json`, the §14 eval) is reported and must stay
//!   at least 97 %. Needs the sidecar (`npm run sidecar:build`). With
//!   `QVAULT_PF_DUMP=/dir` it also writes ids and logits per page.
//! * `app_path_classifies_through_the_service`: install the model files into
//!   a throw-away models dir and run a page through `QvacService` as the app
//!   does (load, SHA-256 checks, classify, decode, post-process).

use std::path::PathBuf;

use serde_json::Value;

use app_lib::privacy_filter::decoder::{self, Biases, Decode};
use app_lib::privacy_filter::tokenizer::O200kTokenizer;

fn model_dir() -> Option<PathBuf> {
    std::env::var_os("QVAULT_PF_MODEL_DIR").map(PathBuf::from)
}

fn fixture(name: &str) -> Value {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/privacy-filter").join(name);
    serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap()
}

fn eval_pages() -> Vec<(String, Vec<String>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../eval/fixtures");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().map_or(false, |e| e == "json"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|p| {
            let v: Value = serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap();
            let pages = v["pages"].as_array().unwrap().iter().map(|pg| pg["text"].as_str().unwrap().to_string()).collect();
            (v["id"].as_str().unwrap().to_string(), pages)
        })
        .collect()
}

#[test]
fn tokenizer_matches_tiktoken() {
    let Some(dir) = model_dir() else {
        eprintln!("skipping: set QVAULT_PF_MODEL_DIR");
        return;
    };
    let t = std::time::Instant::now();
    let tok = O200kTokenizer::from_file(&dir.join("tokenizer.json")).expect("tokenizer.json");
    eprintln!("tokenizer loaded in {:.2}s, {} entries", t.elapsed().as_secs_f64(), tok.vocab_size());
    let golden = fixture("o200k_ids.json");
    let flat: Vec<(String, String)> =
        eval_pages().into_iter().flat_map(|(id, ps)| ps.into_iter().map(move |p| (id.clone(), p))).collect();
    let want_pages = golden["pages"].as_array().unwrap();
    assert_eq!(want_pages.len(), flat.len());
    let mut n = 0;
    let mut tokens = 0;
    for (want, (id, text)) in want_pages.iter().zip(&flat) {
        assert_eq!(want["fixture"].as_str().unwrap(), id);
        let ids: Vec<u32> = serde_json::from_value(want["ids"].clone()).unwrap();
        let got = tok.encode(text);
        let got_ids: Vec<u32> = got.iter().map(|t| t.id).collect();
        assert_eq!(got_ids, ids, "{} page {}", id, want["page"]);
        // offsets tile the text
        let mut pos = 0;
        for t in &got {
            assert_eq!(t.start, pos);
            pos = t.end;
        }
        assert_eq!(pos, text.len());
        n += 1;
        tokens += ids.len();
    }
    for s in golden["strings"].as_array().unwrap() {
        let text = s["text"].as_str().unwrap();
        let ids: Vec<u32> = serde_json::from_value(s["ids"].clone()).unwrap();
        let got: Vec<u32> = tok.encode(text).iter().map(|t| t.id).collect();
        assert_eq!(got, ids, "{:?}", text);
    }
    eprintln!("{} pages ({} tokens) and {} strings identical to tiktoken", n, tokens, golden["strings"].as_array().unwrap().len());
}

#[test]
fn reference_spans_fixture_is_consistent() {
    // Model-free: the reference file covers every eval page and its spans
    // sit on character boundaries of the page text.
    let reference = fixture("reference_spans.json");
    let pages = eval_pages();
    for (op, v) in reference["operating_points"].as_object().unwrap() {
        let fx = v["fixtures"].as_object().unwrap();
        assert_eq!(fx.len(), pages.len(), "{}", op);
        for (id, texts) in &pages {
            let ps = fx[id].as_array().unwrap();
            assert_eq!(ps.len(), texts.len());
            for (spans, text) in ps.iter().zip(texts) {
                for s in spans.as_array().unwrap() {
                    let (a, b) = (s[0].as_u64().unwrap() as usize, s[1].as_u64().unwrap() as usize);
                    assert!(a < b && b <= text.len() && text.is_char_boundary(a) && text.is_char_boundary(b));
                    assert!(decoder::SPAN_LABELS.contains(&s[2].as_str().unwrap()));
                }
            }
        }
    }
    let _ = Decode::Argmax;
    let _ = Biases::default();
}

#[cfg(desktop)]
mod with_sidecar {
    use super::*;
    use std::sync::Arc;

    use app_lib::privacy_filter::scanner::{self, PfConfig};
    use app_lib::qvac::{catalog, QvacService, SidecarPaths};

    fn sidecar() -> SidecarPaths {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let program = manifest
            .join("binaries")
            .join(format!("qvac-sidecar-{}{}", env!("QVAULT_TARGET_TRIPLE"), std::env::consts::EXE_SUFFIX));
        let worker = manifest.join("resources").join("qvac-worker").join("worker.js");
        SidecarPaths::checked(program.clone(), worker.clone())
            .unwrap_or_else(|| panic!("sidecar not built: {:?} / {:?} (run `npm run sidecar:build`)", program, worker))
    }

    /// A throw-away app-data dir whose models dir holds the Privacy Filter
    /// files (symlinked) in the catalog layout.
    fn service(tag: &str) -> (Arc<QvacService>, PathBuf) {
        let dir = model_dir().unwrap();
        let root = std::env::temp_dir().join(format!("qvault-pf-it-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let models = root.join("models");
        let m = catalog::privacy_filter();
        let target = models.join(m.dir.unwrap());
        std::fs::create_dir_all(&target).unwrap();
        for f in m.files() {
            let src = [dir.join(f.name), dir.join("onnx").join(f.name)].into_iter().find(|p| p.is_file()).unwrap_or_else(|| panic!("{} missing in QVAULT_PF_MODEL_DIR", f.name));
            #[cfg(unix)]
            std::os::unix::fs::symlink(&src, target.join(f.name)).unwrap();
            #[cfg(not(unix))]
            std::fs::copy(&src, target.join(f.name)).unwrap();
        }
        let svc = Arc::new(QvacService::new(&root, Some(&models), Some(sidecar())));
        (svc, root)
    }

    #[test]
    fn spans_match_opf_reference() {
        if model_dir().is_none() {
            eprintln!("skipping: set QVAULT_PF_MODEL_DIR");
            return;
        }
        let (svc, root) = service("golden");
        let session = svc.prepare_privacy_filter().expect("Privacy Filter loads");
        let pages = eval_pages();
        let mut checked = 0;
        let t = std::time::Instant::now();
        let mut scored = Vec::new();
        for (id, texts) in &pages {
            for (pi, text) in texts.iter().enumerate() {
                let scores = scanner::score_page(&session, session.tokenizer(), text).expect("classify");
                scored.push((id.clone(), pi, text.clone(), scores));
            }
        }
        eprintln!("scored {} pages in {:.2}s", scored.len(), t.elapsed().as_secs_f64());
        // Diagnostics: write ids, logits (f32 LE) and Qvault's spans per page,
        // so OpenAI's decoder can be run on exactly these logits.
        if let Some(out) = std::env::var_os("QVAULT_PF_DUMP") {
            let out = PathBuf::from(out);
            std::fs::create_dir_all(&out).unwrap();
            for (id, pi, text, scores) in &scored {
                let stem = format!("{}_{}", id, pi);
                let ids: Vec<u32> = scores.tokens.iter().map(|t| t.id).collect();
                std::fs::write(out.join(format!("{}.ids.json", stem)), serde_json::to_vec(&ids).unwrap()).unwrap();
                let bytes: Vec<u8> = scores.logits.iter().flat_map(|f| f.to_le_bytes()).collect();
                std::fs::write(out.join(format!("{}.logits.f32", stem)), bytes).unwrap();
                let spans: Vec<(usize, usize, &str)> = decoder::decode(text, &scores.tokens, &scores.logits, &Decode::Viterbi(Biases::default()))
                    .iter()
                    .map(|s| (s.start, s.end, s.label_name()))
                    .collect();
                std::fs::write(out.join(format!("{}.qvault.json", stem)), serde_json::to_vec(&spans).unwrap()).unwrap();
            }
        }
        type Spans = Vec<(usize, usize, String)>;
        let want_of = |v: &Value, id: &str, pi: usize| -> Spans {
            v["fixtures"][id][pi]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| (s[0].as_u64().unwrap() as usize, s[1].as_u64().unwrap() as usize, s[2].as_str().unwrap().to_string()))
                .collect()
        };
        // 1. Strict: opf's decoder on the logits of the engine Qvault ships
        //    (@qvac/onnx 0.15.1). Tokenizer + engine + decoder, end to end.
        let strict = fixture("reference_spans_qvac_onnx.json");
        // 2. Reported: opf on Python onnxruntime 1.31 logits (the §14 eval).
        //    The two ONNX Runtime builds round the int8 MoE differently, so a
        //    few near-tie spans differ; agreement must stay high.
        let python = fixture("reference_spans.json");
        for (op, v) in strict["operating_points"].as_object().unwrap() {
            let biases: Biases = serde_json::from_value(v["biases"].clone()).unwrap();
            let mut mismatches = Vec::new();
            let (mut same_py, mut union_py) = (0usize, 0usize);
            for (id, pi, text, scores) in &scored {
                let got: Spans = decoder::decode(text, &scores.tokens, &scores.logits, &Decode::Viterbi(biases))
                    .into_iter()
                    .map(|s| (s.start, s.end, s.label_name().to_string()))
                    .collect();
                let want = want_of(v, id, *pi);
                if got != want {
                    mismatches.push(format!("{} p{}:\n  got  {:?}\n  want {:?}", id, pi, got, want));
                }
                checked += want.len();
                let py = want_of(&python["operating_points"][op.as_str()], id, *pi);
                same_py += got.iter().filter(|g| py.contains(g)).count();
                union_py += got.len() + py.iter().filter(|p| !got.contains(p)).count();
            }
            assert!(mismatches.is_empty(), "operating point {}: {} page(s) differ\n{}", op, mismatches.len(), mismatches.join("\n"));
            let jaccard = same_py as f64 / union_py.max(1) as f64;
            eprintln!(
                "operating point {}: identical to opf on the shipped engine's logits on all pages; {:.1} % of spans identical to opf on Python onnxruntime ({} of {})",
                op,
                100.0 * jaccard,
                same_py,
                union_py
            );
            assert!(jaccard >= 0.97, "agreement with the Python reference fell to {:.3}", jaccard);
        }
        eprintln!("{} reference spans checked", checked);
        svc.shutdown();
        let _ = std::fs::remove_dir_all(root);
    }

    /// Import from a folder in the Hugging Face layout (tokenizer.json and
    /// viterbi_calibration.json at the root, the ONNX files under onnx/;
    /// other files ignored), then from single files one at a time.
    #[test]
    fn folder_and_file_imports_install_the_model() {
        let Some(dir) = model_dir() else {
            eprintln!("skipping: set QVAULT_PF_MODEL_DIR");
            return;
        };
        use app_lib::qvac::models::{ImportOutcome, ModelStore};
        let root = std::env::temp_dir().join(format!("qvault-pf-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = ModelStore::new(&root);
        let pf = catalog::privacy_filter();
        let t = std::time::Instant::now();
        let sum = store.import_paths(&[dir.clone()], None).expect("folder import");
        eprintln!("folder import in {:.1}s: {:?}", t.elapsed().as_secs_f64(), sum);
        assert!(matches!(&sum.outcomes[..], [ImportOutcome::Installed { id }] if id == pf.id));
        assert!(sum.ignored > 0, "the folder's other files (README, other quantisations) are ignored");
        assert!(store.is_installed(pf));
        assert_eq!(store.verify_model(pf).unwrap(), None);
        store.delete(pf.id).unwrap();
        assert!(!store.is_installed(pf));
        // one file at a time: incomplete until the last one
        let files = [dir.join("tokenizer.json"), dir.join("viterbi_calibration.json"), dir.join("onnx/model_quantized.onnx")];
        for f in &files {
            assert!(matches!(store.import(f, false).unwrap(), ImportOutcome::Incomplete { .. }));
        }
        assert!(matches!(store.import(&dir.join("onnx/model_quantized.onnx_data"), false).unwrap(), ImportOutcome::Installed { .. }));
        assert!(store.is_installed(pf));
        let _ = std::fs::remove_dir_all(root);
    }

    /// Real download from Hugging Face (1.65 GB): cancel once, then download
    /// fully and verify. Opt-in: QVAULT_PF_DOWNLOAD_IT=1.
    #[test]
    fn download_cancel_then_install() {
        if std::env::var("QVAULT_PF_DOWNLOAD_IT").map(|v| v != "1").unwrap_or(true) {
            eprintln!("skipping: set QVAULT_PF_DOWNLOAD_IT=1 (downloads 1.65 GB)");
            return;
        }
        use app_lib::qvac::client::CancelToken;
        use app_lib::qvac::models::{ModelError, ModelStore};
        let root = std::env::temp_dir().join(format!("qvault-pf-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let store = ModelStore::new(&root);
        let pf = catalog::privacy_filter();
        let cancel = CancelToken::new();
        let c2 = cancel.clone();
        let err = store
            .download(pf, &cancel, move |rx, _total| {
                if rx > 50_000_000 {
                    c2.cancel();
                }
            })
            .unwrap_err();
        assert_eq!(err, ModelError::Cancelled);
        assert!(!store.is_installed(pf));
        assert!(!root.join("models/.partial/openai-privacy-filter-int8").exists(), "staging removed after cancel");
        let t = std::time::Instant::now();
        let mut last = (0u64, 0u64);
        let path = store.download(pf, &CancelToken::new(), |rx, total| last = (rx, total)).expect("download");
        eprintln!("downloaded {} bytes in {:.0}s", last.0, t.elapsed().as_secs_f64());
        assert_eq!(last, (pf.total_size(), pf.total_size()));
        assert!(path.ends_with("openai-privacy-filter-int8/model_quantized.onnx"));
        assert!(store.is_installed(pf));
        assert_eq!(store.verify_model(pf).unwrap(), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn app_path_classifies_through_the_service() {
        if model_dir().is_none() {
            eprintln!("skipping: set QVAULT_PF_MODEL_DIR");
            return;
        }
        let (svc, root) = service("app");
        let session = svc.prepare_privacy_filter().expect("Privacy Filter loads");
        let text = "Witness statement of Dr. Amara Okafor, born 14/02/1979, of 12 Harbour Row, Leith, \
                    e-mail amara.okafor@example.org, passport no. K4417781.";
        let r = scanner::scan(&session, session.tokenizer(), session.calibration(), text, 0, &[], &PfConfig::default()).unwrap();
        let got: Vec<String> = r.entities.iter().map(|e| format!("{}: {}", e.category.to_string(), e.text)).collect();
        eprintln!("{:?}", got);
        assert!(got.iter().any(|g| g == "PersonName: Amara Okafor"), "honorific trimmed: {:?}", got);
        assert!(got.iter().any(|g| g.starts_with("DateOfBirth: 14/02/1979")), "{:?}", got);
        assert!(r.entities.iter().all(|e| e.layer == "privacy-filter"));
        svc.shutdown();
        let _ = std::fs::remove_dir_all(root);
    }
}
