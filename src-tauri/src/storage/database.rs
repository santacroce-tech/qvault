use rusqlite::{Connection, Result, params};
use std::path::Path;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let db = Database {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        conn.execute_batch("
            CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                filename TEXT NOT NULL,
                file_type TEXT NOT NULL,
                file_hash TEXT NOT NULL,
                page_count INTEGER,
                original_path TEXT NOT NULL,
                clean_path TEXT,
                status TEXT NOT NULL,
                pii_count INTEGER DEFAULT 0,
                created_at TEXT NOT NULL,
                completed_at TEXT
            );

            CREATE TABLE IF NOT EXISTS redactions (
                id TEXT PRIMARY KEY,
                document_id TEXT REFERENCES documents(id),
                page INTEGER NOT NULL,
                start_offset INTEGER NOT NULL,
                end_offset INTEGER NOT NULL,
                original_text TEXT NOT NULL,
                replacement TEXT NOT NULL,
                category TEXT NOT NULL,
                detection_layer TEXT NOT NULL,
                confidence REAL NOT NULL,
                status TEXT NOT NULL,
                reviewed_at TEXT
            );

            -- Extracted entities: every PII occurrence found in a document,
            -- stored for cross-document analysis and occurrence history
            CREATE TABLE IF NOT EXISTS entities (
                id TEXT PRIMARY KEY,
                document_id TEXT REFERENCES documents(id),
                original_text TEXT NOT NULL,
                normalized_text TEXT NOT NULL,
                category TEXT NOT NULL,
                page INTEGER NOT NULL,
                start_offset INTEGER NOT NULL,
                end_offset INTEGER NOT NULL,
                confidence REAL NOT NULL,
                detection_layer TEXT NOT NULL,
                created_at TEXT NOT NULL
            );

            -- Create index for fast lookups by normalized text (cross-document search)
            CREATE INDEX IF NOT EXISTS idx_entities_normalized ON entities(normalized_text);
            CREATE INDEX IF NOT EXISTS idx_entities_category ON entities(category);
            CREATE INDEX IF NOT EXISTS idx_entities_document ON entities(document_id);

            -- Extracted page text: full text of each page for reconstruction
            CREATE TABLE IF NOT EXISTS document_pages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                document_id TEXT REFERENCES documents(id),
                page_number INTEGER NOT NULL,
                raw_text TEXT NOT NULL,
                cleaned_text TEXT,
                created_at TEXT NOT NULL,
                UNIQUE(document_id, page_number)
            );

            CREATE TABLE IF NOT EXISTS annotations (
                id TEXT PRIMARY KEY,
                document_id TEXT REFERENCES documents(id),
                page INTEGER NOT NULL,
                kind TEXT NOT NULL,
                x REAL NOT NULL,
                y REAL NOT NULL,
                width REAL NOT NULL,
                height REAL NOT NULL,
                content TEXT NOT NULL,
                font_size REAL,
                created_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_annotations_document ON annotations(document_id);

            CREATE TABLE IF NOT EXISTS audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                document_id TEXT,
                action TEXT NOT NULL,
                details TEXT,
                created_at TEXT NOT NULL
            );
        ")?;

        Ok(())
    }

    pub fn with_conn<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&Connection) -> Result<T>,
    {
        let conn = self.conn.lock().unwrap();
        f(&conn)
    }

    // ─── Documents ───

    pub fn insert_document(
        &self,
        id: &str,
        filename: &str,
        file_type: &str,
        file_hash: &str,
        page_count: i32,
        original_path: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO documents (id, filename, file_type, file_hash, page_count, original_path, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'processing', ?7)",
            params![id, filename, file_type, file_hash, page_count, original_path, now],
        )?;
        Ok(())
    }

    pub fn update_document_status(&self, id: &str, status: &str, pii_count: i32) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE documents SET status = ?1, pii_count = ?2 WHERE id = ?3",
            params![status, pii_count, id],
        )?;
        Ok(())
    }

    pub fn update_document_page_count(&self, id: &str, page_count: i32) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE documents SET page_count = ?1 WHERE id = ?2",
            params![page_count, id],
        )?;
        Ok(())
    }

    pub fn update_document_clean_path(&self, id: &str, clean_path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE documents SET clean_path = ?1, status = 'complete', completed_at = ?2 WHERE id = ?3",
            params![clean_path, now, id],
        )?;
        Ok(())
    }

    pub fn get_document(&self, id: &str) -> Result<Option<DocumentRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, filename, file_type, file_hash, page_count, original_path, clean_path, status, pii_count, created_at, completed_at
             FROM documents WHERE id = ?1"
        )?;

        let mut rows = stmt.query_map(params![id], |row| {
            Ok(DocumentRow {
                id: row.get(0)?,
                filename: row.get(1)?,
                file_type: row.get(2)?,
                file_hash: row.get(3)?,
                page_count: row.get(4)?,
                original_path: row.get(5)?,
                clean_path: row.get(6)?,
                status: row.get(7)?,
                pii_count: row.get(8)?,
                created_at: row.get(9)?,
                completed_at: row.get(10)?,
            })
        })?;

        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    // ─── Redactions ───

    pub fn insert_redaction(
        &self,
        id: &str,
        document_id: &str,
        page: i32,
        start_offset: i32,
        end_offset: i32,
        original_text: &str,
        replacement: &str,
        category: &str,
        detection_layer: &str,
        confidence: f64,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO redactions (id, document_id, page, start_offset, end_offset, original_text, replacement, category, detection_layer, confidence, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'pending')",
            params![id, document_id, page, start_offset, end_offset, original_text, replacement, category, detection_layer, confidence],
        )?;
        Ok(())
    }

    pub fn get_redactions(&self, document_id: &str) -> Result<Vec<RedactionRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, document_id, page, start_offset, end_offset, original_text, replacement, category, detection_layer, confidence, status, reviewed_at
             FROM redactions WHERE document_id = ?1 ORDER BY page, start_offset"
        )?;

        let rows = stmt.query_map(params![document_id], |row| {
            Ok(RedactionRow {
                id: row.get(0)?,
                document_id: row.get(1)?,
                page: row.get(2)?,
                start_offset: row.get(3)?,
                end_offset: row.get(4)?,
                original_text: row.get(5)?,
                replacement: row.get(6)?,
                category: row.get(7)?,
                detection_layer: row.get(8)?,
                confidence: row.get(9)?,
                status: row.get(10)?,
                reviewed_at: row.get(11)?,
            })
        })?;

        rows.collect()
    }

    pub fn update_redaction_status(&self, id: &str, status: &str, replacement: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        if let Some(rep) = replacement {
            conn.execute(
                "UPDATE redactions SET status = ?1, replacement = ?2, reviewed_at = ?3 WHERE id = ?4",
                params![status, rep, now, id],
            )?;
        } else {
            conn.execute(
                "UPDATE redactions SET status = ?1, reviewed_at = ?2 WHERE id = ?3",
                params![status, now, id],
            )?;
        }
        Ok(())
    }

    pub fn bulk_approve_redactions(&self, document_id: &str, category: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        if let Some(cat) = category {
            conn.execute(
                "UPDATE redactions SET status = 'approved', reviewed_at = ?1 WHERE document_id = ?2 AND category = ?3 AND status = 'pending'",
                params![now, document_id, cat],
            )?;
        } else {
            conn.execute(
                "UPDATE redactions SET status = 'approved', reviewed_at = ?1 WHERE document_id = ?2 AND status = 'pending'",
                params![now, document_id],
            )?;
        }
        Ok(())
    }

    // ─── Entities (cross-document PII knowledge base) ───

    pub fn insert_entity(
        &self,
        id: &str,
        document_id: &str,
        original_text: &str,
        normalized_text: &str,
        category: &str,
        page: i32,
        start_offset: i32,
        end_offset: i32,
        confidence: f64,
        detection_layer: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO entities (id, document_id, original_text, normalized_text, category, page, start_offset, end_offset, confidence, detection_layer, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![id, document_id, original_text, normalized_text, category, page, start_offset, end_offset, confidence, detection_layer, now],
        )?;
        Ok(())
    }

    pub fn get_entities_by_document(&self, document_id: &str) -> Result<Vec<EntityRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, document_id, original_text, normalized_text, category, page, start_offset, end_offset, confidence, detection_layer, created_at
             FROM entities WHERE document_id = ?1 ORDER BY page, start_offset"
        )?;

        let rows = stmt.query_map(params![document_id], |row| {
            Ok(EntityRow {
                id: row.get(0)?,
                document_id: row.get(1)?,
                original_text: row.get(2)?,
                normalized_text: row.get(3)?,
                category: row.get(4)?,
                page: row.get(5)?,
                start_offset: row.get(6)?,
                end_offset: row.get(7)?,
                confidence: row.get(8)?,
                detection_layer: row.get(9)?,
                created_at: row.get(10)?,
            })
        })?;

        rows.collect()
    }

    /// Find all documents containing a specific entity text (cross-document search)
    pub fn find_entity_occurrences(&self, normalized_text: &str) -> Result<Vec<EntityRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT e.id, e.document_id, e.original_text, e.normalized_text, e.category, e.page, e.start_offset, e.end_offset, e.confidence, e.detection_layer, e.created_at
             FROM entities e ORDER BY e.created_at DESC"
        )?;

        // Use normalized text for matching
        let pattern = normalized_text.to_lowercase();
        let rows = stmt.query_map([], |row| {
            Ok(EntityRow {
                id: row.get(0)?,
                document_id: row.get(1)?,
                original_text: row.get(2)?,
                normalized_text: row.get(3)?,
                category: row.get(4)?,
                page: row.get(5)?,
                start_offset: row.get(6)?,
                end_offset: row.get(7)?,
                confidence: row.get(8)?,
                detection_layer: row.get(9)?,
                created_at: row.get(10)?,
            })
        })?;

        let all: Vec<EntityRow> = rows.filter_map(|r| r.ok()).collect();
        Ok(all.into_iter().filter(|e| e.normalized_text.to_lowercase() == pattern).collect())
    }

    /// Get all unique entities across all documents with occurrence counts
    pub fn get_entity_summary(&self) -> Result<Vec<EntitySummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT normalized_text, category, COUNT(*) as occurrence_count, COUNT(DISTINCT document_id) as document_count
             FROM entities GROUP BY normalized_text, category ORDER BY occurrence_count DESC"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(EntitySummary {
                normalized_text: row.get(0)?,
                category: row.get(1)?,
                occurrence_count: row.get(2)?,
                document_count: row.get(3)?,
            })
        })?;

        rows.collect()
    }

    // ─── Document Pages (text storage for reconstruction) ───

    pub fn insert_document_page(
        &self,
        document_id: &str,
        page_number: i32,
        raw_text: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR REPLACE INTO document_pages (document_id, page_number, raw_text, created_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![document_id, page_number, raw_text, now],
        )?;
        Ok(())
    }

    pub fn update_page_cleaned_text(
        &self,
        document_id: &str,
        page_number: i32,
        cleaned_text: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE document_pages SET cleaned_text = ?1 WHERE document_id = ?2 AND page_number = ?3",
            params![cleaned_text, document_id, page_number],
        )?;
        Ok(())
    }

    pub fn get_document_pages(&self, document_id: &str) -> Result<Vec<DocumentPageRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT document_id, page_number, raw_text, cleaned_text, created_at
             FROM document_pages WHERE document_id = ?1 ORDER BY page_number"
        )?;

        let rows = stmt.query_map(params![document_id], |row| {
            Ok(DocumentPageRow {
                document_id: row.get(0)?,
                page_number: row.get(1)?,
                raw_text: row.get(2)?,
                cleaned_text: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        rows.collect()
    }

    // ─── History ───

    pub fn get_history(&self, offset: i64, limit: i64) -> Result<Vec<DocumentRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, filename, file_type, file_hash, page_count, original_path, clean_path, status, pii_count, created_at, completed_at
             FROM documents ORDER BY created_at DESC LIMIT ?1 OFFSET ?2"
        )?;

        let rows = stmt.query_map(params![limit, offset], |row| {
            Ok(DocumentRow {
                id: row.get(0)?,
                filename: row.get(1)?,
                file_type: row.get(2)?,
                file_hash: row.get(3)?,
                page_count: row.get(4)?,
                original_path: row.get(5)?,
                clean_path: row.get(6)?,
                status: row.get(7)?,
                pii_count: row.get(8)?,
                created_at: row.get(9)?,
                completed_at: row.get(10)?,
            })
        })?;

        rows.collect()
    }

    pub fn search_history(&self, query: &str) -> Result<Vec<DocumentRow>> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}%", query);
        let mut stmt = conn.prepare(
            "SELECT id, filename, file_type, file_hash, page_count, original_path, clean_path, status, pii_count, created_at, completed_at
             FROM documents WHERE filename LIKE ?1 ORDER BY created_at DESC LIMIT 50"
        )?;

        let rows = stmt.query_map(params![pattern], |row| {
            Ok(DocumentRow {
                id: row.get(0)?,
                filename: row.get(1)?,
                file_type: row.get(2)?,
                file_hash: row.get(3)?,
                page_count: row.get(4)?,
                original_path: row.get(5)?,
                clean_path: row.get(6)?,
                status: row.get(7)?,
                pii_count: row.get(8)?,
                created_at: row.get(9)?,
                completed_at: row.get(10)?,
            })
        })?;

        rows.collect()
    }

    pub fn delete_document(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM entities WHERE document_id = ?1", params![id])?;
        conn.execute("DELETE FROM document_pages WHERE document_id = ?1", params![id])?;
        conn.execute("DELETE FROM redactions WHERE document_id = ?1", params![id])?;
        conn.execute("DELETE FROM annotations WHERE document_id = ?1", params![id])?;
        conn.execute("DELETE FROM documents WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ─── Annotations ───

    pub fn insert_annotation(
        &self,
        id: &str,
        document_id: &str,
        page: i32,
        kind: &str,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        content: &str,
        font_size: Option<f64>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO annotations (id, document_id, page, kind, x, y, width, height, content, font_size, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![id, document_id, page, kind, x, y, width, height, content, font_size, now],
        )?;
        Ok(())
    }

    pub fn update_annotation(
        &self,
        id: &str,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        content: &str,
        font_size: Option<f64>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE annotations SET x = ?1, y = ?2, width = ?3, height = ?4, content = ?5, font_size = ?6 WHERE id = ?7",
            params![x, y, width, height, content, font_size, id],
        )?;
        Ok(())
    }

    pub fn delete_annotation(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM annotations WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_annotations(&self, document_id: &str) -> Result<Vec<AnnotationRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, document_id, page, kind, x, y, width, height, content, font_size, created_at
             FROM annotations WHERE document_id = ?1 ORDER BY page, created_at"
        )?;

        let rows = stmt.query_map(params![document_id], |row| {
            Ok(AnnotationRow {
                id: row.get(0)?,
                document_id: row.get(1)?,
                page: row.get(2)?,
                kind: row.get(3)?,
                x: row.get(4)?,
                y: row.get(5)?,
                width: row.get(6)?,
                height: row.get(7)?,
                content: row.get(8)?,
                font_size: row.get(9)?,
                created_at: row.get(10)?,
            })
        })?;

        rows.collect()
    }

    // ─── Audit Log ───

    pub fn insert_audit_log(&self, document_id: Option<&str>, action: &str, details: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO audit_log (document_id, action, details, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![document_id, action, details, now],
        )?;
        Ok(())
    }
}

