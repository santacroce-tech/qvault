//! Phase 4 storage: the schema migration (existing databases upgrade
//! cleanly), grouping in the database, user decisions surviving regroups,
//! known-entity suggestions with their notes, and deletion clean-up.

use std::path::PathBuf;

use app_lib::entities::resolve::ResolveConfig;
use app_lib::entities::{service, Embedder};
use app_lib::storage::database::Database;

fn tmp_db(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("qvault-entity-{}-{}", tag, uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&d).unwrap();
    d.join("qvault.db")
}

/// The schema every release before Phase 4 created (user_version 0).
const PRE_PHASE4_SCHEMA: &str = "
    CREATE TABLE documents (id TEXT PRIMARY KEY, filename TEXT NOT NULL, file_type TEXT NOT NULL, file_hash TEXT NOT NULL,
        page_count INTEGER, original_path TEXT NOT NULL, clean_path TEXT, status TEXT NOT NULL, pii_count INTEGER DEFAULT 0,
        created_at TEXT NOT NULL, completed_at TEXT);
    CREATE TABLE redactions (id TEXT PRIMARY KEY, document_id TEXT REFERENCES documents(id), page INTEGER NOT NULL,
        start_offset INTEGER NOT NULL, end_offset INTEGER NOT NULL, original_text TEXT NOT NULL, replacement TEXT NOT NULL,
        category TEXT NOT NULL, detection_layer TEXT NOT NULL, confidence REAL NOT NULL, status TEXT NOT NULL, reviewed_at TEXT);
    CREATE TABLE entities (id TEXT PRIMARY KEY, document_id TEXT REFERENCES documents(id), original_text TEXT NOT NULL,
        normalized_text TEXT NOT NULL, category TEXT NOT NULL, page INTEGER NOT NULL, start_offset INTEGER NOT NULL,
        end_offset INTEGER NOT NULL, confidence REAL NOT NULL, detection_layer TEXT NOT NULL, created_at TEXT NOT NULL);
    CREATE INDEX idx_entities_normalized ON entities(normalized_text);
    CREATE INDEX idx_entities_category ON entities(category);
    CREATE INDEX idx_entities_document ON entities(document_id);
    CREATE TABLE document_pages (id INTEGER PRIMARY KEY AUTOINCREMENT, document_id TEXT REFERENCES documents(id),
        page_number INTEGER NOT NULL, raw_text TEXT NOT NULL, cleaned_text TEXT, created_at TEXT NOT NULL,
        UNIQUE(document_id, page_number));
    CREATE TABLE ocr_pages (document_id TEXT NOT NULL, page INTEGER NOT NULL, status TEXT NOT NULL, reason TEXT, dpi INTEGER,
        layout TEXT, blocks INTEGER, duration_ms INTEGER, updated_at TEXT NOT NULL, PRIMARY KEY (document_id, page));
    CREATE TABLE audit_log (id INTEGER PRIMARY KEY AUTOINCREMENT, document_id TEXT, action TEXT NOT NULL, details TEXT,
        created_at TEXT NOT NULL);
";

/// Add a document with detections (all stored as redaction + entity, like a scan).
fn add_doc(db: &Database, id: &str, items: &[(&str, &str, &str)]) {
    db.insert_document(id, &format!("{}.pdf", id), "pdf", "h", 1, "/x").unwrap();
    let mut off = 0i32;
    for (k, (cat, text, status)) in items.iter().enumerate() {
        let rid = format!("{}-r{}", id, k);
        db.insert_redaction(&rid, id, 0, off, off + text.len() as i32, text, "[X]", cat, "regex", 0.9).unwrap();
        db.insert_entity(&format!("{}-e{}", id, k), id, text, text.trim(), cat, 0, off, off + text.len() as i32, 0.9, "regex").unwrap();
        if *status != "pending" {
            db.update_redaction_status(&rid, status, None).unwrap();
        }
        off += text.len() as i32 + 1;
    }
}

fn group_of(groups: &[service::GroupSummary], text: &str) -> String {
    groups.iter().find(|g| g.variants.iter().any(|v| v.text == text)).map(|g| g.id.clone()).unwrap_or_else(|| panic!("{} not grouped", text))
}

