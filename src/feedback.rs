use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

/// A screen region a human commented on, in page CSS pixels.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Region {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// The viewer scale when the region was drawn, for faithful replay.
    pub scale: f64,
}

/// One human annotation addressed to an agent session.
#[derive(Debug, Clone, Serialize)]
pub struct Feedback {
    pub id: i64,
    pub session: String,
    pub created_at: i64,
    pub author: String,
    pub comment: String,
    pub region: Option<Region>,
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
    let region = match (
        row.get::<_, Option<f64>>(5)?,
        row.get::<_, Option<f64>>(6)?,
        row.get::<_, Option<f64>>(7)?,
        row.get::<_, Option<f64>>(8)?,
        row.get::<_, Option<f64>>(9)?,
    ) {
        (Some(x), Some(y), Some(width), Some(height), Some(scale)) => Some(Region {
            x,
            y,
            width,
            height,
            scale,
        }),
        _ => None,
    };
    Ok(Feedback {
        id: row.get(0)?,
        session: row.get(1)?,
        created_at: row.get(2)?,
        author: row.get(3)?,
        comment: row.get(4)?,
        region,
        status: row.get(10)?,
    })
}

fn query(conn: &Connection, session: &str, pending_only: bool) -> Result<Vec<Feedback>> {
    let sql = if pending_only {
        "SELECT id, session, created_at, author, comment, x, y, width, height, scale, status
         FROM feedback WHERE session = ?1 AND status = 'pending' ORDER BY id"
    } else {
        "SELECT id, session, created_at, author, comment, x, y, width, height, scale, status
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
                x REAL, y REAL, width REAL, height REAL, scale REAL,
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
        region: Option<Region>,
    ) -> Result<Feedback> {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or_default();
        let (x, y, width, height, scale) = region
            .map(|r| {
                (
                    Some(r.x),
                    Some(r.y),
                    Some(r.width),
                    Some(r.height),
                    Some(r.scale),
                )
            })
            .unwrap_or((None, None, None, None, None));

        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT INTO feedback
                (session, created_at, author, comment, x, y, width, height, scale, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending')",
            params![session, created_at, author, comment, x, y, width, height, scale],
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
            region,
            status: "pending".to_string(),
        })
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
