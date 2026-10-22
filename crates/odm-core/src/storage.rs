//! SQLite persistence for downloads and settings.
//!
//! The database runs in WAL mode so that a crash never leaves a half-written
//! row behind. Access goes through a single connection guarded by a mutex;
//! all statements are short, so this is not a bottleneck.

use crate::error::{DownloadError, ErrorCode, Result};
use crate::model::{Download, DownloadId, DownloadStatus, ErrorInfo, Priority, SegmentState};
use crate::settings::Settings;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SCHEMA_VERSION: i64 = 1;

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| DownloadError::io(&e, "Cannot create data directory"))?;
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(DownloadError::new(
                ErrorCode::Storage,
                "The database was created by a newer version of Veltrix",
            )
            .retryable(false));
        }
        if version < 1 {
            conn.execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS downloads (
                    id TEXT PRIMARY KEY NOT NULL,
                    url TEXT NOT NULL,
                    final_url TEXT,
                    referrer TEXT,
                    file_name TEXT NOT NULL,
                    name_locked INTEGER NOT NULL DEFAULT 0,
                    save_dir TEXT NOT NULL,
                    total_size INTEGER,
                    downloaded INTEGER NOT NULL DEFAULT 0,
                    status TEXT NOT NULL,
                    error_code TEXT,
                    error_message TEXT,
                    error_http_status INTEGER,
                    error_retryable INTEGER NOT NULL DEFAULT 0,
                    category TEXT NOT NULL,
                    priority INTEGER NOT NULL DEFAULT 0,
                    in_queue INTEGER NOT NULL DEFAULT 0,
                    queue_position INTEGER NOT NULL DEFAULT 0,
                    resumable INTEGER NOT NULL DEFAULT 0,
                    etag TEXT,
                    last_modified TEXT,
                    mime TEXT,
                    segments TEXT NOT NULL DEFAULT '[]',
                    connections INTEGER NOT NULL DEFAULT 0,
                    expected_sha256 TEXT,
                    retry_count INTEGER NOT NULL DEFAULT 0,
                    created_at INTEGER NOT NULL,
                    updated_at INTEGER NOT NULL,
                    completed_at INTEGER,
                    avg_speed INTEGER NOT NULL DEFAULT 0,
                    active_ms INTEGER NOT NULL DEFAULT 0,
                    requires_auth INTEGER NOT NULL DEFAULT 0
                );
                CREATE INDEX IF NOT EXISTS idx_downloads_status ON downloads(status);
                CREATE INDEX IF NOT EXISTS idx_downloads_queue ON downloads(in_queue, priority, queue_position);
                CREATE TABLE IF NOT EXISTS settings (
                    key TEXT PRIMARY KEY NOT NULL,
                    value TEXT NOT NULL
                );
                PRAGMA user_version = 1;
                "#,
            )?;
        }
        Ok(Store { conn: Mutex::new(conn) })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        // A panic while holding the lock cannot corrupt SQLite state; keep going.
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn upsert(&self, d: &Download) -> Result<()> {
        let segments = serde_json::to_string(&d.segments).map_err(|e| DownloadError::internal(e.to_string()))?;
        let (ecode, emsg, estatus, eretry) = match &d.error {
            Some(e) => (
                Some(serde_json::to_value(e.code).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()),
                Some(e.message.clone()),
                e.http_status.map(i64::from),
                e.retryable,
            ),
            None => (None, None, None, false),
        };
        self.conn().execute(
            r#"INSERT INTO downloads (
                id, url, final_url, referrer, file_name, name_locked, save_dir, total_size, downloaded, status,
                error_code, error_message, error_http_status, error_retryable, category, priority, in_queue,
                queue_position, resumable, etag, last_modified, mime, segments, connections, expected_sha256,
                retry_count, created_at, updated_at, completed_at, avg_speed, active_ms, requires_auth
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                      ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32)
            ON CONFLICT(id) DO UPDATE SET
                url=excluded.url, final_url=excluded.final_url, referrer=excluded.referrer,
                file_name=excluded.file_name, name_locked=excluded.name_locked, save_dir=excluded.save_dir,
                total_size=excluded.total_size, downloaded=excluded.downloaded, status=excluded.status,
                error_code=excluded.error_code, error_message=excluded.error_message,
                error_http_status=excluded.error_http_status, error_retryable=excluded.error_retryable,
                category=excluded.category, priority=excluded.priority, in_queue=excluded.in_queue,
                queue_position=excluded.queue_position, resumable=excluded.resumable, etag=excluded.etag,
                last_modified=excluded.last_modified, mime=excluded.mime, segments=excluded.segments,
                connections=excluded.connections, expected_sha256=excluded.expected_sha256,
                retry_count=excluded.retry_count, updated_at=excluded.updated_at,
                completed_at=excluded.completed_at, avg_speed=excluded.avg_speed, active_ms=excluded.active_ms,
                requires_auth=excluded.requires_auth"#,
            params![
                d.id.to_string(),
                d.url,
                d.final_url,
                d.referrer,
                d.file_name,
                d.name_locked,
                d.save_dir.to_string_lossy(),
                d.total_size.map(|v| v as i64),
                d.downloaded as i64,
                d.status.as_str(),
                ecode,
                emsg,
                estatus,
                eretry,
                d.category,
                d.priority.as_i64(),
                d.in_queue,
                d.queue_position,
                d.resumable,
                d.etag,
                d.last_modified,
                d.mime,
                segments,
                d.connections,
                d.expected_sha256,
                d.retry_count,
                d.created_at,
                d.updated_at,
                d.completed_at,
                d.avg_speed as i64,
                d.active_ms as i64,
                d.requires_auth,
            ],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: DownloadId) -> Result<()> {
        self.conn().execute("DELETE FROM downloads WHERE id = ?1", params![id.to_string()])?;
        Ok(())
    }

    pub fn get(&self, id: DownloadId) -> Result<Option<Download>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT * FROM downloads WHERE id = ?1")?;
        Ok(stmt.query_row(params![id.to_string()], row_to_download).optional()?)
    }

    pub fn list(&self) -> Result<Vec<Download>> {
        let conn = self.conn();
        let mut stmt = conn.prepare("SELECT * FROM downloads ORDER BY created_at ASC")?;
        let rows = stmt.query_map([], row_to_download)?;
        let mut out = Vec::new();
        for r in rows {
            match r {
                Ok(d) => out.push(d),
                // A single unreadable row must not make the whole history disappear.
                Err(e) => tracing::warn!(error = %e, "skipping unreadable download row"),
            }
        }
        Ok(out)
    }

    pub fn load_settings(&self) -> Result<Settings> {
        let conn = self.conn();
        let raw: Option<String> =
            conn.query_row("SELECT value FROM settings WHERE key = 'app'", [], |r| r.get(0)).optional()?;
        let mut settings = match raw {
            Some(json) => serde_json::from_str(&json).unwrap_or_else(|e| {
                tracing::warn!(error = %e, "settings unreadable, using defaults");
                Settings::default()
            }),
            None => Settings::default(),
        };
        settings.normalize();
        Ok(settings)
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let json = serde_json::to_string(settings).map_err(|e| DownloadError::internal(e.to_string()))?;
        self.conn().execute(
            "INSERT INTO settings (key, value) VALUES ('app', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    pub fn get_value(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn().query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0)).optional()?)
    }

    pub fn set_value(&self, key: &str, value: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }
}

