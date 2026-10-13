//! Database side of entity resolution: variants, groups, user decisions,
//! stored vectors, proposals, and known-entity suggestions with their
//! explanation. Metadata only ever reaches the audit log.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;
use std::time::Instant;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::forms;
use super::known::{self, Flag, Known};
use super::resolve::{self, Item, ResolveConfig};
use super::{embedding_input, Embedder};
use crate::pipeline::types::PiiCategory;
use crate::storage::database::Database;

/// One regroup at a time (scan completion, user actions, manual rebuild).
static REGROUP: Mutex<()> = Mutex::new(());

const EMBED_BATCH: usize = 64;

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn sql(e: rusqlite::Error) -> String {
    format!("database error: {}", e)
}

pub fn vec_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

pub fn blob_to_vec(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
}

// ── row types for the UI ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariantSummary {
    pub id: String,
    pub text: String,
    pub occurrences: i64,
    pub documents: i64,
    pub approved: i64,
    pub pinned: bool,
    pub has_embedding: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupSummary {
    pub id: String,
    pub category: String,
    pub label: String,
    pub occurrences: i64,
    pub documents: i64,
    pub approved: i64,
    pub variants: Vec<VariantSummary>,
    /// Pending proposals involving this group.
    pub proposals: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalSummary {
    pub group_a: String,
    pub group_b: String,
    pub label_a: String,
    pub label_b: String,
    pub variant_a: String,
    pub variant_b: String,
    pub category: String,
    pub rule: String,
    pub score: f64,
    pub cosine: Option<f64>,
    /// The short form also matches another, different person.
    pub ambiguous: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupDocument {
    pub document_id: String,
    pub filename: String,
    pub created_at: String,
    pub occurrences: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedactionNote {
    pub redaction_id: String,
    pub kind: String,
    pub rule: String,
    pub source_text: String,
    pub source_approved: bool,
    pub documents: i64,
    pub group_id: Option<String>,
    pub group_label: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegroupReport {
    pub variants: usize,
    pub groups: usize,
    /// Groups with more than one variant.
    pub merged_groups: usize,
    pub proposals: usize,
    pub embedded: usize,
    pub embedding_model: Option<String>,
    pub embedding_error: Option<String>,
    pub embed_ms: u64,
    pub duration_ms: u64,
}

impl RegroupReport {
    /// Audit-log details (counts and model id only).
    pub fn audit(&self) -> String {
        format!(
            "variants={} groups={} merged_groups={} proposals={} embedded={} embedding_model={} embedding_error={} embed_ms={} duration_ms={}",
            self.variants,
            self.groups,
            self.merged_groups,
            self.proposals,
            self.embedded,
            self.embedding_model.as_deref().unwrap_or("-"),
            self.embedding_error.as_deref().map(|_| "yes").unwrap_or("no"),
            self.embed_ms,
            self.duration_ms
        )
    }
}

// ── variants ─────────────────────────────────────────────────────────────

/// Add a variant row for every distinct (category, text) in `entities`, and
/// drop variants (and their decisions) whose occurrences are all gone.
fn sync_variants(conn: &Connection) -> rusqlite::Result<()> {
    let ts = now();
    let mut stmt = conn.prepare(
        "SELECT DISTINCT e.category, e.normalized_text FROM entities e
         LEFT JOIN entity_variants v ON v.category = e.category AND v.text = e.normalized_text
         WHERE v.id IS NULL AND length(trim(e.normalized_text)) > 0",
    )?;
    let missing: Vec<(String, String)> = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    for (c, t) in missing {
        conn.execute(
            "INSERT OR IGNORE INTO entity_variants (id, category, text, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![new_id(), c, t, ts],
        )?;
    }
    conn.execute_batch(
        "DELETE FROM entity_variants WHERE NOT EXISTS (
             SELECT 1 FROM entities e WHERE e.category = entity_variants.category AND e.normalized_text = entity_variants.text);
         DELETE FROM entity_cannot_link WHERE a NOT IN (SELECT id FROM entity_variants) OR b NOT IN (SELECT id FROM entity_variants);",
    )?;
    Ok(())
}

/// Compute and store vectors for variants that have none for `embedder`'s
/// model. Returns how many were embedded.
fn embed_missing(db: &Database, embedder: &dyn Embedder) -> Result<usize, String> {
    let model = embedder.model_id();
    let todo: Vec<(String, String, String)> = db
        .with_conn(|c| {
            let mut s = c.prepare(
                "SELECT id, category, text FROM entity_variants
                 WHERE (embedding IS NULL OR embedding_model IS NULL OR embedding_model != ?1)
                   AND category IN ('PersonName', 'CompanyName', 'Address')",
            )?;
            let rows = s.query_map(params![model], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<Vec<_>, _>>();
            rows
        })
        .map_err(sql)?;
    let mut done = 0usize;
    for chunk in todo.chunks(EMBED_BATCH) {
        let inputs: Vec<String> = chunk.iter().map(|(_, c, t)| embedding_input(&model, c, t)).collect();
        let vecs = embedder.embed(&inputs)?;
        if vecs.len() != chunk.len() {
            return Err("embedding model returned the wrong number of vectors".into());
        }
        db.with_conn(|c| {
            let tx = c.unchecked_transaction()?;
            for ((id, _, _), v) in chunk.iter().zip(vecs.iter()) {
                tx.execute(
                    "UPDATE entity_variants SET embedding = ?1, embedding_model = ?2 WHERE id = ?3",
                    params![vec_to_blob(v), model, id],
                )?;
            }
            tx.commit()
        })
        .map_err(sql)?;
        done += chunk.len();
    }
    Ok(done)
}

struct VariantRow {
    id: String,
    category: String,
    text: String,
    group_id: Option<String>,
    pinned: bool,
    embedding: Option<Vec<f32>>,
}

fn load_variants(conn: &Connection, vector_model: Option<&str>) -> rusqlite::Result<Vec<VariantRow>> {
    let mut s = conn.prepare("SELECT id, category, text, group_id, pinned, embedding, embedding_model FROM entity_variants ORDER BY category, text")?;
    let rows = s
        .query_map([], |r| {
            let blob: Option<Vec<u8>> = r.get(5)?;
            let m: Option<String> = r.get(6)?;
            let embedding = match (blob, m, vector_model) {
                (Some(b), Some(m), Some(want)) if m == want => Some(blob_to_vec(&b)),
                _ => None,
            };
            Ok(VariantRow {
                id: r.get(0)?,
                category: r.get(1)?,
                text: r.get(2)?,
                group_id: r.get(3)?,
                pinned: r.get::<_, i64>(4)? != 0,
                embedding,
            })
        })?
        .collect::<Result<Vec<_>, _>>();
    rows
}

fn occurrence_counts(conn: &Connection) -> rusqlite::Result<HashMap<(String, String), i64>> {
    let mut s = conn.prepare("SELECT category, normalized_text, COUNT(*) FROM entities GROUP BY category, normalized_text")?;
    let rows = s.query_map([], |r| Ok(((r.get::<_, String>(0)?, r.get::<_, String>(1)?), r.get::<_, i64>(2)?)))?;
    rows.collect()
}

// ── regroup ──────────────────────────────────────────────────────────────

/// Bring the groups up to date: new variants, vectors (when an embedder is
/// given; failures fall back to rules), grouping, proposals.
/// `vector_model`: use stored vectors of this model (even if `embedder` is
/// `None`, e.g. the sidecar is down). `None`: rules and string similarity only.
pub fn regroup(db: &Database, embedder: Option<&dyn Embedder>, vector_model: Option<&str>, cfg: &ResolveConfig) -> Result<RegroupReport, String> {
    let _one = REGROUP.lock().unwrap_or_else(|p| p.into_inner());
    let started = Instant::now();
    let mut rep = RegroupReport::default();
    db.with_conn(sync_variants).map_err(sql)?;
    if let Some(e) = embedder {
        let t = Instant::now();
        match embed_missing(db, e) {
            Ok(n) => rep.embedded = n,
            Err(err) => {
                log::warn!("entity grouping: embedding failed, using rules only for new variants: {}", err);
                rep.embedding_error = Some(err);
            }
        }
        rep.embed_ms = t.elapsed().as_millis() as u64;
    }
    rep.embedding_model = vector_model.map(String::from);
    let cfg = if vector_model.is_some() { *cfg } else { ResolveConfig::rules_only() };

    db.with_conn(|conn| {
        let rows = load_variants(conn, vector_model)?;
        let occ = occurrence_counts(conn)?;
        let index: HashMap<&str, usize> = rows.iter().enumerate().map(|(i, r)| (r.id.as_str(), i)).collect();
        let mut items: Vec<Item> = rows
            .iter()
            .map(|r| {
                let mut it = Item::new(&r.category, &r.text);
                it.embedding = r.embedding.clone();
                it.pinned = if r.pinned { r.group_id.clone() } else { None };
                it
            })
            .collect();
        // Pinned variants without a group (should not happen) are unpinned.
        for it in items.iter_mut() {
            if it.pinned.as_deref().map(|g| g.is_empty()).unwrap_or(false) {
                it.pinned = None;
            }
        }
        let mut cannot: HashSet<(usize, usize)> = HashSet::new();
        {
            let mut s = conn.prepare("SELECT a, b FROM entity_cannot_link")?;
            let pairs = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for p in pairs {
                let (a, b) = p?;
                if let (Some(&x), Some(&y)) = (index.get(a.as_str()), index.get(b.as_str())) {
                    cannot.insert((x.min(y), x.max(y)));
                }
            }
        }
        let res = resolve::resolve(&items, &cannot, &cfg);

        // Stable group ids: each cluster keeps the id most of its members
        // already had (largest clusters choose first).
        let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
        for (i, c) in res.cluster.iter().enumerate() {
            members.entry(*c).or_default().push(i);
        }
        let mut order: Vec<usize> = members.keys().copied().collect();
        order.sort_by_key(|c| std::cmp::Reverse(members[c].len()));
        let mut claimed: HashSet<String> = HashSet::new();
        let mut gid: HashMap<usize, String> = HashMap::new();
        for c in order {
            let mut votes: HashMap<&str, usize> = HashMap::new();
            for &i in &members[&c] {
                if let Some(g) = rows[i].group_id.as_deref() {
                    *votes.entry(g).or_default() += 1 + 10 * rows[i].pinned as usize;
                }
            }
            let mut cands: Vec<(&str, usize)> = votes.into_iter().collect();
            cands.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
            let id = cands.into_iter().map(|(g, _)| g.to_string()).find(|g| !claimed.contains(g)).unwrap_or_else(new_id);
            claimed.insert(id.clone());
            gid.insert(c, id);
        }

        let ts = now();
        let tx = conn.unchecked_transaction()?;
        for (c, ms) in &members {
            let id = &gid[c];
            let occ_of = |i: usize| *occ.get(&(rows[i].category.clone(), rows[i].text.clone())).unwrap_or(&0) as usize;
            let canon = resolve::canonical_index(ms, &items, &occ_of);
            tx.execute(
                "INSERT INTO entity_groups (id, category, label, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(id) DO UPDATE SET category = excluded.category, label = excluded.label, updated_at = excluded.updated_at",
                params![id, rows[canon].category, rows[canon].text, ts],
            )?;
            for &i in ms {
                if rows[i].group_id.as_deref() != Some(id.as_str()) {
                    tx.execute("UPDATE entity_variants SET group_id = ?1 WHERE id = ?2", params![id, rows[i].id])?;
                }
            }
        }
        tx.execute_batch("DELETE FROM entity_groups WHERE id NOT IN (SELECT DISTINCT group_id FROM entity_variants WHERE group_id IS NOT NULL); DELETE FROM entity_proposals;")?;
        for p in &res.proposals {
            let (ga, gb) = (&gid[&res.cluster[p.a]], &gid[&res.cluster[p.b]]);
            let (ga, gb, va, vb) = if ga <= gb { (ga, gb, &rows[p.a].id, &rows[p.b].id) } else { (gb, ga, &rows[p.b].id, &rows[p.a].id) };
            tx.execute(
                "INSERT OR REPLACE INTO entity_proposals (group_a, group_b, variant_a, variant_b, rule, score, cosine, ambiguous) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![ga, gb, va, vb, p.rule.as_str(), p.score as f64, p.cosine.map(|c| c as f64), p.ambiguous as i64],
            )?;
        }
        tx.commit()?;
        rep.variants = rows.len();
        rep.groups = members.len();
        rep.merged_groups = members.values().filter(|m| m.len() > 1).count();
        rep.proposals = res.proposals.len();
        items.clear();
        Ok(())
    })
    .map_err(sql)?;
    rep.duration_ms = started.elapsed().as_millis() as u64;
    Ok(rep)
}

// ── user decisions ───────────────────────────────────────────────────────

fn group_variant_ids(conn: &Connection, group: &str) -> rusqlite::Result<Vec<String>> {
    let mut s = conn.prepare("SELECT id FROM entity_variants WHERE group_id = ?1")?;
    let rows = s.query_map(params![group], |r| r.get(0))?.collect();
    rows
}

fn group_category(conn: &Connection, group: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row("SELECT category FROM entity_groups WHERE id = ?1", params![group], |r| r.get(0)).optional()
}

/// Merge `others` into `target` (user decision; all members are pinned so
/// later regroups keep them together). Groups of different categories are
/// never merged.
pub fn merge_groups(db: &Database, target: &str, others: &[String]) -> Result<usize, String> {
    let _one = REGROUP.lock().unwrap_or_else(|p| p.into_inner());
    db.with_conn(|conn| {
        let cat = group_category(conn, target)?.ok_or(rusqlite::Error::QueryReturnedNoRows)?;
        for o in others {
            if group_category(conn, o)?.as_deref() != Some(cat.as_str()) {
                return Err(rusqlite::Error::InvalidParameterName("groups of different categories cannot be merged".into()));
            }
        }
        let tx = conn.unchecked_transaction()?;
        let mut moved = 0usize;
        for o in others.iter().filter(|o| o.as_str() != target) {
            moved += tx.execute("UPDATE entity_variants SET group_id = ?1 WHERE group_id = ?2", params![target, o])?;
            tx.execute("DELETE FROM entity_groups WHERE id = ?1", params![o])?;
        }
        tx.execute("UPDATE entity_variants SET pinned = 1 WHERE group_id = ?1", params![target])?;
        // Earlier "keep apart" decisions inside the merged group no longer apply.
        tx.execute(
            "DELETE FROM entity_cannot_link WHERE a IN (SELECT id FROM entity_variants WHERE group_id = ?1)
               AND b IN (SELECT id FROM entity_variants WHERE group_id = ?1)",
            params![target],
        )?;
        tx.commit()?;
        Ok(moved)
    })
    .map_err(|e| match e {
        rusqlite::Error::InvalidParameterName(m) => m,
        rusqlite::Error::QueryReturnedNoRows => "group not found".into(),
        e => sql(e),
    })
}

/// Take one variant out of its group (user decision). It gets a group of
/// its own and is kept apart from the variants it was grouped with.
pub fn split_variant(db: &Database, variant: &str) -> Result<String, String> {
    let _one = REGROUP.lock().unwrap_or_else(|p| p.into_inner());
    db.with_conn(|conn| {
        let (cat, text, group): (String, String, Option<String>) = conn.query_row(
            "SELECT category, text, group_id FROM entity_variants WHERE id = ?1",
            params![variant],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let ts = now();
        let new_group = new_id();
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO entity_groups (id, category, label, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
            params![new_group, cat, text, ts],
        )?;
        if let Some(g) = group {
            let others = group_variant_ids(&tx, &g)?;
            for o in others.iter().filter(|o| o.as_str() != variant) {
                let (a, b) = if o.as_str() < variant { (o.as_str(), variant) } else { (variant, o.as_str()) };
                tx.execute("INSERT OR IGNORE INTO entity_cannot_link (a, b, created_at) VALUES (?1, ?2, ?3)", params![a, b, ts])?;
            }
            tx.execute("UPDATE entity_variants SET pinned = 1 WHERE group_id = ?1", params![g])?;
        }
        tx.execute("UPDATE entity_variants SET group_id = ?1, pinned = 1 WHERE id = ?2", params![new_group, variant])?;
        tx.commit()?;
        Ok(new_group)
    })
    .map_err(sql)
}

/// "These are different": keep the two groups apart and drop the proposal.
pub fn dismiss_proposal(db: &Database, group_a: &str, group_b: &str) -> Result<(), String> {
    let _one = REGROUP.lock().unwrap_or_else(|p| p.into_inner());
    db.with_conn(|conn| {
        let a = group_variant_ids(conn, group_a)?;
        let b = group_variant_ids(conn, group_b)?;
        let ts = now();
        let tx = conn.unchecked_transaction()?;
        for x in &a {
            for y in &b {
                let (p, q) = if x < y { (x, y) } else { (y, x) };
                tx.execute("INSERT OR IGNORE INTO entity_cannot_link (a, b, created_at) VALUES (?1, ?2, ?3)", params![p, q, ts])?;
            }
        }
        let (ga, gb) = if group_a <= group_b { (group_a, group_b) } else { (group_b, group_a) };
        tx.execute("DELETE FROM entity_proposals WHERE group_a = ?1 AND group_b = ?2", params![ga, gb])?;
        tx.commit()
    })
    .map_err(sql)
}

// ── queries for the UI ───────────────────────────────────────────────────

/// Join of an entity occurrence with its review status.
const OCC_JOIN: &str = "FROM entities e
    LEFT JOIN redactions r ON r.document_id = e.document_id AND r.page = e.page
         AND r.start_offset = e.start_offset AND r.end_offset = e.end_offset AND r.category = e.category";

pub fn list_groups(db: &Database) -> Result<Vec<GroupSummary>, String> {
    db.with_conn(|conn| {
        // Occurrence statistics per variant string.
        let mut s = conn.prepare(&format!(
            "SELECT e.category, e.normalized_text, COUNT(*), COUNT(DISTINCT e.document_id),
                    SUM(CASE WHEN r.status = 'approved' THEN 1 ELSE 0 END)
             {} GROUP BY e.category, e.normalized_text",
            OCC_JOIN
        ))?;
        let mut stats: HashMap<(String, String), (i64, i64, i64)> = HashMap::new();
        for row in s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, Option<i64>>(4)?)))? {
            let (c, t, n, d, a) = row?;
            stats.insert((c, t), (n, d, a.unwrap_or(0)));
        }
        // Distinct documents per group (not the sum over variants).
        let mut s = conn.prepare(
            "SELECT v.group_id, COUNT(DISTINCT e.document_id) FROM entity_variants v
             JOIN entities e ON e.category = v.category AND e.normalized_text = v.text
             WHERE v.group_id IS NOT NULL GROUP BY v.group_id",
        )?;
        let group_docs: HashMap<String, i64> = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        let mut s = conn.prepare("SELECT group_a, group_b FROM entity_proposals")?;
        let mut prop_count: HashMap<String, i64> = HashMap::new();
        for row in s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (a, b) = row?;
            *prop_count.entry(a).or_default() += 1;
            *prop_count.entry(b).or_default() += 1;
        }
        let mut s = conn.prepare(
            "SELECT g.id, g.category, g.label, v.id, v.text, v.pinned, v.embedding IS NOT NULL
             FROM entity_groups g JOIN entity_variants v ON v.group_id = g.id ORDER BY g.id, v.text",
        )?;
        let mut groups: Vec<GroupSummary> = Vec::new();
        for row in s.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)? != 0,
                r.get::<_, bool>(6)?,
            ))
        })? {
            let (gid, cat, label, vid, text, pinned, has_emb) = row?;
            let (n, d, a) = stats.get(&(cat.clone(), text.clone())).copied().unwrap_or((0, 0, 0));
            if groups.last().map(|g| g.id != gid).unwrap_or(true) {
                groups.push(GroupSummary {
                    id: gid.clone(),
                    category: cat.clone(),
                    label,
                    occurrences: 0,
                    documents: *group_docs.get(&gid).unwrap_or(&0),
                    approved: 0,
                    variants: vec![],
                    proposals: *prop_count.get(&gid).unwrap_or(&0),
                });
            }
            let g = groups.last_mut().unwrap();
            g.occurrences += n;
            g.approved += a;
            g.variants.push(VariantSummary { id: vid, text, occurrences: n, documents: d, approved: a, pinned, has_embedding: has_emb });
        }
        groups.retain(|g| g.occurrences > 0);
        groups.sort_by(|x, y| y.occurrences.cmp(&x.occurrences).then(x.label.cmp(&y.label)));
        Ok(groups)
    })
    .map_err(sql)
}

