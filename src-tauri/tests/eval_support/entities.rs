//! Phase 4 evaluation (see eval/README.md):
//!
//! * `entity_resolution_against_baseline` (CI, no model): pairwise and
//!   B-cubed precision/recall of entity grouping on `eval/entities/variants.json`,
//!   rules and string similarity only, against `eval/entity-baseline.json`.
//! * `known_entity_suggestions_against_baseline` (CI, Layers 1–2): how many
//!   labels missed by the detection layers the known-entity suggestions
//!   recover on the detection fixtures, and how many false suggestions they
//!   add, in three knowledge-base scenarios.
//! * `entity_resolution_embeddings` (opt-in, `QVAULT_EVAL_EMBED=1`): the same
//!   grouping with embedding models through the real sidecar (threshold
//!   sweep, speed, memory).
//! * `known_entity_suggestions_layer3` (opt-in, `QVAULT_EVAL_L3=1`): the
//!   suggestions on top of Layers 1–3 with a real model.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use app_lib::entities::known::{self, Known};
use app_lib::entities::resolve::{self, Item, ResolveConfig, Resolution};
use app_lib::entities::embedding_input;
use app_lib::llm::llm_scanner;
use app_lib::qvac::QvacService;

use super::layer3::{install, l3_preds, sidecar, MemSampler, ModelSpec};
use super::*;

// ── entity-resolution set ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct SetEntity {
    id: String,
    lang: String,
    category: String,
    mentions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct EntitySet {
    entities: Vec<SetEntity>,
}

struct Mention {
    gold: usize,
    item: usize,
    category: String,
    lang: String,
}

struct Prepared {
    entities: Vec<SetEntity>,
    mentions: Vec<Mention>,
    items: Vec<Item>,
}

fn prepare() -> Prepared {
    let path = eval_dir().join("entities").join("variants.json");
    let set: EntitySet = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let mut items: Vec<Item> = Vec::new();
    let mut index: HashMap<(String, String), usize> = HashMap::new();
    let mut mentions = Vec::new();
    for (g, e) in set.entities.iter().enumerate() {
        assert!(CATEGORIES.contains(&e.category.as_str()), "{}: unknown category", e.id);
        for m in &e.mentions {
            // One variant per distinct string, as in the app's database.
            let key = (e.category.clone(), m.trim().to_string());
            let item = *index.entry(key).or_insert_with(|| {
                items.push(Item::new(&e.category, m.trim()));
                items.len() - 1
            });
            mentions.push(Mention { gold: g, item, category: e.category.clone(), lang: e.lang.clone() });
        }
    }
    Prepared { entities: set.entities, mentions, items }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct PairScore {
    pub tp: usize,
    pub fp: usize,
    #[serde(rename = "fn")]
    pub fn_: usize,
}

impl PairScore {
    fn p(&self) -> f64 {
        ratio(self.tp, self.tp + self.fp)
    }
    fn r(&self) -> f64 {
        ratio(self.tp, self.tp + self.fn_)
    }
    fn f1(&self) -> f64 {
        let (p, r) = (self.p(), self.r());
        if p + r == 0.0 { 0.0 } else { 2.0 * p * r / (p + r) }
    }
}

/// Pairwise over mention pairs of one category (cross-category pairs are
/// never linked by construction and are checked separately).
fn pairwise(m: &[Mention], pred: &dyn Fn(usize) -> usize, filter: &dyn Fn(&Mention) -> bool) -> PairScore {
    let mut s = PairScore::default();
    for i in 0..m.len() {
        if !filter(&m[i]) {
            continue;
        }
        for j in i + 1..m.len() {
            if !filter(&m[j]) || m[i].category != m[j].category {
                continue;
            }
            let g = m[i].gold == m[j].gold;
            let p = pred(m[i].item) == pred(m[j].item);
            match (g, p) {
                (true, true) => s.tp += 1,
                (false, true) => s.fp += 1,
                (true, false) => s.fn_ += 1,
                _ => {}
            }
        }
    }
    s
}

/// B-cubed precision and recall (per mention, averaged; within category).
fn bcubed(m: &[Mention], pred: &dyn Fn(usize) -> usize) -> (f64, f64) {
    let (mut p, mut r) = (0.0, 0.0);
    for a in m {
        let same_pred: Vec<&Mention> = m.iter().filter(|b| b.category == a.category && pred(b.item) == pred(a.item)).collect();
        let same_gold: Vec<&Mention> = m.iter().filter(|b| b.category == a.category && b.gold == a.gold).collect();
        let both = same_pred.iter().filter(|b| b.gold == a.gold).count() as f64;
        p += both / same_pred.len() as f64;
        r += both / same_gold.len() as f64;
    }
    (p / m.len() as f64, r / m.len() as f64)
}

/// Clusters after a reviewer accepted every proposal.
fn accept_all(res: &Resolution) -> Vec<usize> {
    accept_where(res, &|_| true)
}

/// Clusters after a reviewer accepted the proposals `keep` says yes to.
fn accept_where(res: &Resolution, keep: &dyn Fn(&resolve::ProposalIdx) -> bool) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..res.clusters).collect();
    fn find(p: &mut Vec<usize>, x: usize) -> usize {
        let mut r = x;
        while p[r] != r {
            r = p[r];
        }
        p[x] = r;
        r
    }
    for pr in res.proposals.iter().filter(|p| keep(p)) {
        let (a, b) = (find(&mut parent, res.cluster[pr.a]), find(&mut parent, res.cluster[pr.b]));
        if a != b {
            parent[a] = b;
        }
    }
    (0..res.cluster.len()).map(|i| find(&mut parent, res.cluster[i])).collect()
}

