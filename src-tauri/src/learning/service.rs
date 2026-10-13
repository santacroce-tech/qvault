//! Database side of learning from review decisions: recording decisions,
//! loading the [`Memory`] for a scan, storing marks and suppressions, the
//! "Not personal data" list (add / remove / export / import / candidates).
//!
//! Like the rest of the database, this is stored **unencrypted** in the app
//! data folder. Only counts and ids ever reach the audit log or the app log.

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

use super::{key, DecisionStats, ExampleCandidate, ImportPreview, ListFile, ListFileItem, Mark, Memory};
use crate::pipeline::types::PiiEntity;
use crate::storage::database::Database;

/// Rejections before a string is suggested for the list in Settings.
pub const CANDIDATE_MIN_REJECTIONS: usize = 3;

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn sql(e: rusqlite::Error) -> String {
    format!("database error: {}", e)
}

// ── decisions ────────────────────────────────────────────────────────────

/// Remember the decision on one redaction (`approved` / `rejected`); back
/// to `pending` forgets it. Idempotent: an occurrence counts once.
pub fn record_decision(db: &Database, redaction_id: &str, status: &str) -> Result<(), String> {
    db.with_conn(|c| record_decision_conn(c, redaction_id, status)).map_err(sql)
}

fn record_decision_conn(c: &Connection, redaction_id: &str, status: &str) -> rusqlite::Result<()> {
    if status != "approved" && status != "rejected" {
        c.execute("DELETE FROM review_decisions WHERE redaction_id = ?1", params![redaction_id])?;
        return Ok(());
    }
    let row: Option<(String, String, String, String)> = c
        .query_row(
            "SELECT document_id, category, original_text, detection_layer FROM redactions WHERE id = ?1",
            params![redaction_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;
    let Some((doc, cat, text, layer)) = row else { return Ok(()) };
    let k = key(&text);
    if k.is_empty() {
        return Ok(());
    }
    c.execute(
        "INSERT OR REPLACE INTO review_decisions (redaction_id, document_id, category, norm_text, display_text, layer, decision, decided_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![redaction_id, doc, cat, k, text.trim(), layer, status, now()],
    )?;
    Ok(())
}

/// Remember every decision in a document (after bulk actions).
pub fn record_document_decisions(db: &Database, document_id: &str) -> Result<(), String> {
    db.with_conn(|c| {
        let mut s = c.prepare("SELECT id, status FROM redactions WHERE document_id = ?1 AND status IN ('approved', 'rejected')")?;
        let rows: Vec<(String, String)> = s.query_map(params![document_id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        let tx = c.unchecked_transaction()?;
        for (id, st) in rows {
            record_decision_conn(&tx, &id, &st)?;
        }
        tx.commit()
    })
    .map_err(sql)
}

/// Backfill decisions from redactions reviewed before this feature existed
/// (schema migration to version 2). Returns how many were recorded.
pub fn backfill_decisions(c: &Connection) -> rusqlite::Result<usize> {
    let mut s = c.prepare(
        "SELECT r.id, r.status FROM redactions r LEFT JOIN review_decisions d ON d.redaction_id = r.id
         WHERE r.status IN ('approved', 'rejected') AND d.redaction_id IS NULL",
    )?;
    let rows: Vec<(String, String)> = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
    for (id, st) in &rows {
        record_decision_conn(c, id, st)?;
    }
    Ok(rows.len())
}

/// Forget all review decisions (Settings). The list is kept.
pub fn clear_decisions(db: &Database) -> Result<usize, String> {
    db.with_conn(|c| c.execute("DELETE FROM review_decisions", [])).map_err(sql)
}

/// Everything a scan needs: decision counts and the list.
pub fn load_memory(db: &Database) -> Result<Memory, String> {
    db.with_conn(|c| {
        let mut m = Memory::new();
        let mut s = c.prepare(
            "SELECT category, norm_text,
                    SUM(CASE WHEN decision = 'rejected' THEN 1 ELSE 0 END),
                    COUNT(DISTINCT CASE WHEN decision = 'rejected' THEN document_id END),
                    SUM(CASE WHEN decision = 'approved' THEN 1 ELSE 0 END)
             FROM review_decisions GROUP BY category, norm_text",
        )?;
        for row in s.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?, r.get::<_, i64>(4)?))
        })? {
            let (cat, k, rej, docs, app) = row?;
            m.set_stats(&cat, &k, DecisionStats { rejected: rej as usize, rejected_documents: docs as usize, approved: app as usize });
        }
        let mut s = c.prepare("SELECT id, category, text FROM not_personal_data")?;
        for row in s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))? {
            let (id, cat, text) = row?;
            m.add_list_item(&cat, &text, &id);
        }
        Ok(m)
    })
    .map_err(sql)
}

