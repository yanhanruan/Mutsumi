//! Data layer — SQLite-backed dynamic memory.
//!
//! Splits cleanly from the immutable Markdown persona (loaded elsewhere): this
//! module owns all *mutable* state — user profile, relationship metrics, and the
//! memory stream with embeddings.
//!
//! The connection lives behind a [`Mutex`] inside [`Db`], which is registered as
//! Tauri managed state. Because `rusqlite` is synchronous, async commands should
//! touch it via `tauri::async_runtime::spawn_blocking` to avoid blocking the
//! runtime.
//!
//! Storage path: `<app_data_dir>/mutsumi.db` (mirrors `persistence::state_path`).

pub mod memory;
pub mod state;
pub mod vector;

mod schema;

use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{AppHandle, Manager};

/// Managed handle to the SQLite connection.
pub struct Db(pub Mutex<Connection>);

impl Db {
    /// Open (creating if needed) the app database, apply pragmas + migrations.
    pub fn open(app: &AppHandle) -> rusqlite::Result<Db> {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|e| rusqlite::Error::InvalidParameterName(e.to_string()))?;
        std::fs::create_dir_all(&dir).ok();
        Ok(Db(Mutex::new(open_conn(&dir.join("mutsumi.db"))?)))
    }
}

/// Open a connection at `path` with the app's pragmas + migrations applied.
/// Shared by [`Db::open`] and the benchmark harness.
pub(crate) fn open_conn(path: &std::path::Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    // WAL for concurrent reads during background writes; a busy timeout so a
    // brief lock contends rather than erroring immediately.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    schema::migrate(&conn)?;
    Ok(conn)
}

/// Current unix timestamp (seconds) — the single time source for db writes.
pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
