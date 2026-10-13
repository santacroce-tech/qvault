//! Learning from review decisions (§13), database side: the migration from
//! schema version 1 (with decisions backfilled), recording and forgetting
//! decisions, marks and suppressions stored by a scan, the one-click
//! rejection, restoring a suppressed finding, and the "Not personal data"
//! list (add, remove, export, import preview, candidates).

use std::path::PathBuf;

use app_lib::entities::service as entity_service;
use app_lib::learning::{self, service, ListFileItem};
use app_lib::pipeline::types::{PiiCategory, PiiEntity};
use app_lib::storage::database::{Database, SCHEMA_VERSION};

fn tmp_db(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("qvault-learning-{}-{}", tag, uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&d).unwrap();
    d.join("qvault.db")
}

/// A document whose findings are stored like a scan stores them; `status`
/// other than pending is set through the same path as the review sidebar.
fn add_doc(db: &Database, id: &str, items: &[(&str, &str, &str)]) -> Vec<String> {
    db.insert_document(id, &format!("{}.pdf", id), "pdf", "h", 1, "/x").unwrap();
    let mut off = 0i32;
    let mut ids = vec![];
    for (k, (cat, text, status)) in items.iter().enumerate() {
        let rid = format!("{}-r{}", id, k);
        db.insert_redaction(&rid, id, 0, off, off + text.len() as i32, text, "[X]", cat, "heuristic", 0.6).unwrap();
        db.insert_entity(&format!("{}-e{}", id, k), id, text, text.trim(), cat, 0, off, off + text.len() as i32, 0.6, "heuristic").unwrap();
        if *status != "pending" {
            db.update_redaction_status(&rid, status, None).unwrap();
            service::record_decision(db, &rid, status).unwrap();
        }
        off += text.len() as i32 + 1;
        ids.push(rid);
    }
    ids
}

fn ent(text: &str, cat: PiiCategory, start: usize) -> PiiEntity {
    PiiEntity {
        id: uuid::Uuid::new_v4().to_string(),
        text: text.into(),
        start,
        end: start + text.len(),
        page: 0,
        replacement: cat.replacement_label().into(),
        category: cat,
        layer: "heuristic".into(),
        confidence: 0.6,
    }
}

#[test]
fn version_1_database_upgrades_and_backfills_decisions() {
    let path = tmp_db("migrate");
    {
        // A database exactly as Phase 4 (schema version 1) left it.
        let db = Database::open(&path).unwrap();
        db.with_conn(|c| {
            c.execute_batch(
                "DROP TABLE review_decisions; DROP TABLE not_personal_data; DROP TABLE redaction_memory;
                 DROP TABLE suppressed_detections; PRAGMA user_version = 1;",
            )
        })
        .unwrap();
        db.with_conn(|c| {
            c.execute_batch(
                "INSERT INTO documents VALUES ('d1','a.pdf','pdf','h',1,'/x',NULL,'review',3,'2026-01-01T00:00:00Z',NULL);
                 INSERT INTO redactions VALUES ('r1','d1',0,0,14,'Effective Date','[N]','PersonName','heuristic',0.6,'rejected','2026-01-01T00:00:00Z');
                 INSERT INTO redactions VALUES ('r2','d1',0,20,32,'Maria Santos','[N]','PersonName','llm',0.8,'approved','2026-01-01T00:00:00Z');
                 INSERT INTO redactions VALUES ('r3','d1',0,40,48,'New York','[N]','CompanyName','heuristic',0.6,'pending',NULL);",
            )
        })
        .unwrap();
    }
    let db = Database::open(&path).unwrap();
    assert_eq!(db.schema_version().unwrap(), SCHEMA_VERSION);
    assert_eq!(SCHEMA_VERSION, 2);
    assert_eq!(db.get_redactions("d1").unwrap().len(), 3, "old data intact");
    let m = service::load_memory(&db).unwrap();
    let s = m.stats("PersonName", "effective date").expect("rejection backfilled");
    assert_eq!((s.rejected, s.rejected_documents, s.approved), (1, 1, 0));
    assert_eq!(m.stats("PersonName", "Maria Santos").unwrap().approved, 1);
    assert!(m.stats("CompanyName", "New York").is_none(), "pending findings are not decisions");
    assert_eq!(m.list_len(), 0);
    drop(db);
    // Re-opening is a no-op.
    let db = Database::open(&path).unwrap();
    let n: i64 = db.with_conn(|c| c.query_row("SELECT COUNT(*) FROM review_decisions", [], |r| r.get(0))).unwrap();
    assert_eq!(n, 2);
    // A fresh database starts at the current version, empty.
    let fresh = Database::open(&tmp_db("fresh")).unwrap();
    assert_eq!(fresh.schema_version().unwrap(), SCHEMA_VERSION);
    assert!(service::load_memory(&fresh).unwrap().is_empty());
}