// ── scans ────────────────────────────────────────────────────────────────

/// Counters for one scan (audit log: counts only).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanLearning {
    pub marked: usize,
    pub suppressed: usize,
}

/// Store the marks of kept detections (call after their redactions are
/// inserted) and the suppressed detections of one page.
pub fn store_outcome(
    db: &Database,
    document_id: &str,
    page: u32,
    marks: &[Mark],
    suppressed: &[(PiiEntity, String)],
) -> Result<(), String> {
    if marks.is_empty() && suppressed.is_empty() {
        return Ok(());
    }
    db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        let ts = now();
        for m in marks {
            tx.execute(
                "INSERT OR REPLACE INTO redaction_memory (redaction_id, document_id, rejected, rejected_documents, approved, original_confidence, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![m.redaction_id, document_id, m.stats.rejected as i64, m.stats.rejected_documents as i64, m.stats.approved as i64, m.original_confidence, ts],
            )?;
        }
        for (e, item) in suppressed {
            tx.execute(
                "INSERT OR REPLACE INTO suppressed_detections (id, document_id, page, start_offset, end_offset, original_text, category, detection_layer, confidence, list_item_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![e.id, document_id, page as i64, e.start as i64, e.end as i64, e.text, e.category.to_string(), e.layer, e.confidence, item, ts],
            )?;
        }
        tx.commit()
    })
    .map_err(sql)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkRow {
    pub redaction_id: String,
    pub rejected: i64,
    pub rejected_documents: i64,
    pub original_confidence: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuppressedRow {
    pub id: String,
    pub page: i64,
    pub start_offset: i64,
    pub end_offset: i64,
    pub original_text: String,
    pub category: String,
    pub detection_layer: String,
    pub list_item_id: Option<String>,
    /// The list item still exists.
    pub listed: bool,
}

/// What the review sidebar shows about a document.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLearning {
    pub marks: Vec<MarkRow>,
    pub suppressed: Vec<SuppressedRow>,
}

