//! Local SQLite store: source snapshots, alert episodes and a bounded cache.
//!
//! Everything lives under the platform per-user application data directory,
//! never the installation folder. Writes are idempotent transactions keyed by
//! (product, retrieved_at); an older response never overwrites a newer one.

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use swo_core::alert::AlertMemory;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// A stored raw payload exactly as retrieved, with its hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: i64,
    pub product: String,
    pub source_url: String,
    pub retrieved_at: DateTime<Utc>,
    pub sha256: String,
    pub payload: String,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self, StoreError> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                product TEXT NOT NULL,
                source_url TEXT NOT NULL,
                retrieved_at TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                payload TEXT NOT NULL,
                UNIQUE (product, sha256)
            );
            CREATE INDEX IF NOT EXISTS snapshots_product_time
                ON snapshots (product, retrieved_at DESC);
            CREATE TABLE IF NOT EXISTS alert_memory (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
        )?;
        conn.execute(
            "INSERT OR REPLACE INTO meta (key, value) VALUES ('schema_version', ?1)",
            params![SCHEMA_VERSION.to_string()],
        )?;
        Ok(Self { conn })
    }

    pub fn schema_version(&self) -> Result<u32, StoreError> {
        let v: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.and_then(|s| s.parse().ok()).unwrap_or(0))
    }

    /// Store a retrieved payload. Idempotent: re-storing identical bytes for a
    /// product keeps the original row and its earlier `retrieved_at`.
    pub fn put_snapshot(
        &self,
        product: &str,
        source_url: &str,
        retrieved_at: DateTime<Utc>,
        payload: &str,
    ) -> Result<Snapshot, StoreError> {
        let sha = sha256_hex(payload);
        self.conn.execute(
            "INSERT OR IGNORE INTO snapshots (product, source_url, retrieved_at, sha256, payload)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![product, source_url, retrieved_at.to_rfc3339(), sha, payload],
        )?;
        self.snapshot_by_hash(product, &sha)
            .map(|s| s.expect("row was just inserted or already present"))
    }

    fn snapshot_by_hash(&self, product: &str, sha: &str) -> Result<Option<Snapshot>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, product, source_url, retrieved_at, sha256, payload
                 FROM snapshots WHERE product = ?1 AND sha256 = ?2",
                params![product, sha],
                row_to_snapshot,
            )
            .optional()?)
    }

    /// Newest snapshot for a product, used as last-known-good on failure.
    pub fn latest_snapshot(&self, product: &str) -> Result<Option<Snapshot>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, product, source_url, retrieved_at, sha256, payload
                 FROM snapshots WHERE product = ?1
                 ORDER BY datetime(retrieved_at) DESC, id DESC LIMIT 1",
                params![product],
                row_to_snapshot,
            )
            .optional()?)
    }

    /// Snapshots available for replay, newest first.
    pub fn snapshot_index(
        &self,
        product: &str,
        limit: u32,
    ) -> Result<Vec<(i64, DateTime<Utc>)>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, retrieved_at FROM snapshots WHERE product = ?1
             ORDER BY datetime(retrieved_at) DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![product, limit], |r| {
            let ts: String = r.get(1)?;
            Ok((r.get::<_, i64>(0)?, ts))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (id, ts) = row?;
            if let Ok(t) = DateTime::parse_from_rfc3339(&ts) {
                out.push((id, t.with_timezone(&Utc)));
            }
        }
        Ok(out)
    }

    pub fn snapshot_by_id(&self, id: i64) -> Result<Option<Snapshot>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, product, source_url, retrieved_at, sha256, payload FROM snapshots WHERE id = ?1",
                params![id],
                row_to_snapshot,
            )
            .optional()?)
    }

    pub fn save_alert_memory(&self, memory: &AlertMemory) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT OR REPLACE INTO alert_memory (id, json, updated_at) VALUES (1, ?1, ?2)",
            params![serde_json::to_string(memory)?, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    /// Load persisted alert memory. A corrupt row yields empty memory rather
    /// than a crash; episode history is then rebuilt from fresh data.
    pub fn load_alert_memory(&self) -> Result<AlertMemory, StoreError> {
        let raw: Option<String> = self
            .conn
            .query_row("SELECT json FROM alert_memory WHERE id = 1", [], |r| {
                r.get(0)
            })
            .optional()?;
        Ok(raw
            .and_then(|j| serde_json::from_str(&j).ok())
            .unwrap_or_default())
    }

    /// Enforce the retention policy: keep at most `keep` snapshots per product,
    /// then trim oldest rows overall until the database is within `limit_bytes`.
    pub fn enforce_limits(
        &self,
        keep_per_product: u32,
        limit_bytes: u64,
    ) -> Result<u64, StoreError> {
        self.conn.execute(
            "DELETE FROM snapshots WHERE id NOT IN (
                 SELECT id FROM (
                     SELECT id, ROW_NUMBER() OVER (
                         PARTITION BY product ORDER BY datetime(retrieved_at) DESC, id DESC
                     ) AS rn FROM snapshots
                 ) WHERE rn <= ?1
             )",
            params![keep_per_product],
        )?;
        // Trimming for size never removes the newest snapshot of a product:
        // last-known-good must survive a full cache, or an offline launch
        // would lose the very data it depends on.
        let mut removed = 0u64;
        while self.size_bytes()? > limit_bytes {
            let n = self.conn.execute(
                "DELETE FROM snapshots WHERE id IN (
                     SELECT id FROM snapshots
                     WHERE id NOT IN (
                         SELECT id FROM (
                             SELECT id, ROW_NUMBER() OVER (
                                 PARTITION BY product ORDER BY datetime(retrieved_at) DESC, id DESC
                             ) AS rn FROM snapshots
                         ) WHERE rn = 1
                     )
                     ORDER BY datetime(retrieved_at) ASC, id ASC LIMIT 8
                 )",
                [],
            )?;
            if n == 0 {
                break;
            }
            removed += n as u64;
        }
        if removed > 0 {
            self.conn.execute_batch("VACUUM")?;
        }
        Ok(removed)
    }

    /// Approximate on-disk size, computed from SQLite's own page accounting.
    pub fn size_bytes(&self) -> Result<u64, StoreError> {
        let pages: i64 = self.conn.query_row("PRAGMA page_count", [], |r| r.get(0))?;
        let size: i64 = self.conn.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        Ok((pages * size).max(0) as u64)
    }

    pub fn snapshot_count(&self) -> Result<i64, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM snapshots", [], |r| r.get(0))?)
    }
}