#[derive(Debug, Clone, Serialize)]
struct SystemResult {
    name: String,
    auto: PairScore,
    /// A reviewer accepts the correct proposals and dismisses the wrong ones.
    reviewed: PairScore,
    accepted: PairScore,
    bcubed_auto: (f64, f64),
    proposals: usize,
    proposals_correct: usize,
    ambiguous: usize,
    ambiguous_correct: usize,
    by_lang_auto: BTreeMap<String, PairScore>,
    by_category_auto: BTreeMap<String, PairScore>,
    by_category_accepted: BTreeMap<String, PairScore>,
    rules_of_wrong_merges: BTreeMap<String, usize>,
}

fn evaluate(name: &str, p: &Prepared, items: &[Item], cfg: &ResolveConfig) -> (SystemResult, Resolution) {
    let res = resolve::resolve(items, &HashSet::new(), cfg);
    let acc = accept_all(&res);
    let auto_of = |i: usize| res.cluster[i];
    let acc_of = |i: usize| acc[i];
    let all = |_: &Mention| true;
    let mut by_lang = BTreeMap::new();
    for l in ["en", "es", "pt"] {
        by_lang.insert(l.to_string(), pairwise(&p.mentions, &auto_of, &|m: &Mention| m.lang == l));
    }
    let cats: Vec<String> = {
        let mut c: Vec<String> = p.mentions.iter().map(|m| m.category.clone()).collect();
        c.sort();
        c.dedup();
        c
    };
    let mut by_cat = BTreeMap::new();
    let mut by_cat_acc = BTreeMap::new();
    for c in &cats {
        by_cat.insert(c.clone(), pairwise(&p.mentions, &auto_of, &|m: &Mention| &m.category == c));
        by_cat_acc.insert(c.clone(), pairwise(&p.mentions, &acc_of, &|m: &Mention| &m.category == c));
    }
    // Gold entity of an item: majority of its mentions (identical strings of
    // two entities are ambiguous by nature).
    let item_gold = |i: usize| -> HashSet<usize> { p.mentions.iter().filter(|m| m.item == i).map(|m| m.gold).collect() };
    let proposals_correct = res.proposals.iter().filter(|pr| !item_gold(pr.a).is_disjoint(&item_gold(pr.b))).count();
    let rev = accept_where(&res, &|pr| !item_gold(pr.a).is_disjoint(&item_gold(pr.b)));
    let rev_of = |i: usize| rev[i];
    let mut wrong: BTreeMap<String, usize> = BTreeMap::new();
    for (a, b, rule) in &res.merges {
        if item_gold(*a).is_disjoint(&item_gold(*b)) {
            *wrong.entry(rule.as_str().to_string()).or_default() += 1;
        }
    }
    (
        SystemResult {
            name: name.to_string(),
            auto: pairwise(&p.mentions, &auto_of, &all),
            reviewed: pairwise(&p.mentions, &rev_of, &all),
            accepted: pairwise(&p.mentions, &acc_of, &all),
            bcubed_auto: bcubed(&p.mentions, &auto_of),
            proposals: res.proposals.len(),
            proposals_correct,
            ambiguous: res.proposals.iter().filter(|p| p.ambiguous).count(),
            ambiguous_correct: res.proposals.iter().filter(|pr| pr.ambiguous && !item_gold(pr.a).is_disjoint(&item_gold(pr.b))).count(),
            by_lang_auto: by_lang,
            by_category_auto: by_cat,
            by_category_accepted: by_cat_acc,
            rules_of_wrong_merges: wrong,
        },
        res,
    )
}

fn sys_row(r: &SystemResult) -> String {
    format!(
        "| {} | {} / {} / {} | {} / {} | {} / {} | {} / {} / {} | {} ({} correct, {}; {} flagged ambiguous, {} correct) |\n",
        r.name,
        pct(r.auto.p()), pct(r.auto.r()), pct(r.auto.f1()),
        pct(r.bcubed_auto.0), pct(r.bcubed_auto.1),
        pct(r.reviewed.p()), pct(r.reviewed.r()),
        pct(r.accepted.p()), pct(r.accepted.r()), pct(r.accepted.f1()),
        r.proposals, r.proposals_correct, pct(ratio(r.proposals_correct, r.proposals)), r.ambiguous, r.ambiguous_correct,
    )
}

const SYS_HEAD: &str = "| system | applied automatically: pairwise P / R / F1 | B³ P / R | after review (correct proposals accepted): P / R | if every proposal is accepted: P / R / F1 | proposals (correct, precision) |\n|---|---|---|---|---|---|\n";