#[test]
fn decisions_are_counted_once_per_occurrence_and_follow_the_latest_click() {
    let db = Database::open(&tmp_db("decisions")).unwrap();
    let a = add_doc(&db, "a", &[("PersonName", "Effective Date", "rejected"), ("PersonName", "EFFECTIVE DATE", "rejected")]);
    add_doc(&db, "b", &[("PersonName", "Effective Date", "rejected"), ("PersonName", "Dana Holloway", "approved")]);
    // Clicking reject again does not count twice.
    service::record_decision(&db, &a[0], "rejected").unwrap();
    let s = service::load_memory(&db).unwrap().stats("PersonName", "Effective Date").cloned().unwrap();
    assert_eq!((s.rejected, s.rejected_documents, s.approved), (3, 2, 0));
    // Changing one to approved: the string is no longer "rejected before".
    db.update_redaction_status(&a[1], "approved", None).unwrap();
    service::record_decision(&db, &a[1], "approved").unwrap();
    let s = service::load_memory(&db).unwrap().stats("PersonName", "Effective Date").cloned().unwrap();
    assert_eq!((s.rejected, s.approved), (2, 1));
    assert!(!s.rejected_before());
    // Back to pending forgets the decision.
    service::record_decision(&db, &a[1], "pending").unwrap();
    assert!(service::load_memory(&db).unwrap().stats("PersonName", "Effective Date").unwrap().rejected_before());
    // Bulk approval is remembered too.
    add_doc(&db, "c", &[("CompanyName", "Atlas Digital LLC", "pending")]);
    db.bulk_approve_redactions("c", None).unwrap();
    service::record_document_decisions(&db, "c").unwrap();
    assert_eq!(service::load_memory(&db).unwrap().stats("CompanyName", "Atlas Digital LLC").unwrap().approved, 1);
    // Deleting a document drops its approvals, keeps its rejections.
    db.delete_document("b").unwrap();
    let m = service::load_memory(&db).unwrap();
    assert!(m.stats("PersonName", "Dana Holloway").is_none());
    assert_eq!(m.stats("PersonName", "Effective Date").unwrap().rejected, 2);
    assert_eq!(service::clear_decisions(&db).unwrap(), 3);
    assert!(service::load_memory(&db).unwrap().is_empty());
}