#[test]
fn existing_database_upgrades_cleanly() {
    let path = tmp_db("migrate");
    {
        let c = rusqlite::Connection::open(&path).unwrap();
        c.execute_batch(PRE_PHASE4_SCHEMA).unwrap();
        c.execute_batch(
            "INSERT INTO documents VALUES ('d1','a.pdf','pdf','h',1,'/x',NULL,'review',2,'2026-01-01T00:00:00Z',NULL);
             INSERT INTO redactions VALUES ('r1','d1',0,0,12,'Maria Santos','[N]','PersonName','heuristic',0.6,'approved',NULL);
             INSERT INTO redactions VALUES ('r2','d1',0,20,33,'Santos, Maria','[N]','PersonName','llm',0.8,'pending',NULL);
             INSERT INTO entities VALUES ('e1','d1','Maria Santos','Maria Santos','PersonName',0,0,12,0.6,'heuristic','2026-01-01T00:00:00Z');
             INSERT INTO entities VALUES ('e2','d1','Santos, Maria','Santos, Maria','PersonName',0,20,33,0.8,'llm','2026-01-01T00:00:00Z');
             INSERT INTO audit_log (document_id, action, details, created_at) VALUES ('d1','upload','x','2026-01-01T00:00:00Z');",
        )
        .unwrap();
        let v: i64 = c.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v, 0);
    }
    let db = Database::open(&path).unwrap();
    assert_eq!(db.schema_version().unwrap(), app_lib::storage::database::SCHEMA_VERSION);
    // Old data intact.
    assert_eq!(db.get_redactions("d1").unwrap().len(), 2);
    assert_eq!(db.get_entities_by_document("d1").unwrap().len(), 2);
    assert_eq!(db.get_entity_summary().unwrap().len(), 2);
    // New tables exist and are empty until the first regroup.
    let n: i64 = db.with_conn(|c| c.query_row("SELECT COUNT(*) FROM entity_variants", [], |r| r.get(0))).unwrap();
    assert_eq!(n, 0);
    for t in ["entity_groups", "entity_cannot_link", "entity_proposals", "redaction_notes"] {
        db.with_conn(|c| c.query_row(&format!("SELECT COUNT(*) FROM {}", t), [], |r| r.get::<_, i64>(0))).unwrap();
    }
    // Existing entities are grouped lazily.
    let rep = service::regroup(&db, None, None, &ResolveConfig::rules_only()).unwrap();
    assert_eq!(rep.variants, 2);
    assert_eq!(rep.groups, 1, "Maria Santos and Santos, Maria are one entity");
    drop(db);
    // Re-opening is a no-op.
    let db = Database::open(&path).unwrap();
    assert_eq!(db.schema_version().unwrap(), app_lib::storage::database::SCHEMA_VERSION);
    let groups = service::list_groups(&db).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].label, "Maria Santos");
    assert_eq!(groups[0].occurrences, 2);
    assert_eq!(groups[0].approved, 1);
}

#[test]
fn fresh_database_is_current() {
    let db = Database::open(&tmp_db("fresh")).unwrap();
    assert_eq!(db.schema_version().unwrap(), app_lib::storage::database::SCHEMA_VERSION);
}