fn row_to_download(r: &Row<'_>) -> rusqlite::Result<Download> {
    let id: String = r.get("id")?;
    let id = uuid::Uuid::parse_str(&id)
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))?;
    let status: String = r.get("status")?;
    let status = DownloadStatus::parse(&status).unwrap_or(DownloadStatus::Paused);
    let segments: String = r.get("segments")?;
    let segments: Vec<SegmentState> = serde_json::from_str(&segments).unwrap_or_default();
    let error_code: Option<String> = r.get("error_code")?;
    let error = match error_code {
        Some(code) => Some(ErrorInfo {
            code: serde_json::from_value(serde_json::Value::String(code)).unwrap_or(ErrorCode::Internal),
            message: r.get::<_, Option<String>>("error_message")?.unwrap_or_default(),
            http_status: r.get::<_, Option<i64>>("error_http_status")?.map(|v| v as u16),
            retryable: r.get("error_retryable")?,
        }),
        None => None,
    };
    let save_dir: String = r.get("save_dir")?;
    Ok(Download {
        id,
        url: r.get("url")?,
        final_url: r.get("final_url")?,
        referrer: r.get("referrer")?,
        file_name: r.get("file_name")?,
        name_locked: r.get("name_locked")?,
        save_dir: PathBuf::from(save_dir),
        total_size: r.get::<_, Option<i64>>("total_size")?.map(|v| v as u64),
        downloaded: r.get::<_, i64>("downloaded")? as u64,
        status,
        error,
        category: r.get("category")?,
        priority: Priority::from_i64(r.get("priority")?),
        in_queue: r.get("in_queue")?,
        queue_position: r.get("queue_position")?,
        resumable: r.get("resumable")?,
        etag: r.get("etag")?,
        last_modified: r.get("last_modified")?,
        mime: r.get("mime")?,
        segments,
        connections: r.get("connections")?,
        expected_sha256: r.get("expected_sha256")?,
        retry_count: r.get("retry_count")?,
        created_at: r.get("created_at")?,
        updated_at: r.get("updated_at")?,
        completed_at: r.get("completed_at")?,
        avg_speed: r.get::<_, i64>("avg_speed")? as u64,
        active_ms: r.get::<_, i64>("active_ms")? as u64,
        requires_auth: r.get("requires_auth")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::now_ms;

    fn sample() -> Download {
        Download {
            id: uuid::Uuid::new_v4(),
            url: "https://example.com/a.zip".into(),
            final_url: None,
            referrer: None,
            file_name: "a.zip".into(),
            name_locked: false,
            save_dir: std::env::temp_dir(),
            total_size: Some(100),
            downloaded: 50,
            status: DownloadStatus::Paused,
            error: Some(ErrorInfo {
                code: ErrorCode::Network,
                message: "x".into(),
                http_status: Some(503),
                retryable: true,
            }),
            category: "archives".into(),
            priority: Priority::High,
            in_queue: true,
            queue_position: 3,
            resumable: true,
            etag: Some("\"abc\"".into()),
            last_modified: None,
            mime: Some("application/zip".into()),
            segments: vec![
                SegmentState { start: 0, end: Some(49), written: 49 },
                SegmentState { start: 50, end: Some(99), written: 1 },
            ],
            connections: 0,
            expected_sha256: None,
            retry_count: 1,
            created_at: now_ms(),
            updated_at: now_ms(),
            completed_at: None,
            avg_speed: 10,
            active_ms: 5,
            requires_auth: false,
        }
    }

    #[test]
    fn roundtrip_download() {
        let store = Store::open_in_memory().unwrap();
        let mut d = sample();
        store.upsert(&d).unwrap();
        assert_eq!(store.get(d.id).unwrap().unwrap(), d);
        d.status = DownloadStatus::Completed;
        d.error = None;
        store.upsert(&d).unwrap();
        let all = store.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0], d);
        store.delete(d.id).unwrap();
        assert!(store.get(d.id).unwrap().is_none());
    }

    #[test]
    fn settings_roundtrip() {
        let store = Store::open_in_memory().unwrap();
        let mut s = store.load_settings().unwrap();
        s.segments_per_download = 5;
        store.save_settings(&s).unwrap();
        assert_eq!(store.load_settings().unwrap().segments_per_download, 5);
    }

    #[test]
    fn reopen_file_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db").join("odm.sqlite");
        let d = sample();
        {
            let store = Store::open(&path).unwrap();
            store.upsert(&d).unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.get(d.id).unwrap().unwrap().segments, d.segments);
    }
}
