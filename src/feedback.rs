use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

/// One human annotation addressed to an agent session.
///
/// A note may carry a screenshot of the exact region the human drew on. The
/// image is captured when the note is sent, so it still shows what the human
/// meant after the page has navigated or reflowed. Fetch the bytes from the
/// session's `…/feedback/{id}/screenshot` endpoint; only presence is listed.
#[derive(Debug, Clone, Serialize)]
pub struct Feedback {
    pub id: i64,
    pub session: String,
    pub created_at: i64,
    pub author: String,
    pub comment: String,
    pub screenshot: bool,
    pub status: String,
}

/// One recorded control-plane action.
#[derive(Debug, Clone, Serialize)]
pub struct AuditEntry {
    pub id: i64,
    pub at: i64,
    pub session: String,
    pub action: String,
    pub detail: String,
}

/// Durable per-session inbox for human feedback.
///
/// This replaces the old download-plus-filesystem-watcher path: annotations are
/// written straight to SQLite and read back by the agent, so nothing is lost
/// when no watcher is running.
pub struct FeedbackStore {
    conn: Mutex<Connection>,
    audit_retain: i64,
}

fn row_to_feedback(row: &rusqlite::Row<'_>) -> rusqlite::Result<Feedback> {
    Ok(Feedback {
        id: row.get(0)?,
        session: row.get(1)?,
        created_at: row.get(2)?,
        author: row.get(3)?,
        comment: row.get(4)?,
        screenshot: row.get(5)?,
        status: row.get(6)?,
    })
}