#[test]
fn grouping_and_user_decisions_survive_regroups() {
    let db = Database::open(&tmp_db("decisions")).unwrap();
    add_doc(&db, "d1", &[("PersonName", "Maria Santos", "approved"), ("CompanyName", "Meridian Consulting LLC", "approved"), ("PersonName", "João Santos", "pending")]);
    add_doc(&db, "d2", &[("PersonName", "SANTOS, Maria", "pending"), ("PersonName", "M. Santos", "pending"), ("CompanyName", "Meridian Consulting", "pending")]);
    let cfg = ResolveConfig::rules_only();
    let rep = service::regroup(&db, None, None, &cfg).unwrap();
    assert_eq!(rep.variants, 6);
    let groups = service::list_groups(&db).unwrap();
    let maria = group_of(&groups, "Maria Santos");
    assert_eq!(maria, group_of(&groups, "SANTOS, Maria"));
    assert_eq!(group_of(&groups, "Meridian Consulting LLC"), group_of(&groups, "Meridian Consulting"));
    let m = group_of(&groups, "M. Santos");
    assert_ne!(m, maria, "an initial is proposed, not merged");
    assert_ne!(group_of(&groups, "João Santos"), maria);
    let g = groups.iter().find(|g| g.id == maria).unwrap();
    assert_eq!(g.documents, 2);
    let props = service::list_proposals(&db).unwrap();
    let p = props.iter().find(|p| (p.group_a == m || p.group_b == m) && (p.group_a == maria || p.group_b == maria)).expect("initial proposal");
    assert_eq!(p.rule, "initial");
    assert!(!props.iter().any(|p| p.category != "PersonName" && p.category != "CompanyName"));

    // Group ids are stable across regroups.
    service::regroup(&db, None, None, &cfg).unwrap();
    assert_eq!(group_of(&service::list_groups(&db).unwrap(), "Maria Santos"), maria);

    // Accept the proposal: merge.
    service::merge_groups(&db, &maria, &[m.clone()]).unwrap();
    service::regroup(&db, None, None, &cfg).unwrap();
    let groups = service::list_groups(&db).unwrap();
    assert_eq!(group_of(&groups, "M. Santos"), maria);
    assert!(service::list_proposals(&db).unwrap().iter().all(|p| p.group_a != m && p.group_b != m));

    // Split "SANTOS, Maria" out: it stays apart, and is not proposed again.
    let vid = groups.iter().find(|g| g.id == maria).unwrap().variants.iter().find(|v| v.text == "SANTOS, Maria").unwrap().id.clone();
    let new_group = service::split_variant(&db, &vid).unwrap();
    service::regroup(&db, None, None, &cfg).unwrap();
    let groups = service::list_groups(&db).unwrap();
    assert_eq!(group_of(&groups, "SANTOS, Maria"), new_group);
    assert_eq!(group_of(&groups, "Maria Santos"), maria);
    assert!(!service::list_proposals(&db).unwrap().iter().any(|p| p.group_a == new_group || p.group_b == new_group));

    // Merging across categories is refused.
    let company = group_of(&groups, "Meridian Consulting");
    assert!(service::merge_groups(&db, &maria, &[company]).is_err());

    // Dismiss a proposal: João vs the Maria group are not proposed (conflict),
    // so dismiss the company/person-free one we can create: a surname-only.
    add_doc(&db, "d3", &[("PersonName", "Ms. Santos", "pending")]);
    service::regroup(&db, None, None, &cfg).unwrap();
    let groups = service::list_groups(&db).unwrap();
    let ms = group_of(&groups, "Ms. Santos");
    let props = service::list_proposals(&db).unwrap();
    let p = props.iter().find(|p| p.group_a == ms || p.group_b == ms).expect("surname-only proposal");
    let other = if p.group_a == ms { p.group_b.clone() } else { p.group_a.clone() };
    service::dismiss_proposal(&db, &ms, &other).unwrap();
    service::regroup(&db, None, None, &cfg).unwrap();
    assert!(!service::list_proposals(&db)
        .unwrap()
        .iter()
        .any(|p| (p.group_a == ms && p.group_b == other) || (p.group_a == other && p.group_b == ms)));

    // Deleting documents prunes their variants and groups.
    for d in ["d1", "d2", "d3"] {
        db.delete_document(d).unwrap();
    }
    service::regroup(&db, None, None, &cfg).unwrap();
    assert!(service::list_groups(&db).unwrap().is_empty());
    let n: i64 = db.with_conn(|c| c.query_row("SELECT COUNT(*) FROM entity_variants", [], |r| r.get(0))).unwrap();
    assert_eq!(n, 0);
}

struct FakeEmbedder;

impl Embedder for FakeEmbedder {
    fn model_id(&self) -> String {
        "fake".into()
    }
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        // Letter histogram: similar strings get similar vectors.
        Ok(texts
            .iter()
            .map(|t| {
                let q = t.rsplit("Query: ").next().unwrap_or(t).to_lowercase();
                let mut v = vec![0f32; 26];
                for c in q.chars().filter(|c| c.is_ascii_lowercase()) {
                    v[(c as u8 - b'a') as usize] += 1.0;
                }
                let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
                v.into_iter().map(|x| x / n).collect()
            })
            .collect())
    }
}

struct BrokenEmbedder;

impl Embedder for BrokenEmbedder {
    fn model_id(&self) -> String {
        "broken".into()
    }
    fn embed(&self, _: &[String]) -> Result<Vec<Vec<f32>>, String> {
        Err("engine unavailable".into())
    }
}