fn listing(p: &Prepared, items: &[Item], res: &Resolution) -> String {
    let mut s = String::from("Proposals:\n\n");
    let gold_name = |i: usize| -> String {
        let mut g: Vec<&str> = p.mentions.iter().filter(|m| m.item == i).map(|m| p.entities[m.gold].id.as_str()).collect();
        g.dedup();
        g.join("/")
    };
    for pr in &res.proposals {
        let ok = gold_name(pr.a).split('/').any(|x| gold_name(pr.b).split('/').any(|y| x == y));
        s.push_str(&format!(
            "- {} `{}` ↔ `{}` ({}{}{}){}\n",
            if ok { "✓" } else { "✗" },
            items[pr.a].text,
            items[pr.b].text,
            pr.rule.as_str(),
            if pr.ambiguous { ", ambiguous" } else { "" },
            pr.cosine.map(|c| format!(", cos {:.3}", c)).unwrap_or_default(),
            if ok { String::new() } else { format!(" — {} vs {}", gold_name(pr.a), gold_name(pr.b)) }
        ));
    }
    s.push_str("\nWrong automatic merges:\n\n");
    let mut any = false;
    for (a, b, rule) in &res.merges {
        let (ga, gb) = (gold_name(*a), gold_name(*b));
        if !ga.split('/').any(|x| gb.split('/').any(|y| x == y)) {
            any = true;
            s.push_str(&format!("- `{}` + `{}` ({}) — {} vs {}\n", items[*a].text, items[*b].text, rule.as_str(), ga, gb));
        }
    }
    if !any {
        s.push_str("- none\n");
    }
    // Same-entity pairs left apart (neither merged nor proposed).
    s.push_str("\nVariants left apart (not merged, not proposed), per entity:\n\n");
    let acc = accept_all(res);
    for (g, e) in p.entities.iter().enumerate() {
        let its: Vec<usize> = {
            let mut v: Vec<usize> = p.mentions.iter().filter(|m| m.gold == g).map(|m| m.item).collect();
            v.dedup();
            v
        };
        let mut groups: BTreeMap<usize, Vec<&str>> = BTreeMap::new();
        for i in its {
            groups.entry(acc[i]).or_default().push(items[i].text.as_str());
        }
        if groups.len() > 1 {
            let parts: Vec<String> = groups.values().map(|v| format!("[{}]", v.join(", "))).collect();
            s.push_str(&format!("- `{}`: {}\n", e.id, parts.join(" · ")));
        }
    }
    s
}

