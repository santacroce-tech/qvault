//! Opt-in: score predictions produced outside Qvault (another model, another
//! runtime) with the harness's own scoring, as a stand-in "Layer 3".
//!
//! `QVAULT_EVAL_EXTERNAL=/abs/a.json[,/abs/b.json…]`. Each file:
//!
//! ```json
//! {"system": "name", "meta": {…anything…},
//!  "fixtures": {"<fixture id>": [[{"start": 0, "end": 4, "category": "PersonName", "text": "…"}], …pages]}}
//! ```
//!
//! `start`/`end` are UTF-8 byte offsets into the fixture page text, exactly
//! as in `eval/fixtures/*.json`; `category` is a Qvault `PiiCategory` name.
//! Every span is validated (range, char boundary, `text` equals the slice,
//! known category) and the run fails on any mismatch, so an offset bug in the
//! producer cannot silently inflate or deflate the score.
//!
//! Reported per file: the predictions alone, and Layers 1+2 plus the external
//! predictions combined the way the app combines Layer 3 (an external span
//! that overlaps a Layer 1–2 detection is dropped, as in
//! `llm_scanner::scan_with`); also Layer 1 alone plus the external
//! predictions, i.e. without Layer 2's name heuristic. Report:
//! `external-<system>.{md,json}` in `QVAULT_EVAL_OUT`.

use serde_json::{json, Value};

use super::*;

fn load_external(path: &str, fixtures: &[Fixture]) -> (String, Value, Predictions) {
    let raw = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {}", path, e));
    let v: Value = serde_json::from_slice(&raw).unwrap_or_else(|e| panic!("{}: {}", path, e));
    let system = v["system"].as_str().unwrap_or("external").to_string();
    let by_id = v["fixtures"].as_object().unwrap_or_else(|| panic!("{}: no \"fixtures\" object", path));
    let mut errs = Vec::new();
    let preds: Predictions = fixtures
        .iter()
        .map(|fx| {
            let pages = by_id.get(&fx.id).and_then(|p| p.as_array());
            let Some(pages) = pages else {
                errs.push(format!("{}: missing", fx.id));
                return vec![vec![]; fx.pages.len()];
            };
            if pages.len() != fx.pages.len() {
                errs.push(format!("{}: {} pages, fixture has {}", fx.id, pages.len(), fx.pages.len()));
            }
            fx.pages
                .iter()
                .enumerate()
                .map(|(pi, page)| {
                    let text = &page.text;
                    let items = pages.get(pi).and_then(|p| p.as_array()).cloned().unwrap_or_default();
                    items
                        .iter()
                        .filter_map(|it| {
                            let start = it["start"].as_u64()? as usize;
                            let end = it["end"].as_u64()? as usize;
                            let category = it["category"].as_str().unwrap_or("").to_string();
                            let w = format!("{} p{} {}..{}", fx.id, pi + 1, start, end);
                            if start >= end || end > text.len() || !text.is_char_boundary(start) || !text.is_char_boundary(end) {
                                errs.push(format!("{}: bad range", w));
                                return None;
                            }
                            if let Some(t) = it["text"].as_str() {
                                if &text[start..end] != t {
                                    errs.push(format!("{}: slice is {:?}, prediction says {:?}", w, &text[start..end], t));
                                }
                            }
                            if !CATEGORIES.contains(&category.as_str()) {
                                errs.push(format!("{}: unknown category {:?}", w, category));
                            }
                            Some(Pred {
                                start,
                                end,
                                text: text[start..end].to_string(),
                                category,
                                layer: "external".into(),
                                confidence: it["score"].as_f64().unwrap_or(1.0),
                            })
                        })
                        .collect()
                })
                .collect()
        })
        .collect();
    assert!(errs.is_empty(), "{}: invalid external predictions:\n{}", path, errs.join("\n"));
    (system, v.get("meta").cloned().unwrap_or(Value::Null), preds)
}

/// Layers 1+2, then every external span that does not overlap one of them.
fn combine(l12: &Predictions, ext: &Predictions) -> Predictions {
    l12.iter()
        .zip(ext)
        .map(|(fx_a, fx_b)| {
            fx_a.iter()
                .zip(fx_b)
                .map(|(a, b)| {
                    let mut all = a.clone();
                    all.extend(b.iter().filter(|p| !a.iter().any(|q| p.start < q.end && q.start < p.end)).cloned());
                    all
                })
                .collect()
        })
        .collect()
}