pub fn document_learning(db: &Database, document_id: &str) -> Result<DocumentLearning, String> {
    db.with_conn(|c| {
        let mut s = c.prepare(
            "SELECT m.redaction_id, m.rejected, m.rejected_documents, m.original_confidence FROM redaction_memory m
             JOIN redactions r ON r.id = m.redaction_id WHERE m.document_id = ?1",
        )?;
        let marks = s
            .query_map(params![document_id], |r| {
                Ok(MarkRow { redaction_id: r.get(0)?, rejected: r.get(1)?, rejected_documents: r.get(2)?, original_confidence: r.get(3)? })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut s = c.prepare(
            "SELECT s.id, s.page, s.start_offset, s.end_offset, s.original_text, s.category, s.detection_layer, s.list_item_id, l.id IS NOT NULL
             FROM suppressed_detections s LEFT JOIN not_personal_data l ON l.id = s.list_item_id
             WHERE s.document_id = ?1 ORDER BY s.page, s.start_offset",
        )?;
        let suppressed = s
            .query_map(params![document_id], |r| {
                Ok(SuppressedRow {
                    id: r.get(0)?,
                    page: r.get(1)?,
                    start_offset: r.get(2)?,
                    end_offset: r.get(3)?,
                    original_text: r.get(4)?,
                    category: r.get(5)?,
                    detection_layer: r.get(6)?,
                    list_item_id: r.get(7)?,
                    listed: r.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(DocumentLearning { marks, suppressed })
    })
    .map_err(sql)
}

/// Reject every pending finding of a document that was rejected before (the
/// one-click action in the sidebar; a user action). Returns how many.
pub fn reject_previously_rejected(db: &Database, document_id: &str) -> Result<usize, String> {
    db.with_conn(|c| {
        let mut s = c.prepare(
            "SELECT r.id FROM redactions r JOIN redaction_memory m ON m.redaction_id = r.id
             WHERE r.document_id = ?1 AND r.status = 'pending'",
        )?;
        let ids: Vec<String> = s.query_map(params![document_id], |r| r.get(0))?.collect::<Result<_, _>>()?;
        let tx = c.unchecked_transaction()?;
        let ts = now();
        for id in &ids {
            tx.execute("UPDATE redactions SET status = 'rejected', reviewed_at = ?1 WHERE id = ?2", params![ts, id])?;
            record_decision_conn(&tx, id, "rejected")?;
        }
        tx.commit()?;
        Ok(ids.len())
    })
    .map_err(sql)
}

/// Put a suppressed detection back into the review list as a pending
/// finding (the user overrides the list for this occurrence). Returns the
/// new redaction id.
pub fn restore_suppressed(db: &Database, suppressed_id: &str) -> Result<String, String> {
    db.with_conn(|c| {
        let (doc, page, start, end, text, cat, layer, conf): (String, i64, i64, i64, String, String, String, f64) = c.query_row(
            "SELECT document_id, page, start_offset, end_offset, original_text, category, detection_layer, confidence
             FROM suppressed_detections WHERE id = ?1",
            params![suppressed_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
        )?;
        let replacement = crate::llm::llm_scanner::parse_category(&cat)
            .map(|c| c.replacement_label().to_string())
            .unwrap_or_else(|| "[REDACTED]".into());
        let rid = uuid::Uuid::new_v4().to_string();
        let ts = now();
        let tx = c.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO redactions (id, document_id, page, start_offset, end_offset, original_text, replacement, category, detection_layer, confidence, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending')",
            params![rid, doc, page, start, end, text, replacement, cat, layer, conf],
        )?;
        tx.execute(
            "INSERT INTO entities (id, document_id, original_text, normalized_text, category, page, start_offset, end_offset, confidence, detection_layer, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![uuid::Uuid::new_v4().to_string(), doc, text, text.trim(), cat, page, start, end, conf, layer, ts],
        )?;
        tx.execute("DELETE FROM suppressed_detections WHERE id = ?1", params![suppressed_id])?;
        tx.execute(
            "UPDATE documents SET pii_count = (SELECT COUNT(*) FROM redactions WHERE document_id = ?1) WHERE id = ?1",
            params![doc],
        )?;
        tx.commit()?;
        Ok(rid)
    })
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => "suppressed finding not found".into(),
        e => sql(e),
    })
}

// ── the "Not personal data" list ─────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListItem {
    pub id: String,
    pub category: String,
    pub text: String,
    pub note: Option<String>,
    /// review | settings | suggestion | import
    pub source: String,
    pub created_at: String,
    /// Detections suppressed by this item so far (documents still stored).
    pub suppressed: i64,
}

pub fn list_items(db: &Database) -> Result<Vec<ListItem>, String> {
    db.with_conn(|c| {
        let mut s = c.prepare(
            "SELECT l.id, l.category, l.text, l.note, l.source, l.created_at,
                    (SELECT COUNT(*) FROM suppressed_detections s WHERE s.list_item_id = l.id)
             FROM not_personal_data l ORDER BY l.text COLLATE NOCASE, l.category",
        )?;
        let rows = s
            .query_map([], |r| {
                Ok(ListItem {
                    id: r.get(0)?,
                    category: r.get(1)?,
                    text: r.get(2)?,
                    note: r.get(3)?,
                    source: r.get(4)?,
                    created_at: r.get(5)?,
                    suppressed: r.get(6)?,
                })
            })?
            .collect();
        rows
    })
    .map_err(sql)
}

fn existing_keys(c: &Connection) -> rusqlite::Result<HashSet<(String, String)>> {
    let mut s = c.prepare("SELECT category, norm_text FROM not_personal_data")?;
    let rows = s.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect();
    rows
}

fn insert_item(c: &Connection, item: &ListFileItem, source: &str) -> rusqlite::Result<Option<String>> {
    let id = uuid::Uuid::new_v4().to_string();
    let n = c.execute(
        "INSERT OR IGNORE INTO not_personal_data (id, category, text, norm_text, note, source, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, item.category, item.text, key(&item.text), item.note, source, now()],
    )?;
    Ok((n > 0).then_some(id))
}

/// Add one item (always an explicit user action). Returns the item id;
/// adding an item that is already listed returns the existing id.
pub fn add_item(db: &Database, category: &str, text: &str, note: Option<&str>, source: &str) -> Result<String, String> {
    let clean = super::validate_item(&ListFileItem { text: text.into(), category: category.into(), note: note.map(String::from) })?;
    let source = match source {
        "review" | "settings" | "suggestion" | "import" => source,
        _ => "settings",
    };
    db.with_conn(|c| {
        if let Some(id) = insert_item(c, &clean, source)? {
            return Ok(id);
        }
        c.query_row(
            "SELECT id FROM not_personal_data WHERE category = ?1 AND norm_text = ?2",
            params![clean.category, key(&clean.text)],
            |r| r.get(0),
        )
    })
    .map_err(sql)
}

pub fn remove_item(db: &Database, id: &str) -> Result<bool, String> {
    db.with_conn(|c| c.execute("DELETE FROM not_personal_data WHERE id = ?1", params![id])).map(|n| n > 0).map_err(sql)
}

/// The list as a JSON file (for sharing within a firm).
pub fn export_json(db: &Database) -> Result<String, String> {
    let items = list_items(db)?
        .into_iter()
        .map(|i| ListFileItem { text: i.text, category: i.category, note: i.note })
        .collect();
    serde_json::to_string_pretty(&ListFile::new(items)).map_err(|e| e.to_string())
}

/// Validate a list file against the current list without changing anything.
pub fn preview_import(db: &Database, json: &str) -> Result<ImportPreview, String> {
    let existing = db.with_conn(existing_keys).map_err(sql)?;
    super::preview_import(json, &existing)
}

/// Add the items the user confirmed from a preview. Items are validated
/// again; already listed ones are skipped. Returns how many were added.
pub fn apply_import(db: &Database, items: &[ListFileItem]) -> Result<usize, String> {
    if items.len() > super::LIST_MAX_ITEMS {
        return Err("too many items".into());
    }
    let clean: Vec<ListFileItem> = items.iter().map(super::validate_item).collect::<Result<_, _>>()?;
    db.with_conn(|c| {
        let tx = c.unchecked_transaction()?;
        let mut added = 0;
        for it in &clean {
            if insert_item(&tx, it, "import")?.is_some() {
                added += 1;
            }
        }
        tx.commit()?;
        Ok(added)
    })
    .map_err(sql)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub category: String,
    /// How the string was last written in a rejected finding.
    pub text: String,
    pub rejected: usize,
    pub rejected_documents: usize,
}

/// Strings rejected at least [`CANDIDATE_MIN_REJECTIONS`] times and never
/// approved, not on the list yet. Suggestions only: adding one is a click.
pub fn candidates(db: &Database) -> Result<Vec<Candidate>, String> {
    let m = load_memory(db)?;
    let list = m.candidates(CANDIDATE_MIN_REJECTIONS);
    db.with_conn(|c| {
        let mut out = Vec::new();
        for (cat, k, s) in list {
            let text: Option<String> = c
                .query_row(
                    "SELECT display_text FROM review_decisions WHERE category = ?1 AND norm_text = ?2 AND decision = 'rejected'
                     ORDER BY decided_at DESC LIMIT 1",
                    params![cat, k],
                    |r| r.get(0),
                )
                .optional()?;
            out.push(Candidate { category: cat, text: text.unwrap_or(k), rejected: s.rejected, rejected_documents: s.rejected_documents });
        }
        Ok(out)
    })
    .map_err(sql)
}

/// Unanimous decisions as few-shot candidates for Layer 3.
pub fn example_candidates(db: &Database) -> Result<Vec<ExampleCandidate>, String> {
    db.with_conn(|c| {
        let mut s = c.prepare(
            "SELECT category, MAX(display_text),
                    SUM(CASE WHEN decision = 'rejected' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN decision = 'approved' THEN 1 ELSE 0 END)
             FROM review_decisions GROUP BY category, norm_text",
        )?;
        let rows = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (cat, text, rej, app) = row?;
            if let Some(category) = crate::llm::llm_scanner::parse_category(&cat) {
                out.push(ExampleCandidate { text, category, rejected: rej as usize, approved: app as usize });
            }
        }
        Ok(out)
    })
    .map_err(sql)
}

/// Sizes for the Settings screen.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningSummary {
    pub decisions: i64,
    pub rejected_strings: i64,
    pub list_items: i64,
}

pub fn summary(db: &Database) -> Result<LearningSummary, String> {
    db.with_conn(|c| {
        Ok(LearningSummary {
            decisions: c.query_row("SELECT COUNT(*) FROM review_decisions", [], |r| r.get(0))?,
            rejected_strings: c.query_row(
                "SELECT COUNT(*) FROM (SELECT 1 FROM review_decisions GROUP BY category, norm_text
                 HAVING SUM(decision = 'rejected') > 0 AND SUM(decision = 'approved') = 0)",
                [],
                |r| r.get(0),
            )?,
            list_items: c.query_row("SELECT COUNT(*) FROM not_personal_data", [], |r| r.get(0))?,
        })
    })
    .map_err(sql)
}