#[derive(Debug, Serialize, Deserialize)]
struct EntityBaseline {
    /// Allowed absolute drop of pairwise precision / recall.
    tolerance: f64,
    auto_precision: f64,
    auto_recall: f64,
    reviewed_recall: f64,
    accepted_precision: f64,
    accepted_recall: f64,
    proposal_precision: f64,
    known_entity: BTreeMap<String, KnownBaseline>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KnownBaseline {
    recovered: usize,
    false_suggestions: usize,
}

fn baseline_path() -> PathBuf {
    eval_dir().join("entity-baseline.json")
}

fn load_baseline() -> Option<EntityBaseline> {
    serde_json::from_slice(&std::fs::read(baseline_path()).ok()?).ok()
}

fn update_baseline() -> bool {
    std::env::var("QVAULT_EVAL_UPDATE_BASELINE").map(|v| v == "1").unwrap_or(false)
}

fn save_baseline(b: &EntityBaseline) {
    std::fs::write(baseline_path(), serde_json::to_string_pretty(b).unwrap() + "\n").unwrap();
    eprintln!("baseline updated: {}", baseline_path().display());
}

fn r4(x: f64) -> f64 {
    (x * 10000.0).round() / 10000.0
}

#[test]
fn entity_resolution_against_baseline() {
    let p = prepare();
    let (r, res) = evaluate("rules + string similarity (no model)", &p, &p.items, &ResolveConfig::rules_only());
    // Never across categories.
    for (a, b, _) in &res.merges {
        assert_eq!(p.items[*a].category, p.items[*b].category);
    }
    for pr in &res.proposals {
        assert_eq!(p.items[pr.a].category, p.items[pr.b].category);
    }
    let mut md = String::from("# Eval: entity resolution (no model)\n\n");
    md.push_str(&format!(
        "`eval/entities/variants.json`: {} entities, {} mentions ({} distinct strings). Pairwise = mention pairs of the same category. *Applied automatically* is what the app does on its own; *if every proposal is accepted* is the upper bound after review.\n\n",
        p.entities.len(), p.mentions.len(), p.items.len()
    ));
    md.push_str(SYS_HEAD);
    md.push_str(&sys_row(&r));
    md.push_str("\nBy language (applied automatically, pairwise P / R):\n\n| lang | P | R | pairs |\n|---|---:|---:|---:|\n");
    for (l, s) in &r.by_lang_auto {
        md.push_str(&format!("| {} | {} | {} | {} |\n", l, pct(s.p()), pct(s.r()), s.tp + s.fn_));
    }
    md.push_str("\nBy category (pairwise P / R; applied → accepted):\n\n| category | applied P / R | accepted P / R |\n|---|---|---|\n");
    for (c, s) in &r.by_category_auto {
        let a = &r.by_category_accepted[c];
        md.push_str(&format!("| {} | {} / {} | {} / {} |\n", c, pct(s.p()), pct(s.r()), pct(a.p()), pct(a.r())));
    }
    md.push('\n');
    md.push_str(&listing(&p, &p.items, &res));
    write(&out_dir(), "entity-resolution", &md, &json!({"rules": r}));
    eprintln!(
        "entity resolution (rules): applied P {} R {} | reviewed P {} R {} | accepted P {} R {} | proposals {} ({} correct)",
        pct(r.auto.p()), pct(r.auto.r()), pct(r.reviewed.p()), pct(r.reviewed.r()), pct(r.accepted.p()), pct(r.accepted.r()), r.proposals, r.proposals_correct
    );

    let current = (r4(r.auto.p()), r4(r.auto.r()), r4(r.accepted.p()), r4(r.accepted.r()), r4(ratio(r.proposals_correct, r.proposals)));
    let reviewed_recall = r4(r.reviewed.r());
    if update_baseline() {
        let mut b = load_baseline().unwrap_or(EntityBaseline {
            tolerance: 0.01,
            auto_precision: 0.0,
            auto_recall: 0.0,
            reviewed_recall: 0.0,
            accepted_precision: 0.0,
            accepted_recall: 0.0,
            proposal_precision: 0.0,
            known_entity: BTreeMap::new(),
        });
        (b.auto_precision, b.auto_recall, b.accepted_precision, b.accepted_recall, b.proposal_precision) = current;
        b.reviewed_recall = reviewed_recall;
        save_baseline(&b);
        return;
    }
    let b = load_baseline().expect("eval/entity-baseline.json");
    let t = b.tolerance;
    let mut fails = Vec::new();
    for (name, now, was) in [
        ("applied precision", current.0, b.auto_precision),
        ("applied recall", current.1, b.auto_recall),
        ("recall after review", reviewed_recall, b.reviewed_recall),
        ("accepted precision", current.2, b.accepted_precision),
        ("accepted recall", current.3, b.accepted_recall),
        ("proposal precision", current.4, b.proposal_precision),
    ] {
        if now < was - t {
            fails.push(format!("{}: {} < baseline {}", name, pct(now), pct(was)));
        }
    }
    assert!(fails.is_empty(), "entity resolution regressed against eval/entity-baseline.json:\n  {}", fails.join("\n  "));
}

// ── known-entity suggestions on the detection fixtures ───────────────────

#[derive(Debug, Clone, Default, Serialize)]
struct KnownResult {
    scenario: String,
    labels: usize,
    missed_before: usize,
    recovered: usize,
    suggestions: usize,
    false_suggestions: usize,
    recall_before: f64,
    recall_after: f64,
    precision_before: f64,
    precision_after: f64,
    covered_before: f64,
    covered_after: f64,
    recovered_items: Vec<String>,
    false_items: Vec<String>,
    by_rule: BTreeMap<String, (usize, usize)>,
}

fn to_known(category: &str, text: &str, approved: bool) -> Option<Known> {
    let c = llm_scanner::parse_category(category)?;
    Some(Known { category: c, text: text.to_string(), approved, weak: false, documents: approved as usize, group_label: None, group_id: None })
}

/// Run the suggestions for one scenario over every fixture.
fn known_scenario(name: &str, fixtures: &[Fixture], preds: &Predictions, kb: &dyn Fn(usize) -> Vec<Known>) -> KnownResult {
    let mut after: Predictions = preds.clone();
    let mut rules: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut flags_all: Vec<(usize, usize, Pred, String)> = Vec::new();
    for (fi, fx) in fixtures.iter().enumerate() {
        let known = kb(fi);
        for (pi, page) in fx.pages.iter().enumerate() {
            let existing: Vec<(usize, usize)> = preds[fi][pi].iter().map(|p| (p.start, p.end)).collect();
            for f in known::flag_page(&page.text, &known, &existing) {
                let pred = Pred { start: f.start, end: f.end, category: f.category.to_string(), text: f.text.clone(), layer: known::LAYER.into(), confidence: f.confidence };
                after[fi][pi].push(pred.clone());
                flags_all.push((fi, pi, pred, f.rule.as_str().to_string()));
            }
        }
    }
    let before = score(fixtures, preds);
    let now = score(fixtures, &after);
    let mut res = KnownResult { scenario: name.into(), labels: before.overall.overlap.labels, ..Default::default() };
    res.missed_before = before.overall.overlap.labels - before.overall.overlap.labels_found;
    res.recovered = now.overall.overlap.labels_found - before.overall.overlap.labels_found;
    res.suggestions = flags_all.len();
    for (fi, pi, p, rule) in &flags_all {
        let page = &fixtures[*fi].pages[*pi];
        let req = page.spans.iter().any(|s| !s.optional && s.start < p.end && p.start < s.end);
        let opt = page.spans.iter().any(|s| s.optional && s.start < p.end && p.start < s.end);
        let e = rules.entry(rule.clone()).or_default();
        e.0 += 1;
        if !req && !opt {
            res.false_suggestions += 1;
            e.1 += 1;
            res.false_items.push(format!("`{}` p{} {} ({}): {:?}", fixtures[*fi].id, pi + 1, p.category, rule, p.text));
        }
    }
    // Which labels came back.
    let missed: HashSet<(String, usize, String)> = before.false_negatives.iter().filter(|m| m.kind == "missed").map(|m| (m.fixture.clone(), m.page, m.text.clone())).collect();
    let still: HashSet<(String, usize, String)> = now.false_negatives.iter().filter(|m| m.kind == "missed").map(|m| (m.fixture.clone(), m.page, m.text.clone())).collect();
    for m in before.false_negatives.iter().filter(|m| m.kind == "missed") {
        let k = (m.fixture.clone(), m.page, m.text.clone());
        if missed.contains(&k) && !still.contains(&k) {
            res.recovered_items.push(format!("`{}` p{} {}: {:?}", m.fixture, m.page, m.category, m.text));
        }
    }
    res.recall_before = before.overall.overlap.recall();
    res.recall_after = now.overall.overlap.recall();
    res.precision_before = before.overall.overlap.precision();
    res.precision_after = now.overall.overlap.precision();
    res.covered_before = before.overall.covered_recall();
    res.covered_after = now.overall.covered_recall();
    res.by_rule = rules;
    res
}

/// The three knowledge-base scenarios for a set of predictions.
fn known_scenarios(fixtures: &[Fixture], preds: &Predictions) -> Vec<KnownResult> {
    let overlaps_label = |fi: usize, pi: usize, p: &Pred| fixtures[fi].pages[pi].spans.iter().any(|s| !s.optional && s.start < p.end && p.start < s.end);
    vec![
        // A: the document's own detections, before any review (what the app
        // does right after a scan).
        known_scenario("same document, unreviewed", fixtures, preds, &|fi| {
            preds[fi]
                .iter()
                .flatten()
                .filter_map(|p| to_known(&p.category, &p.text, false).map(|mut k| {
                    k.weak = known::weak_source(&p.layer, p.confidence);
                    k
                }))
                .collect()
        }),
        // B: the document's correct detections, approved (the reviewer
        // approved true positives and rejected the rest).
        known_scenario("same document, after review", fixtures, preds, &|fi| {
            preds[fi]
                .iter()
                .enumerate()
                .flat_map(|(pi, ps)| ps.iter().filter(move |p| overlaps_label(fi, pi, p)).map(|p| (p.category.clone(), p.text.clone())).collect::<Vec<_>>())
                .filter_map(|(c, t)| to_known(&c, &t, true))
                .collect()
        }),
        // C: every labelled entity of the document was approved in an earlier
        // document about the same parties (upper bound).
        known_scenario("approved in an earlier document (upper bound)", fixtures, preds, &|fi| {
            fixtures[fi].pages.iter().flat_map(|p| p.spans.iter().filter(|s| !s.optional)).filter_map(|s| to_known(&s.category, &s.text, true)).collect()
        }),
    ]
}

fn known_md(title: &str, rows: &[KnownResult]) -> String {
    let mut md = format!("## {}\n\n", title);
    md.push_str("| knowledge base | labels missed before | recovered | suggestions | false suggestions | overlap R before → after | covered R before → after | overlap P before → after |\n|---|---:|---:|---:|---:|---|---|---|\n");
    for r in rows {
        md.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} → {} | {} → {} | {} → {} |\n",
            r.scenario, r.missed_before, r.recovered, r.suggestions, r.false_suggestions,
            pct(r.recall_before), pct(r.recall_after), pct(r.covered_before), pct(r.covered_after),
            pct(r.precision_before), pct(r.precision_after)
        ));
    }
    for r in rows {
        md.push_str(&format!("\n### {}\n\nBy rule (suggestions / false): {}\n\nRecovered ({}):\n\n", r.scenario,
            r.by_rule.iter().map(|(k, (n, f))| format!("{} {}/{}", k, n, f)).collect::<Vec<_>>().join(", "), r.recovered_items.len()));
        for x in &r.recovered_items {
            md.push_str(&format!("- {}\n", x));
        }
        md.push_str(&format!("\nFalse suggestions ({}):\n\n", r.false_items.len()));
        for x in &r.false_items {
            md.push_str(&format!("- {}\n", x));
        }
    }
    md
}