#[test]
fn scan_outcome_marks_suppresses_and_can_be_undone_by_the_user() {
    let db = Database::open(&tmp_db("scan")).unwrap();
    add_doc(&db, "old", &[("PersonName", "Managing Partner", "rejected"), ("PersonName", "Effective Date", "rejected")]);
    let item = service::add_item(&db, "PersonName", "Effective Date", None, "review").unwrap();
    assert_eq!(service::add_item(&db, "PersonName", "  effective   date ", None, "settings").unwrap(), item, "same key, same item");

    // What a scan of a new document does with one page.
    db.insert_document("new", "new.pdf", "pdf", "h", 1, "/x").unwrap();
    let mem = service::load_memory(&db).unwrap();
    let applied = mem.apply(vec![
        ent("Managing Partner", PiiCategory::PersonName, 0),
        ent("Effective Date", PiiCategory::PersonName, 20),
        ent("Dana Holloway", PiiCategory::PersonName, 40),
    ]);
    for e in &applied.kept {
        db.insert_redaction(&e.id, "new", 0, e.start as i32, e.end as i32, &e.text, &e.replacement, &e.category.to_string(), &e.layer, e.confidence).unwrap();
    }
    service::store_outcome(&db, "new", 0, &applied.marks, &applied.suppressed).unwrap();
    let r = db.get_redactions("new").unwrap();
    assert_eq!(r.len(), 2, "the listed string is suppressed, the rejected one still shown");
    let marked = r.iter().find(|x| x.original_text == "Managing Partner").unwrap();
    assert_eq!(marked.status, "pending", "nothing is decided automatically");
    assert_eq!(marked.confidence, 0.3);
    let view = service::document_learning(&db, "new").unwrap();
    assert_eq!(view.marks.len(), 1);
    assert_eq!((view.marks[0].rejected, view.marks[0].rejected_documents), (1, 1));
    assert_eq!(view.suppressed.len(), 1);
    assert!(view.suppressed[0].listed);
    assert_eq!(service::list_items(&db).unwrap()[0].suppressed, 1);

    // One click rejects the marked finding (and remembers it).
    assert_eq!(service::reject_previously_rejected(&db, "new").unwrap(), 1);
    assert_eq!(db.get_redactions("new").unwrap().iter().filter(|x| x.status == "rejected").count(), 1);
    assert_eq!(service::load_memory(&db).unwrap().stats("PersonName", "Managing Partner").unwrap().rejected_documents, 2);
    assert_eq!(service::reject_previously_rejected(&db, "new").unwrap(), 0);

    // Restoring the suppressed finding puts it back, pending.
    let rid = service::restore_suppressed(&db, &view.suppressed[0].id).unwrap();
    let back = db.get_redactions("new").unwrap().into_iter().find(|x| x.id == rid).unwrap();
    assert_eq!((back.status.as_str(), back.original_text.as_str()), ("pending", "Effective Date"));
    assert!(service::document_learning(&db, "new").unwrap().suppressed.is_empty());
    assert!(service::restore_suppressed(&db, &view.suppressed[0].id).is_err());

    // Removing the list item: later scans no longer suppress it.
    assert!(service::remove_item(&db, &item).unwrap());
    assert_eq!(service::load_memory(&db).unwrap().list_len(), 0);
    db.delete_document("new").unwrap();
    let left: i64 = db
        .with_conn(|c| c.query_row("SELECT (SELECT COUNT(*) FROM redaction_memory) + (SELECT COUNT(*) FROM suppressed_detections)", [], |r| r.get(0)))
        .unwrap();
    assert_eq!(left, 0);
}

#[test]
fn known_entity_suggestions_respect_the_list() {
    let db = Database::open(&tmp_db("known")).unwrap();
    // "Juniper Hollow" was detected elsewhere in the document (a weak Layer 2
    // guess), so the known-entity layer would suggest its other occurrence.
    db.insert_document("d", "d.pdf", "pdf", "h", 1, "/x").unwrap();
    let page = "Juniper Hollow said hi. Later, Juniper Hollow left.";
    db.insert_redaction("r0", "d", 0, 0, 14, "Juniper Hollow", "[N]", "PersonName", "heuristic", 0.9).unwrap();
    db.insert_entity("e0", "d", "Juniper Hollow", "Juniper Hollow", "PersonName", 0, 0, 14, 0.9, "heuristic").unwrap();
    let mut mem = learning::Memory::new();
    let (counts, _) = entity_service::flag_document_with(&db, "d", &[page.to_string()], Some(&mem)).unwrap();
    assert_eq!(counts, vec![1], "baseline: the second occurrence is suggested");
    // Same document again, with the string on the list: suppressed, counted.
    db.delete_document("d").unwrap();
    db.insert_document("d", "d.pdf", "pdf", "h", 1, "/x").unwrap();
    db.insert_redaction("r0", "d", 0, 0, 14, "Juniper Hollow", "[N]", "PersonName", "heuristic", 0.9).unwrap();
    db.insert_entity("e0", "d", "Juniper Hollow", "Juniper Hollow", "PersonName", 0, 0, 14, 0.9, "heuristic").unwrap();
    mem.add_list_item("PersonName", "juniper hollow", "item");
    let (counts, learned) = entity_service::flag_document_with(&db, "d", &[page.to_string()], Some(&mem)).unwrap();
    assert_eq!(counts, vec![0]);
    assert_eq!(learned.suppressed, 1);
    assert_eq!(service::document_learning(&db, "d").unwrap().suppressed.len(), 1);
}