// ─── Row types ───

#[derive(Debug, Clone, serde::Serialize)]
pub struct DocumentRow {
    pub id: String,
    pub filename: String,
    pub file_type: String,
    pub file_hash: String,
    pub page_count: i32,
    pub original_path: String,
    pub clean_path: Option<String>,
    pub status: String,
    pub pii_count: i32,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RedactionRow {
    pub id: String,
    pub document_id: String,
    pub page: i32,
    pub start_offset: i32,
    pub end_offset: i32,
    pub original_text: String,
    pub replacement: String,
    pub category: String,
    pub detection_layer: String,
    pub confidence: f64,
    pub status: String,
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EntityRow {
    pub id: String,
    pub document_id: String,
    pub original_text: String,
    pub normalized_text: String,
    pub category: String,
    pub page: i32,
    pub start_offset: i32,
    pub end_offset: i32,
    pub confidence: f64,
    pub detection_layer: String,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct EntitySummary {
    pub normalized_text: String,
    pub category: String,
    pub occurrence_count: i32,
    pub document_count: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AnnotationRow {
    pub id: String,
    pub document_id: String,
    pub page: i32,
    pub kind: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub content: String,
    pub font_size: Option<f64>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DocumentPageRow {
    pub document_id: String,
    pub page_number: i32,
    pub raw_text: String,
    pub cleaned_text: Option<String>,
    pub created_at: String,
}