fn row_to_snapshot(row: &rusqlite::Row<'_>) -> rusqlite::Result<Snapshot> {
    let ts: String = row.get(3)?;
    Ok(Snapshot {
        id: row.get(0)?,
        product: row.get(1)?,
        source_url: row.get(2)?,
        retrieved_at: DateTime::parse_from_rfc3339(&ts)
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        sha256: row.get(4)?,
        payload: row.get(5)?,
    })
}

pub fn sha256_hex(payload: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(payload.as_bytes());
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn t(min: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_757_000_000 + min * 60, 0).unwrap()
    }

    #[test]
    fn storing_the_same_payload_twice_is_idempotent() {
        let s = Store::open_in_memory().unwrap();
        let a = s.put_snapshot("p", "https://x", t(0), "[1,2,3]").unwrap();
        let b = s.put_snapshot("p", "https://x", t(5), "[1,2,3]").unwrap();
        assert_eq!(
            a.id, b.id,
            "identical bytes must not create a second snapshot"
        );
        assert_eq!(
            b.retrieved_at,
            t(0),
            "the original retrieval time is preserved"
        );
        assert_eq!(s.snapshot_count().unwrap(), 1);
    }

    #[test]
    fn changed_payloads_create_new_snapshots_and_latest_wins() {
        let s = Store::open_in_memory().unwrap();
        s.put_snapshot("p", "https://x", t(0), "[1]").unwrap();
        s.put_snapshot("p", "https://x", t(1), "[2]").unwrap();
        assert_eq!(s.latest_snapshot("p").unwrap().unwrap().payload, "[2]");
        assert_eq!(s.snapshot_count().unwrap(), 2);
    }

    #[test]
    fn an_older_response_does_not_become_the_latest() {
        let s = Store::open_in_memory().unwrap();
        s.put_snapshot("p", "https://x", t(10), "[new]").unwrap();
        s.put_snapshot("p", "https://x", t(0), "[old]").unwrap();
        assert_eq!(
            s.latest_snapshot("p").unwrap().unwrap().payload,
            "[new]",
            "a late-arriving older response must not overwrite newer data"
        );
    }

    #[test]
    fn payload_hash_is_recorded_for_reproducible_exports() {
        let s = Store::open_in_memory().unwrap();
        let snap = s.put_snapshot("p", "https://x", t(0), "hello").unwrap();
        assert_eq!(snap.sha256, sha256_hex("hello"));
        assert_eq!(snap.sha256.len(), 64);
    }

    #[test]
    fn last_known_good_survives_a_failed_refresh() {
        let s = Store::open_in_memory().unwrap();
        s.put_snapshot("p", "https://x", t(0), "[good]").unwrap();
        // A failed fetch simply stores nothing; the previous snapshot remains.
        assert_eq!(s.latest_snapshot("p").unwrap().unwrap().payload, "[good]");
        assert!(s.latest_snapshot("other").unwrap().is_none());
    }

    #[test]
    fn retention_keeps_the_newest_snapshots_per_product() {
        let s = Store::open_in_memory().unwrap();
        for i in 0..10 {
            s.put_snapshot("wind", "https://x", t(i), &format!("[{i}]"))
                .unwrap();
            s.put_snapshot("mag", "https://y", t(i), &format!("[m{i}]"))
                .unwrap();
        }
        s.enforce_limits(3, u64::MAX).unwrap();
        assert_eq!(s.snapshot_count().unwrap(), 6, "3 per product");
        assert_eq!(s.latest_snapshot("wind").unwrap().unwrap().payload, "[9]");
    }

    #[test]
    fn a_size_limit_trims_oldest_snapshots() {
        let s = Store::open_in_memory().unwrap();
        let big = "x".repeat(64 * 1024);
        for i in 0..24 {
            s.put_snapshot("wind", "https://x", t(i), &format!("{big}{i}"))
                .unwrap();
        }
        s.put_snapshot("mag", "https://y", t(0), &format!("{big}mag"))
            .unwrap();
        let before = s.snapshot_count().unwrap();
        s.enforce_limits(1000, 256 * 1024).unwrap();
        let after = s.snapshot_count().unwrap();
        assert!(after < before, "the cache must be bounded");
        assert_eq!(
            after, 2,
            "the newest snapshot of each product survives trimming"
        );
        assert!(
            s.latest_snapshot("wind").unwrap().is_some()
                && s.latest_snapshot("mag").unwrap().is_some(),
            "last-known-good must survive a full cache"
        );
    }

    #[test]
    fn alert_memory_round_trips_and_survives_corruption() {
        let s = Store::open_in_memory().unwrap();
        let m = AlertMemory {
            settings_version: 7,
            ..Default::default()
        };
        s.save_alert_memory(&m).unwrap();
        assert_eq!(s.load_alert_memory().unwrap().settings_version, 7);

        s.conn
            .execute("UPDATE alert_memory SET json = 'garbage' WHERE id = 1", [])
            .unwrap();
        assert_eq!(
            s.load_alert_memory().unwrap(),
            AlertMemory::default(),
            "a corrupt cache entry must not crash startup"
        );
    }

    #[test]
    fn snapshot_index_supports_replay_selection() {
        let s = Store::open_in_memory().unwrap();
        for i in 0..5 {
            s.put_snapshot("wind", "https://x", t(i * 10), &format!("[{i}]"))
                .unwrap();
        }
        let idx = s.snapshot_index("wind", 3).unwrap();
        assert_eq!(idx.len(), 3);
        assert!(idx[0].1 > idx[1].1, "newest first");
        let picked = s.snapshot_by_id(idx[2].0).unwrap().unwrap();
        assert!(picked.retrieved_at <= idx[0].1);
    }

    #[test]
    fn schema_version_is_recorded() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.schema_version().unwrap(), SCHEMA_VERSION);
    }

    #[test]
    fn a_store_opens_and_reopens_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cache").join("swo.sqlite3");
        {
            let s = Store::open(&path).unwrap();
            s.put_snapshot("p", "https://x", t(0), "[1]").unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.latest_snapshot("p").unwrap().unwrap().payload, "[1]");
        assert!(s.size_bytes().unwrap() > 0);
        let _ = Duration::seconds(0);
    }
}
