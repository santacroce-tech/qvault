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

            CREATE TABLE IF NOT EXISTS credits (
                id TEXT PRIMARY KEY,
                license_key TEXT,
                balance INTEGER NOT NULL DEFAULT 3,
                free_remaining INTEGER NOT NULL DEFAULT 3,
                free_month TEXT NOT NULL,
                last_synced_at TEXT
            );

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
        conn.execute("DELETE FROM redactions WHERE document_id = ?1", params![id])?;
        conn.execute("DELETE FROM documents WHERE id = ?1", params![id])?;
        Ok(())
    }

    // Credit management
    pub fn get_or_init_credits(&self) -> Result<CreditRow> {
        let conn = self.conn.lock().unwrap();
        let current_month = chrono::Utc::now().format("%Y-%m").to_string();

        let mut stmt = conn.prepare("SELECT id, license_key, balance, free_remaining, free_month, last_synced_at FROM credits LIMIT 1")?;
        let mut rows = stmt.query_map([], |row| {
            Ok(CreditRow {
                id: row.get(0)?,
                license_key: row.get(1)?,
                balance: row.get(2)?,
                free_remaining: row.get(3)?,
                free_month: row.get(4)?,
                last_synced_at: row.get(5)?,
            })
        })?;

        match rows.next() {
            Some(row) => {
                let mut credit = row?;
                // Reset free credits if month changed
                if credit.free_month != current_month {
                    credit.free_remaining = 3;
                    credit.free_month = current_month.clone();
                    drop(rows);
                    conn.execute(
                        "UPDATE credits SET free_remaining = 3, free_month = ?1 WHERE id = ?2",
                        params![current_month, credit.id],
                    )?;
                }
                Ok(credit)
            }
            None => {
                drop(rows);
                let id = uuid::Uuid::new_v4().to_string();
                conn.execute(
                    "INSERT INTO credits (id, balance, free_remaining, free_month) VALUES (?1, 0, 3, ?2)",
                    params![id, current_month],
                )?;
                Ok(CreditRow {
                    id,
                    license_key: None,
                    balance: 0,
                    free_remaining: 3,
                    free_month: current_month,
                    last_synced_at: None,
                })
            }
        }
    }

    pub fn decrement_credit(&self) -> Result<bool> {
        let credits = self.get_or_init_credits()?;
        let conn = self.conn.lock().unwrap();

        if credits.free_remaining > 0 {
            conn.execute(
                "UPDATE credits SET free_remaining = free_remaining - 1 WHERE id = ?1",
                params![credits.id],
            )?;
            Ok(true)
        } else if credits.balance > 0 {
            conn.execute(
                "UPDATE credits SET balance = balance - 1 WHERE id = ?1",
                params![credits.id],
            )?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

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
pub struct CreditRow {
    pub id: String,
    pub license_key: Option<String>,
    pub balance: i32,
    pub free_remaining: i32,
    pub free_month: String,
    pub last_synced_at: Option<String>,
}