pub fn list_proposals(db: &Database) -> Result<Vec<ProposalSummary>, String> {
    db.with_conn(|conn| {
        let mut s = conn.prepare(
            "SELECT p.group_a, p.group_b, ga.label, gb.label, va.text, vb.text, ga.category, p.rule, p.score, p.cosine, p.ambiguous
             FROM entity_proposals p
             JOIN entity_groups ga ON ga.id = p.group_a JOIN entity_groups gb ON gb.id = p.group_b
             JOIN entity_variants va ON va.id = p.variant_a JOIN entity_variants vb ON vb.id = p.variant_b
             WHERE ga.category = gb.category
             ORDER BY p.score DESC",
        )?;
        let rows = s
            .query_map([], |r| {
                Ok(ProposalSummary {
                    group_a: r.get(0)?,
                    group_b: r.get(1)?,
                    label_a: r.get(2)?,
                    label_b: r.get(3)?,
                    variant_a: r.get(4)?,
                    variant_b: r.get(5)?,
                    category: r.get(6)?,
                    rule: r.get(7)?,
                    score: r.get(8)?,
                    cosine: r.get(9)?,
                    ambiguous: r.get::<_, i64>(10)? != 0,
                })
            })?
            .collect::<Result<Vec<_>, _>>();
        let mut rows = rows?;
        rows.sort_by_key(|p| (p.ambiguous, forms::Rule::parse(&p.rule).strength()));
        Ok(rows)
    })
    .map_err(sql)
}