fn query(conn: &Connection, session: &str, pending_only: bool) -> Result<Vec<Feedback>> {
    // `screenshot IS NOT NULL` keeps the list payload free of image data; the
    // bytes are served separately from the per-note screenshot endpoint.
    let sql = if pending_only {
        "SELECT id, session, created_at, author, comment,
                (screenshot IS NOT NULL) AS has_screenshot, status
         FROM feedback WHERE session = ?1 AND status = 'pending' ORDER BY id"
    } else {
        "SELECT id, session, created_at, author, comment,
                (screenshot IS NOT NULL) AS has_screenshot, status
         FROM feedback WHERE session = ?1 ORDER BY id"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![session], row_to_feedback)?;
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

/// Add the screenshot column to a database created before notes carried
/// images. Older rows keep their now-unused region columns and read back with
/// no screenshot, which is the best that can be recovered from stale pixels.
fn migrate_screenshot_column(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(feedback)")?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    if !columns.iter().any(|name| name == "screenshot") {
        conn.execute("ALTER TABLE feedback ADD COLUMN screenshot BLOB", [])?;
    }
    Ok(())
}

impl FeedbackStore {
    pub fn open(path: &Path, audit_retain: i64) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let conn = Connection::open(path)
            .with_context(|| format!("opening feedback db {}", path.display()))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS feedback (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                author TEXT NOT NULL,
                comment TEXT NOT NULL,
                screenshot BLOB,
                status TEXT NOT NULL DEFAULT 'pending'
            );
            CREATE INDEX IF NOT EXISTS feedback_session_status
                ON feedback (session, status);
            CREATE TABLE IF NOT EXISTS audit (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                at INTEGER NOT NULL,
                session TEXT NOT NULL,
                action TEXT NOT NULL,
                detail TEXT NOT NULL
            );",
        )
        .context("initializing feedback schema")?;
        migrate_screenshot_column(&conn).context("migrating feedback schema")?;
        Ok(Self {
            conn: Mutex::new(conn),
            audit_retain: audit_retain.max(1),
        })
    }

    pub async fn add(
        &self,
        session: &str,
        author: &str,
        comment: &str,
        screenshot: Option<Vec<u8>>,
    ) -> Result<Feedback> {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let has_screenshot = screenshot.is_some();

        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO feedback (session, created_at, author, comment, screenshot, status)
             VALUES (?1, ?2, ?3, ?4, ?5, 'pending')",
            params![session, created_at, author, comment, screenshot.as_deref()],
        )
        .context("inserting feedback")?;
        let id = conn.last_insert_rowid();
        drop(conn);

        Ok(Feedback {
            id,
            session: session.to_string(),
            created_at,
            author: author.to_string(),
            comment: comment.to_string(),
            screenshot: has_screenshot,
            status: "pending".to_string(),
        })
    }

    /// The PNG attached to a note, or `None` if the note has no screenshot.
    pub async fn screenshot(&self, session: &str, id: i64) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().await;
        let mut stmt =
            conn.prepare("SELECT screenshot FROM feedback WHERE session = ?1 AND id = ?2")?;
        let mut rows = stmt.query(params![session, id])?;
        match rows.next()? {
            Some(row) => Ok(row.get(0)?),
            None => Ok(None),
        }
    }

    pub async fn list(&self, session: &str, pending_only: bool) -> Result<Vec<Feedback>> {
        let conn = self.conn.lock().await;
        query(&conn, session, pending_only)
    }

    /// Return a session's pending notes and acknowledge them in one transaction.
    ///
    /// The read and the acknowledgement have to be the same operation: a note
    /// that arrives between a separate read and a separate `ack-all` would be
    /// acknowledged without ever being shown to the agent.
    pub async fn consume(&self, session: &str) -> Result<Vec<Feedback>> {
        let mut conn = self.conn.lock().await;
        let tx = conn.transaction()?;
        let items = query(&tx, session, true)?;
        if !items.is_empty() {
            tx.execute(
                "UPDATE feedback SET status = 'acked' WHERE session = ?1 AND status = 'pending'",
                params![session],
            )?;
        }
        tx.commit()?;
        Ok(items)
    }

    pub async fn ack(&self, session: &str, id: i64) -> Result<bool> {
        let conn = self.conn.lock().await;
        let changed = conn.execute(
            "UPDATE feedback SET status = 'acked' WHERE session = ?1 AND id = ?2",
            params![session, id],
        )?;
        Ok(changed > 0)
    }

    pub async fn ack_all(&self, session: &str) -> Result<usize> {
        let conn = self.conn.lock().await;
        let changed = conn.execute(
            "UPDATE feedback SET status = 'acked' WHERE session = ?1 AND status = 'pending'",
            params![session],
        )?;
        Ok(changed)
    }

    /// Record a control-plane action for the audit trail.
    pub async fn record(&self, session: &str, action: &str, detail: &str) -> Result<()> {
        let at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO audit (at, session, action, detail) VALUES (?1, ?2, ?3, ?4)",
            params![at, session, action, detail],
        )
        .context("recording audit entry")?;
        // Keep the trail bounded so a long-lived service cannot grow it forever.
        conn.execute(
            "DELETE FROM audit WHERE id <= (SELECT MAX(id) FROM audit) - ?1",
            params![self.audit_retain],
        )
        .context("pruning audit entries")?;
        Ok(())
    }

    /// The most recent audit entries, newest first.
    pub async fn recent(&self, limit: i64) -> Result<Vec<AuditEntry>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare(
            "SELECT id, at, session, action, detail FROM audit ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit.clamp(1, 1000)], |row| {
            Ok(AuditEntry {
                id: row.get(0)?,
                at: row.get(1)?,
                session: row.get(2)?,
                action: row.get(3)?,
                detail: row.get(4)?,
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn audit_retention_bounds_the_trail() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("lumen-audit-{nanos}.db"));
        let store = FeedbackStore::open(&path, 3).unwrap();

        for i in 0..6 {
            store.record("s", "test", &i.to_string()).await.unwrap();
        }

        let recent = store.recent(100).await.unwrap();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].detail, "5");

        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    fn temp_store(label: &str) -> (FeedbackStore, std::path::PathBuf) {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("lumen-{label}-{nanos}.db"));
        (FeedbackStore::open(&path, 100).unwrap(), path)
    }

    #[tokio::test]
    async fn consume_returns_exactly_what_it_acknowledges() {
        let (store, path) = temp_store("consume");
        store.add("s", "human", "first", None).await.unwrap();
        store.add("s", "human", "second", None).await.unwrap();

        let taken = store.consume("s").await.unwrap();
        assert_eq!(
            taken.iter().map(|f| f.comment.as_str()).collect::<Vec<_>>(),
            vec!["first", "second"]
        );
        assert!(store.list("s", true).await.unwrap().is_empty());

        store.add("s", "human", "after", None).await.unwrap();
        let next = store.consume("s").await.unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].comment, "after");

        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn screenshots_round_trip_and_are_listed_by_presence() {
        let (store, path) = temp_store("screenshot");
        let png = b"\x89PNG\r\n\x1a\nnot-really-a-png".to_vec();
        let added = store
            .add("s", "human", "with image", Some(png.clone()))
            .await
            .unwrap();
        store.add("s", "human", "no image", None).await.unwrap();

        assert_eq!(
            store.screenshot("s", added.id).await.unwrap().as_deref(),
            Some(png.as_slice())
        );

        let listed = store.list("s", false).await.unwrap();
        assert!(listed[0].screenshot);
        assert!(!listed[1].screenshot);
        assert!(store.screenshot("s", 404).await.unwrap().is_none());
        assert!(store.screenshot("s", listed[1].id).await.unwrap().is_none());

        drop(store);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn acknowledging_reports_whether_the_note_exists() {
        let (store, path) = temp_store("ack-missing");
        assert!(!store.ack("s", 404).await.unwrap());
        let added = store.add("s", "human", "note", None).await.unwrap();
        assert!(store.ack("s", added.id).await.unwrap());
        // Acknowledging again is idempotent: the note still exists.
        assert!(store.ack("s", added.id).await.unwrap());
        assert!(store.list("s", true).await.unwrap().is_empty());

        drop(store);
        let _ = std::fs::remove_file(&path);
    }
}