#[test]
fn known_entity_suggestions_against_baseline() {
    let fixtures = load_fixtures();
    let preds = run_system(&fixtures, layers12);
    let rows = known_scenarios(&fixtures, &preds);
    let mut md = String::from("# Eval: known-entity suggestions on the detection fixtures\n\nGenerated by `cargo test --test eval_harness known_entity`. A *suggestion* is a pending `known-entity` detection; *recovered* = labels no layer overlapped that a suggestion overlaps; *false* = suggestions that overlap no label. Fixtures are independent documents, so scenarios A and B only use what the same document already contains; C assumes the document's entities were approved earlier (upper bound).\n\n");
    md.push_str(&known_md("On top of Layers 1–2", &rows));
    write(&out_dir(), "known-entity-l12", &md, &json!({"layers12": rows}));
    for r in &rows {
        eprintln!("known-entity [{}]: recovered {}/{} false {} of {}", r.scenario, r.recovered, r.missed_before, r.false_suggestions, r.suggestions);
    }
    let cur: BTreeMap<String, KnownBaseline> = rows
        .iter()
        .map(|r| (format!("layers12: {}", r.scenario), KnownBaseline { recovered: r.recovered, false_suggestions: r.false_suggestions }))
        .collect();
    if update_baseline() {
        if let Some(mut b) = load_baseline() {
            b.known_entity = cur;
            save_baseline(&b);
        }
        return;
    }
    let b = load_baseline().expect("eval/entity-baseline.json");
    let mut fails = Vec::new();
    for (k, was) in &b.known_entity {
        let now = cur.get(k).cloned().unwrap_or(KnownBaseline { recovered: 0, false_suggestions: usize::MAX });
        if now.recovered < was.recovered {
            fails.push(format!("{}: recovered {} < baseline {}", k, now.recovered, was.recovered));
        }
        if now.false_suggestions > was.false_suggestions + 1 {
            fails.push(format!("{}: false suggestions {} > baseline {}", k, now.false_suggestions, was.false_suggestions));
        }
    }
    assert!(fails.is_empty(), "known-entity suggestions regressed against eval/entity-baseline.json:\n  {}", fails.join("\n  "));
}