/// Documents that mention any variant of a group.
pub fn group_documents(db: &Database, group: &str) -> Result<Vec<GroupDocument>, String> {
    db.with_conn(|conn| {
        let mut s = conn.prepare(
            "SELECT d.id, d.filename, d.created_at, COUNT(*) FROM entity_variants v
             JOIN entities e ON e.category = v.category AND e.normalized_text = v.text
             JOIN documents d ON d.id = e.document_id
             WHERE v.group_id = ?1 GROUP BY d.id ORDER BY d.created_at DESC",
        )?;
        let rows = s
            .query_map(params![group], |r| Ok(GroupDocument { document_id: r.get(0)?, filename: r.get(1)?, created_at: r.get(2)?, occurrences: r.get(3)? }))?
            .collect();
        rows
    })
    .map_err(sql)
}

// ── known-entity suggestions ─────────────────────────────────────────────

/// What the knowledge base knows for scanning `document_id`: strings the
/// user approved in any document, plus strings detected (not yet reviewed)
/// in this document. A string the user rejected and never approved is not
/// used.
pub fn known_entities(db: &Database, document_id: &str) -> Result<Vec<Known>, String> {
    db.with_conn(|conn| {
        let q = format!(
            "SELECT e.category, e.normalized_text,
                    SUM(CASE WHEN r.status = 'approved' THEN 1 ELSE 0 END) AS approved,
                    SUM(CASE WHEN r.status = 'rejected' THEN 1 ELSE 0 END) AS rejected,
                    SUM(CASE WHEN r.status = 'pending' AND e.document_id = ?1 AND e.detection_layer != '{}' THEN 1 ELSE 0 END) AS here,
                    SUM(CASE WHEN r.status = 'pending' AND e.document_id = ?1 AND e.detection_layer != '{}'
                             AND NOT (e.detection_layer = 'heuristic' AND e.confidence < 0.85) THEN 1 ELSE 0 END) AS here_strong,
                    COUNT(DISTINCT CASE WHEN r.status = 'approved' THEN e.document_id END) AS docs,
                    MAX(g.id), MAX(g.label)
             {}
             LEFT JOIN entity_variants v ON v.category = e.category AND v.text = e.normalized_text
             LEFT JOIN entity_groups g ON g.id = v.group_id
             GROUP BY e.category, e.normalized_text
             HAVING (approved > 0 OR here > 0) AND NOT (rejected > 0 AND approved = 0)",
            known::LAYER,
            known::LAYER,
            OCC_JOIN
        );
        let mut s = conn.prepare(&q)?;
        let rows = s.query_map(params![document_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(6)?,
                r.get::<_, Option<String>>(7)?,
                r.get::<_, Option<String>>(8)?,
                r.get::<_, i64>(5)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (cat, text, approved, docs, gid, glabel, strong) = row?;
            let Some(category) = crate::llm::llm_scanner::parse_category(&cat) else { continue };
            out.push(Known { category, text, approved: approved > 0, weak: approved == 0 && strong == 0, documents: docs as usize, group_label: glabel, group_id: gid });
        }
        Ok(out)
    })
    .map_err(sql)
}

/// Suggest known entities on every page of a scanned document. Stores each
/// suggestion as a pending redaction (layer `known-entity`), an entity
/// occurrence, and a note explaining it. Returns the number per page.
pub fn flag_document(db: &Database, document_id: &str, pages: &[String]) -> Result<Vec<usize>, String> {
    flag_document_with(db, document_id, pages, None).map(|(c, _)| c)
}

/// [`flag_document`] that also applies the review memory (§13): a
/// suggestion on the "Not personal data" list is suppressed (stored and
/// counted), one rejected before is marked. Returns the number stored per
/// page and the learning counters.
pub fn flag_document_with(
    db: &Database,
    document_id: &str,
    pages: &[String],
    memory: Option<&crate::learning::Memory>,
) -> Result<(Vec<usize>, crate::learning::service::ScanLearning), String> {
    let mut learned = crate::learning::service::ScanLearning::default();
    let known = known_entities(db, document_id)?;
    let existing = db.get_redactions(document_id).map_err(sql)?;
    let mut counts = vec![0usize; pages.len()];
    if known.is_empty() {
        return Ok((counts, learned));
    }
    // Ranges the list suppressed: not suggested again by this layer.
    let suppressed_ranges: Vec<(i64, i64, i64)> = db
        .with_conn(|c| {
            let mut s = c.prepare("SELECT page, start_offset, end_offset FROM suppressed_detections WHERE document_id = ?1")?;
            let rows = s.query_map(params![document_id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect();
            rows
        })
        .map_err(sql)?;
    for (p, text) in pages.iter().enumerate() {
        let mut ranges: Vec<(usize, usize)> = existing
            .iter()
            .filter(|r| r.page == p as i32)
            .map(|r| (r.start_offset.max(0) as usize, r.end_offset.max(0) as usize))
            .collect();
        ranges.extend(suppressed_ranges.iter().filter(|r| r.0 == p as i64).map(|r| (r.1.max(0) as usize, r.2.max(0) as usize)));
        let flags = known::flag_page(text, &known, &ranges);
        let entities: Vec<crate::pipeline::types::PiiEntity> = flags.iter().map(|f| f.to_entity(p as u32)).collect();
        let applied = match memory {
            Some(m) => m.apply(entities),
            None => crate::learning::Applied { kept: entities, ..Default::default() },
        };
        for e in &applied.kept {
            if let Some(f) = flags.iter().find(|f| f.start == e.start && f.end == e.end) {
                store_flag(db, document_id, e, f)?;
            }
        }
        crate::learning::service::store_outcome(db, document_id, p as u32, &applied.marks, &applied.suppressed)?;
        learned.marked += applied.marks.len();
        learned.suppressed += applied.suppressed.len();
        counts[p] = applied.kept.len();
    }
    Ok((counts, learned))
}

fn store_flag(db: &Database, document_id: &str, e: &crate::pipeline::types::PiiEntity, f: &Flag) -> Result<(), String> {
    let page = e.page;
    let cat = e.category.to_string();
    db.insert_redaction(&e.id, document_id, page as i32, e.start as i32, e.end as i32, &e.text, &e.replacement, &cat, &e.layer, e.confidence)
        .map_err(sql)?;
    db.insert_entity(&new_id(), document_id, &e.text, e.text.trim(), &cat, page as i32, e.start as i32, e.end as i32, e.confidence, &e.layer)
        .map_err(sql)?;
    db.with_conn(|c| {
        c.execute(
            "INSERT OR REPLACE INTO redaction_notes (redaction_id, document_id, kind, rule, source_text, source_approved, documents, group_id, group_label, created_at)
             VALUES (?1, ?2, 'known-entity', ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![e.id, document_id, f.rule.as_str(), f.source_text, f.source_approved as i64, f.documents as i64, f.group_id, f.group_label, now()],
        )
        .map(|_| ())
    })
    .map_err(sql)
}

pub fn redaction_notes(db: &Database, document_id: &str) -> Result<Vec<RedactionNote>, String> {
    db.with_conn(|conn| {
        let mut s = conn.prepare(
            "SELECT redaction_id, kind, rule, source_text, source_approved, documents, group_id, group_label
             FROM redaction_notes WHERE document_id = ?1",
        )?;
        let rows = s
            .query_map(params![document_id], |r| {
                Ok(RedactionNote {
                    redaction_id: r.get(0)?,
                    kind: r.get(1)?,
                    rule: r.get(2)?,
                    source_text: r.get(3)?,
                    source_approved: r.get::<_, i64>(4)? != 0,
                    documents: r.get(5)?,
                    group_id: r.get(6)?,
                    group_label: r.get(7)?,
                })
            })?
            .collect();
        rows
    })
    .map_err(sql)
}

/// Category of a stored string (for callers holding only text).
pub fn category_of(name: &str) -> Option<PiiCategory> {
    crate::llm::llm_scanner::parse_category(name)
}