#[test]
fn embeddings_are_stored_and_optional() {
    let db = Database::open(&tmp_db("embed")).unwrap();
    add_doc(&db, "d1", &[("CompanyName", "Grupo Albar", "approved"), ("CompanyName", "Albar Group", "pending"), ("Email", "a@example.com", "pending")]);
    let rep = service::regroup(&db, Some(&FakeEmbedder), Some("fake"), &ResolveConfig { embed_propose: Some(0.9), embed_min_string: 0.0, ..ResolveConfig::rules_only() }).unwrap();
    assert_eq!(rep.embedded, 2, "names/companies/addresses only");
    let props = service::list_proposals(&db).unwrap();
    assert_eq!(props.len(), 1);
    assert_eq!(props[0].rule, "embedding");
    assert!(props[0].cosine.unwrap() > 0.9);
    // Second run: nothing new to embed.
    let rep = service::regroup(&db, Some(&FakeEmbedder), Some("fake"), &ResolveConfig::default()).unwrap();
    assert_eq!(rep.embedded, 0);
    // Engine down: rules still group, the error is reported, nothing fails.
    let rep = service::regroup(&db, Some(&BrokenEmbedder), None, &ResolveConfig::default()).unwrap();
    assert!(rep.embedding_error.is_some());
    assert_eq!(rep.groups, 3);
    assert!(service::list_proposals(&db).unwrap().is_empty());
    assert!(!rep.audit().contains("example"), "audit details carry no entity text");
}

#[test]
fn known_entities_are_suggested_with_a_note() {
    let db = Database::open(&tmp_db("known")).unwrap();
    // An earlier, reviewed document.
    add_doc(&db, "old", &[("PersonName", "Maria Santos", "approved"), ("PersonName", "Effective Date", "rejected"), ("CompanyName", "Meridian Consulting LLC", "approved")]);
    service::regroup(&db, None, None, &ResolveConfig::rules_only()).unwrap();
    // A new document where the layers found only one thing.
    let page = "Between Meridian Consulting and Ms. Santos, effective as of the Effective Date. Signed: SANTOS, Maria. Contact Dana Holloway.";
    db.insert_document("new", "new.pdf", "pdf", "h", 1, "/x").unwrap();
    let s = page.find("Dana Holloway").unwrap();
    db.insert_redaction("new-r0", "new", 0, s as i32, (s + 13) as i32, "Dana Holloway", "[N]", "PersonName", "heuristic", 0.6).unwrap();
    db.insert_entity("new-e0", "new", "Dana Holloway", "Dana Holloway", "PersonName", 0, s as i32, (s + 13) as i32, 0.6, "heuristic").unwrap();
    let counts = service::flag_document(&db, "new", &[page.to_string()]).unwrap();
    let reds = db.get_redactions("new").unwrap();
    let flagged: Vec<(&str, &str)> = reds.iter().filter(|r| r.detection_layer == "known-entity").map(|r| (r.original_text.as_str(), r.status.as_str())).collect();
    assert_eq!(counts, vec![3], "{:?}", flagged);
    assert!(flagged.contains(&("Meridian Consulting", "pending")));
    assert!(flagged.contains(&("Santos", "pending")));
    assert!(flagged.contains(&("SANTOS, Maria", "pending")));
    assert!(!flagged.iter().any(|(t, _)| t.contains("Effective")), "rejected strings are not known entities");
    assert!(reds.iter().filter(|r| r.detection_layer == "known-entity").all(|r| r.confidence < 0.7 && r.status == "pending"));
    let notes = service::redaction_notes(&db, "new").unwrap();
    assert_eq!(notes.len(), 3);
    let n = notes.iter().find(|n| n.rule == "honorific-surname").unwrap();
    assert_eq!(n.source_text, "Maria Santos");
    assert!(n.source_approved);
    assert_eq!(n.documents, 1);
    assert!(n.group_label.is_some());
    // Suggestions are not used as sources for themselves; a second pass adds nothing.
    assert_eq!(service::flag_document(&db, "new", &[page.to_string()]).unwrap(), vec![0]);
    // Deleting the document removes its notes.
    db.delete_document("new").unwrap();
    assert!(service::redaction_notes(&db, "new").unwrap().is_empty());
}