// ── opt-in: embeddings through the real sidecar ──────────────────────────

fn embed_enabled() -> bool {
    std::env::var("QVAULT_EVAL_EMBED").map(|v| v == "1").unwrap_or(false)
}

/// `QVAULT_EVAL_EMBED_MODELS`: catalog ids (looked up in QVAC_MODELS_DIR) or
/// `name=/abs/path.gguf` (tried as an embedding model outside the catalog).
fn embed_specs() -> Vec<ModelSpec> {
    let models_dir = PathBuf::from(std::env::var("QVAC_MODELS_DIR").expect("QVAC_MODELS_DIR"));
    let list = std::env::var("QVAULT_EVAL_EMBED_MODELS").unwrap_or_else(|_| app_lib::qvac::catalog::embedding_model().id.to_string());
    list.split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| match s.split_once('=') {
            Some((id, path)) => ModelSpec { id: id.to_string(), path: PathBuf::from(path), custom: true },
            None => {
                let m = app_lib::qvac::catalog::find(s).unwrap_or_else(|| panic!("{} is not a catalog id", s));
                ModelSpec { id: s.to_string(), path: models_dir.join(m.file_name), custom: false }
            }
        })
        .collect()
}

/// Embed through the real sidecar with the same protocol calls the app makes
/// (`loadModel` type `embeddings`, then `embed` in batches of 64). The
/// client is driven directly so candidates outside the catalog can be
/// compared too. Returns vectors, load seconds, embedding seconds and the
/// peak RSS of the sidecar.
fn embed_all(spec: &ModelSpec, root: &std::path::Path, texts: &[String]) -> Result<(Vec<Vec<f32>>, f64, f64, u64), String> {
    use app_lib::qvac::client::{CallOptions, ClientConfig, QvacClient, SpawnSpec};
    let paths = sidecar();
    let cache = root.join(format!("cache-{}", spec.id));
    std::fs::create_dir_all(&cache).unwrap();
    let mut env = Vec::new();
    for k in ["PATH", "HOME", "TMPDIR"] {
        if let Some(v) = std::env::var_os(k) {
            env.push((k.into(), v));
        }
    }
    let client = QvacClient::new(
        SpawnSpec { program: paths.program.clone(), args: vec![paths.worker.clone().into_os_string(), "--cache-dir".into(), cache.into_os_string()], env, clear_env: true },
        ClientConfig::default(),
    );
    let mem = MemSampler::start();
    let t = Instant::now();
    let to = std::time::Duration::from_secs(300);
    let qid = match client.call("loadModel", json!({"path": spec.path, "type": "embeddings", "config": {}}), CallOptions::timeout(to)) {
        Ok(r) => r["modelId"].as_str().unwrap_or_default().to_string(),
        Err(e) => {
            let _ = mem.finish();
            client.shutdown();
            return Err(e.to_string());
        }
    };
    // Warm-up (first call allocates the context).
    let _ = client.call("embed", json!({"modelId": qid, "text": [texts[0].clone()]}), CallOptions::timeout(to));
    let load_s = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let mut out: Vec<Vec<f32>> = Vec::new();
    for chunk in texts.chunks(64) {
        match client.call("embed", json!({"modelId": qid, "text": chunk}), CallOptions::timeout(to)) {
            Ok(r) => {
                let v: Vec<Vec<f32>> = serde_json::from_value(r["embedding"].clone()).map_err(|e| e.to_string())?;
                out.extend(v);
            }
            Err(e) => {
                let _ = mem.finish();
                client.shutdown();
                return Err(e.to_string());
            }
        }
    }
    let secs = t.elapsed().as_secs_f64();
    let peak = mem.finish();
    client.shutdown();
    if out.len() != texts.len() {
        return Err("wrong number of vectors".into());
    }
    Ok((out, load_s, secs, peak))
}