#[test]
fn list_export_import_and_candidates() {
    let db = Database::open(&tmp_db("list")).unwrap();
    for d in ["a", "b", "c"] {
        add_doc(&db, d, &[("PersonName", "Judgment Debtor", "rejected"), ("CompanyName", "New York", "rejected")]);
    }
    add_doc(&db, "d", &[("CompanyName", "New York", "approved")]);
    let c = service::candidates(&db).unwrap();
    assert_eq!(c.len(), 1, "approved once elsewhere: not a candidate");
    assert_eq!((c[0].text.as_str(), c[0].rejected, c[0].rejected_documents), ("Judgment Debtor", 3, 3));
    // Candidates are only suggestions: nothing was added.
    assert_eq!(service::list_items(&db).unwrap().len(), 0);
    service::add_item(&db, &c[0].category, &c[0].text, None, "suggestion").unwrap();
    assert!(service::candidates(&db).unwrap().is_empty());
    assert!(service::add_item(&db, "Nope", "x", None, "settings").is_err());
    assert!(service::add_item(&db, "PersonName", "   ", None, "settings").is_err());

    let json = service::export_json(&db).unwrap();
    let file: learning::ListFile = serde_json::from_str(&json).unwrap();
    assert_eq!(file.format, learning::LIST_FORMAT);
    assert_eq!(file.items, vec![ListFileItem { text: "Judgment Debtor".into(), category: "PersonName".into(), note: None }]);

    // Importing into another installation: preview first, then apply.
    let other = Database::open(&tmp_db("list2")).unwrap();
    service::add_item(&other, "PersonName", "Effective Date", Some("defined term"), "settings").unwrap();
    let mut f2 = file.clone();
    f2.items.push(ListFileItem { text: "effective  date".into(), category: "PersonName".into(), note: None });
    f2.items.push(ListFileItem { text: "x".repeat(300), category: "PersonName".into(), note: None });
    let p = service::preview_import(&other, &serde_json::to_string(&f2).unwrap()).unwrap();
    assert_eq!((p.new_items.len(), p.already_listed, p.invalid.len()), (1, 1, 1));
    assert_eq!(service::list_items(&other).unwrap().len(), 1, "a preview changes nothing");
    assert_eq!(service::apply_import(&other, &p.new_items).unwrap(), 1);
    assert_eq!(service::apply_import(&other, &p.new_items).unwrap(), 0, "already listed");
    let items = service::list_items(&other).unwrap();
    assert_eq!(items.iter().map(|i| (i.text.as_str(), i.source.as_str())).collect::<Vec<_>>(), vec![("Effective Date", "settings"), ("Judgment Debtor", "import")]);
    assert!(service::apply_import(&other, &[ListFileItem { text: "a\u{7}".into(), category: "PersonName".into(), note: None }]).is_err());
    let s = service::summary(&db).unwrap();
    assert_eq!((s.decisions, s.rejected_strings, s.list_items), (7, 1, 1));
}