#[test]
fn external_predictions() {
    let Ok(list) = std::env::var("QVAULT_EVAL_EXTERNAL") else {
        eprintln!("skipping: set QVAULT_EVAL_EXTERNAL=/abs/predictions.json[,…] to score external predictions");
        return;
    };
    let fixtures = load_fixtures();
    let l12_preds = run_system(&fixtures, layers12);
    let l12 = score(&fixtures, &l12_preds);
    // Layer 1 without Layer 2's name heuristic (Layer 2 is the main source of
    // false positives, §11.4): what a strong external model could replace.
    let l1_preds = run_system(&fixtures, layer1);
    let out = out_dir();
    for path in list.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let (system, meta, ext) = load_external(path, &fixtures);
        let alone = score(&fixtures, &ext);
        let comb = score(&fixtures, &combine(&l12_preds, &ext));
        let comb1 = score(&fixtures, &combine(&l1_preds, &ext));

        let mut md = format!("# Eval: external predictions, {}\n\n", system);
        md.push_str(&format!(
            "Generated by `QVAULT_EVAL_EXTERNAL=… cargo test --test eval_harness external_predictions`. Scored like Layer 3: alone, and added to Layers 1+2 (spans overlapping a Layer 1–2 detection are dropped, as in the app). Headline = overlap matching; see eval/README.md.\n\nSource: `{}`\n\n",
            std::path::Path::new(path).file_name().unwrap().to_string_lossy()
        ));
        md.push_str("## Overall\n\n");
        md.push_str(TABLE_HEAD);
        md.push_str(&row("Layers 1+2", &l12.overall));
        md.push_str(&row("External alone", &alone.overall));
        md.push_str(&row("Layers 1+2 + external", &comb.overall));
        md.push_str(&row("Layer 1 + external (no Layer 2)", &comb1.overall));
        md.push_str("\n## By language and split\n\n");
        md.push_str(TABLE_HEAD);
        for (name, r) in [("external", &alone), ("L1+2+ext", &comb), ("L1+ext", &comb1)] {
            for (k, b) in r.by_lang.iter().chain(r.by_split.iter()) {
                md.push_str(&row(&format!("{} – {}", name, k), b));
            }
        }
        md.push_str("\n## By category (overlap)\n\n");
        md.push_str(&category_table(&[("L1+2", &l12), ("ext", &alone), ("L1+2+ext", &comb)]));
        md.push_str("\n## External alone: errors\n\n");
        md.push_str(&lists(&alone, usize::MAX));
        md.push_str("\n## Layers 1+2 + external: errors\n\n");
        md.push_str(&lists(&comb, usize::MAX));
        let js = json!({
            "system": system,
            "source": path,
            "meta": meta,
            "layers12": summary_json(&l12),
            "external": summary_json(&alone),
            "layers12_external": summary_json(&comb),
            "layer1_external": summary_json(&comb1),
        });
        let name: String = system
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_' { c } else { '_' })
            .collect();
        write(&out, &format!("external-{}", name), &md, &js);
        eprintln!(
            "{}: alone P {} R {} F1 {} cov {} | L1+2+ext P {} R {} F1 {} cov {} | L1+ext P {} R {} F1 {}",
            system,
            pct(alone.overall.overlap.precision()),
            pct(alone.overall.overlap.recall()),
            pct(alone.overall.overlap.f1()),
            pct(alone.overall.covered_recall()),
            pct(comb.overall.overlap.precision()),
            pct(comb.overall.overlap.recall()),
            pct(comb.overall.overlap.f1()),
            pct(comb.overall.covered_recall()),
            pct(comb1.overall.overlap.precision()),
            pct(comb1.overall.overlap.recall()),
            pct(comb1.overall.overlap.f1()),
        );
    }
}

#[test]
fn combine_drops_overlaps_like_the_app() {
    let p = |s: usize, e: usize| Pred { start: s, end: e, category: "PersonName".into(), text: String::new(), layer: "x".into(), confidence: 1.0 };
    let l12 = vec![vec![vec![p(0, 5), p(10, 15)]]];
    let ext = vec![vec![vec![p(3, 8), p(15, 20), p(30, 35)]]];
    let c = combine(&l12, &ext);
    let got: Vec<(usize, usize)> = c[0][0].iter().map(|x| (x.start, x.end)).collect();
    assert_eq!(got, vec![(0, 5), (10, 15), (15, 20), (30, 35)]);
}