#[test]
fn entity_resolution_embeddings() {
    if !embed_enabled() {
        eprintln!("skipping: set QVAULT_EVAL_EMBED=1 (and QVAC_MODELS_DIR, QVAULT_EVAL_EMBED_MODELS) to run");
        return;
    }
    let p = prepare();
    let root = std::env::temp_dir().join(format!("qvault-eval-emb-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (base, _) = evaluate("rules only", &p, &p.items, &ResolveConfig::rules_only());
    let mut md = String::from("# Eval: entity resolution with embeddings\n\nGenerated by `QVAULT_EVAL_EMBED=1 cargo test --release --test eval_harness entity_resolution_embeddings`. Embeddings never merge on their own: they add proposals (cosine ≥ τ and whole-string Jaro-Winkler ≥ s) or withdraw rule proposals (cosine < veto). *Applied automatically* is therefore the same for every row.\n\n");
    let mut js = serde_json::Map::new();
    js.insert("rules".into(), serde_json::to_value(&base).unwrap());
    let mut summary = String::from("| model | load s | ms / string | peak RSS | setting | applied P / R | after review P / R | accept all P / R / F1 | proposals (precision) |\n|---|---:|---:|---:|---|---|---|---|---|\n");
    summary.push_str(&format!(
        "| none (rules + strings) | – | – | – | – | {} / {} | {} / {} | {} / {} / {} | {} ({}) |\n",
        pct(base.auto.p()), pct(base.auto.r()), pct(base.reviewed.p()), pct(base.reviewed.r()), pct(base.accepted.p()), pct(base.accepted.r()), pct(base.accepted.f1()), base.proposals, pct(ratio(base.proposals_correct, base.proposals))
    ));
    let mut details = String::new();
    for spec in embed_specs() {
        let texts: Vec<String> = p.items.iter().map(|it| embedding_input(&spec.id, &it.category, &it.text)).collect();
        let (vecs, load_s, secs, peak_kb) = match embed_all(&spec, &root, &texts) {
            Ok(x) => x,
            Err(e) => {
                eprintln!("[{}] failed: {}", spec.id, e);
                summary.push_str(&format!("| {} | failed to load/run on the pinned addon: {} | | | | | | |\n", spec.id, e));
                js.insert(spec.id.clone(), json!({"error": e}));
                continue;
            }
        };
        let ms_per = 1000.0 * secs / texts.len() as f64;
        eprintln!("[{}] load {:.1}s, {} strings in {:.2}s ({:.1} ms/string), dim {}, peak {} MB", spec.id, load_s, texts.len(), secs, ms_per, vecs[0].len(), peak_kb / 1024);
        let mut items = p.items.clone();
        for (it, v) in items.iter_mut().zip(vecs.iter()) {
            if app_lib::entities::forms::embeddable(&it.category) {
                it.embedding = Some(v.clone());
            }
        }
        // Cosine distribution: same entity vs different entity (same category).
        let mut same = Vec::new();
        let mut diff = Vec::new();
        for i in 0..p.mentions.len() {
            for j in i + 1..p.mentions.len() {
                let (a, b) = (&p.mentions[i], &p.mentions[j]);
                if a.category != b.category || a.item == b.item || !app_lib::entities::forms::embeddable(&a.category) {
                    continue;
                }
                let c = resolve::cosine(&vecs[a.item], &vecs[b.item]) as f64;
                if a.gold == b.gold { same.push(c) } else { diff.push(c) }
            }
        }
        let q = |v: &mut Vec<f64>, f: f64| {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v[((v.len() - 1) as f64 * f) as usize]
        };
        details.push_str(&format!(
            "\n## `{}`\n\nload {:.1} s · {} strings in {:.2} s ({:.1} ms/string, batches of 64) · dim {} · peak RSS {} MB\n\nCosine, same entity: p10 {:.3} · median {:.3} · p90 {:.3}; different entities: p10 {:.3} · median {:.3} · p90 {:.3} · max {:.3}\n\n",
            spec.id, load_s, texts.len(), secs, ms_per, vecs[0].len(), peak_kb / 1024,
            q(&mut same, 0.1), q(&mut same, 0.5), q(&mut same, 0.9), q(&mut diff, 0.1), q(&mut diff, 0.5), q(&mut diff, 0.9), q(&mut diff, 1.0)
        ));
        details.push_str(SYS_HEAD);
        details.push_str(&sys_row(&base));
        let mut best: Option<(f64, String, SystemResult, Resolution, ResolveConfig)> = None;
        let mut sweep = Vec::new();
        let mut grid: Vec<ResolveConfig> = Vec::new();
        for tau in [0.80f32, 0.85, 0.88, 0.90, 0.92, 0.95] {
            for smin in [0.0f32, 0.7, 0.8] {
                for veto in [None, Some(0.5f32), Some(0.6)] {
                    grid.push(ResolveConfig { embed_propose: Some(tau), embed_min_string: smin, embed_veto: veto, ..ResolveConfig::rules_only() });
                }
            }
        }
        for over in [0.85f32, 0.90, 0.95] {
            grid.push(ResolveConfig { embed_override_conflict: Some(over), ..ResolveConfig::default() });
        }
        for c in grid.clone() {
            grid.push(ResolveConfig { suppress_ambiguous: false, ..c });
        }
        {
            {
                for cfg in grid {
                    let (tau, smin, veto) = (cfg.embed_propose.unwrap_or(0.0), cfg.embed_min_string, cfg.embed_veto);
                    let name = format!(
                        "τ {:.2} · s {:.1} · veto {}{}{}",
                        tau, smin, veto.map(|v| format!("{:.1}", v)).unwrap_or("–".into()),
                        cfg.embed_override_conflict.map(|o| format!(" · override conflicts ≥ {:.2}", o)).unwrap_or_default(),
                        if cfg.suppress_ambiguous { "" } else { " · ambiguous kept" }
                    );
                    let (r, res) = evaluate(&name, &p, &items, &cfg);
                    details.push_str(&sys_row(&r));
                    sweep.push(json!({"tau": tau, "s": smin, "veto": veto, "result": r}));
                    // Pick on recall after review per proposal shown: recall
                    // minus a review cost of 0.1 pt per wrong proposal.
                    let key = r.reviewed.r() - 0.001 * (r.proposals - r.proposals_correct) as f64;
                    if best.as_ref().map(|b| key > b.0).unwrap_or(true) {
                        best = Some((key, name, r, res, cfg));
                    }
                }
            }
        }
        let def = ResolveConfig::default();
        let (dr, dres) = evaluate("default", &p, &items, &def);
        let (_, bname, br, bres, _) = best.unwrap();
        summary.push_str(&format!(
            "| {} | {:.1} | {:.1} | {} MB | default (τ {:.2}, s {:.1}, veto {}) | {} / {} | {} / {} | {} / {} / {} | {} ({}) |\n",
            spec.id, load_s, ms_per, peak_kb / 1024, def.embed_propose.unwrap_or(0.0), def.embed_min_string, def.embed_veto.map(|v| format!("{:.2}", v)).unwrap_or("–".into()),
            pct(dr.auto.p()), pct(dr.auto.r()), pct(dr.reviewed.p()), pct(dr.reviewed.r()), pct(dr.accepted.p()), pct(dr.accepted.r()), pct(dr.accepted.f1()), dr.proposals, pct(ratio(dr.proposals_correct, dr.proposals))
        ));
        summary.push_str(&format!(
            "| {} | | | | best: {} | {} / {} | {} / {} | {} / {} / {} | {} ({}) |\n",
            spec.id, bname, pct(br.auto.p()), pct(br.auto.r()), pct(br.reviewed.p()), pct(br.reviewed.r()), pct(br.accepted.p()), pct(br.accepted.r()), pct(br.accepted.f1()), br.proposals, pct(ratio(br.proposals_correct, br.proposals))
        ));
        details.push_str(&format!("\n### Default setting\n\n{}\n### Best setting ({})\n\n{}", listing(&p, &items, &dres), bname, listing(&p, &items, &bres)));
        js.insert(spec.id.clone(), json!({"load_s": load_s, "ms_per_string": ms_per, "peak_rss_kb": peak_kb, "dim": vecs[0].len(), "default": dr, "sweep": sweep}));
    }
    md.push_str(&summary);
    md.push_str(&details);
    write(&out_dir(), "entity-resolution-embeddings", &md, &Value::Object(js));
}

// ── opt-in: suggestions on top of Layers 1–3 ─────────────────────────────

#[test]
fn known_entity_suggestions_layer3() {
    if !std::env::var("QVAULT_EVAL_L3").map(|v| v == "1").unwrap_or(false) || std::env::var("QVAULT_EVAL_KNOWN").map(|v| v != "1").unwrap_or(true) {
        eprintln!("skipping: set QVAULT_EVAL_L3=1 QVAULT_EVAL_KNOWN=1 (and QVAC_MODELS_DIR, QVAULT_EVAL_MODELS) to run");
        return;
    }
    let fixtures = load_fixtures();
    let models_dir = PathBuf::from(std::env::var("QVAC_MODELS_DIR").expect("QVAC_MODELS_DIR"));
    let spec_str = std::env::var("QVAULT_EVAL_MODELS").unwrap_or_else(|_| app_lib::qvac::catalog::default_llm().id.to_string());
    let s = spec_str.split(',').next().unwrap().trim().to_string();
    let spec = match s.split_once('=') {
        Some((id, path)) => ModelSpec { id: id.into(), path: PathBuf::from(path), custom: true },
        None => ModelSpec { id: s.clone(), path: models_dir.join(app_lib::qvac::catalog::find(&s).expect("catalog id").file_name), custom: false },
    };
    let root = std::env::temp_dir().join(format!("qvault-eval-known-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let (dir, id) = install(&spec, &root);
    let svc = Arc::new(QvacService::new(&root.join("app"), Some(&dir), Some(sidecar())));
    std::fs::create_dir_all(root.join("app")).unwrap();
    svc.set_layer3(true, Some(id)).unwrap();
    let session = svc.prepare_layer3().expect("layer 3");
    let (preds, _, _, failed) = l3_preds(&fixtures, &session, &llm_scanner::PostConfig::default(), true);
    svc.shutdown();
    let rows12 = known_scenarios(&fixtures, &run_system(&fixtures, layers12));
    let rows123 = known_scenarios(&fixtures, &preds);
    let mut md = format!("# Eval: known-entity suggestions on top of Layers 1–3 (`{}`)\n\nFailed chunks: {}.\n\n", spec.id, failed);
    md.push_str(&known_md("On top of Layers 1–2", &rows12));
    md.push('\n');
    md.push_str(&known_md(&format!("On top of Layers 1–3 ({})", spec.id), &rows123));
    write(&out_dir(), &format!("known-entity-l123-{}", spec.id), &md, &json!({"layers12": rows12, "layers123": rows123}));
    for r in &rows123 {
        eprintln!("known-entity L1-3 [{}]: recovered {}/{} false {} of {}", r.scenario, r.recovered, r.missed_before, r.false_suggestions, r.suggestions);
    }
}
