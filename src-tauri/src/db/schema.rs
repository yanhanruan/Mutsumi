//! SQLite schema + forward-only migrations.
//!
//! Versioning uses SQLite's built-in `user_version` pragma: each migration bumps
//! it by one, and [`migrate`] applies only the steps newer than the stored
//! version. To evolve the schema, add a new `const V_N` block and an `if` arm
//! below — never edit a shipped migration.

use rusqlite::Connection;

/// The schema version this build expects. Equals the number of migration steps.
pub const SCHEMA_VERSION: i32 = 5;

/// V1 — the initial three-table design from the blueprint.
const V1: &str = r#"
-- Global user preferences and basic facts, as a simple key/value store.
CREATE TABLE IF NOT EXISTS user_profile (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Real-time numeric relationship metrics. Single canonical row (id = 1).
CREATE TABLE IF NOT EXISTS relationship_state (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    affection  REAL NOT NULL DEFAULT 0,
    trust      REAL NOT NULL DEFAULT 0,
    mood       TEXT NOT NULL DEFAULT 'neutral',
    updated_at INTEGER NOT NULL
);

-- Core memory stream: raw observations and high-level reflections, with an
-- optional embedding BLOB (little-endian f32) for semantic retrieval.
CREATE TABLE IF NOT EXISTS memories (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kind        TEXT NOT NULL,                 -- 'observation' | 'reflection'
    category    TEXT,                          -- e.g. 'preference','habit','state'
    content     TEXT NOT NULL,
    importance  REAL NOT NULL DEFAULT 0.5,     -- 0..1
    embedding   BLOB,                          -- little-endian f32 array, nullable
    created_at  INTEGER NOT NULL,
    last_access INTEGER NOT NULL,
    reflected   INTEGER NOT NULL DEFAULT 0     -- 0/1: folded into a reflection yet
);

CREATE INDEX IF NOT EXISTS idx_memories_kind     ON memories(kind);
CREATE INDEX IF NOT EXISTS idx_memories_reflected ON memories(reflected);
"#;

/// V2 — memory lifecycle columns:
///   * `updated_at`  — when the row's content was last written/refreshed (an
///     "as-of" timestamp used for conflict resolution + the prompt recency hint).
///     Distinct from `last_access` (bumped on *retrieval*). Backfilled to
///     `created_at` for existing rows.
///   * `superseded`  — 0/1 active flag. Retrieval only returns active rows; a
///     contradicting/merged memory can be retired without deleting history.
const V2: &str = r#"
ALTER TABLE memories ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
UPDATE memories SET updated_at = created_at;
ALTER TABLE memories ADD COLUMN superseded INTEGER NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS idx_memories_superseded ON memories(superseded);
"#;

/// V3 — `subject`: whose memory this row is.
///   * `'user'` — a fact about the user (the only kind before this migration, so
///     existing rows correctly default to it).
///   * `'self'` — Mutsumi's own memory: her feelings toward the user and the
///     promises / shared moments she expressed. Kept distinct in retrieval and the
///     prompt so the two are never confused.
const V3: &str = r#"
ALTER TABLE memories ADD COLUMN subject TEXT NOT NULL DEFAULT 'user';
CREATE INDEX IF NOT EXISTS idx_memories_subject ON memories(subject);
"#;

/// V4 — verbatim chat transcript for the IM-style history view.
///   A single, ever-growing message stream (no session/conversation boundaries):
///   `id` (AUTOINCREMENT) is monotonic, so it doubles as the chronological sort
///   key and the keyset-pagination cursor; `created_at` is indexed for date-range
///   filtering. Distinct from `memories`, which holds *learned* facts/reflections;
///   this is the raw conversation the user reads and searches.
const V4: &str = r#"
CREATE TABLE IF NOT EXISTS chat_messages (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    role        TEXT NOT NULL,            -- 'user' | 'assistant'
    content     TEXT NOT NULL,
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_chat_messages_created ON chat_messages(created_at);
"#;

/// V5 — multimodal transcript: image messages.
///   * `kind`       — 'text' (default; all existing rows) | 'image'.
///   * `image_path` — for image rows, the **relative** path under the media dir
///     (e.g. `media/2026/06/…jpg`). The DB never stores image bytes/BLOBs — only
///     this path — so it stays lightweight and query-efficient.
const V5: &str = r#"
ALTER TABLE chat_messages ADD COLUMN kind TEXT NOT NULL DEFAULT 'text';
ALTER TABLE chat_messages ADD COLUMN image_path TEXT;
"#;

/// Apply all migrations newer than the connection's stored `user_version`.
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let current: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;

    if current < 1 {
        conn.execute_batch(V1)?;
    }
    if current < 2 {
        conn.execute_batch(V2)?;
    }
    if current < 3 {
        conn.execute_batch(V3)?;
    }
    if current < 4 {
        conn.execute_batch(V4)?;
    }
    if current < 5 {
        conn.execute_batch(V5)?;
    }

    if current < SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::OptionalExtension;

    fn columns(conn: &Connection, table: &str) -> Vec<String> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        rows
    }

    fn table_exists(conn: &Connection, table: &str) -> bool {
        conn.query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |_| Ok(()),
        )
        .optional()
        .unwrap()
        .is_some()
    }

    #[test]
    fn fresh_migrate_has_all_columns_and_version() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();
        let cols = columns(&conn, "memories");
        assert!(cols.iter().any(|c| c == "updated_at"));
        assert!(cols.iter().any(|c| c == "superseded"));
        assert!(cols.iter().any(|c| c == "subject")); // V3
        assert!(table_exists(&conn, "chat_messages")); // V4
        let chat_cols = columns(&conn, "chat_messages");
        assert!(chat_cols.iter().any(|c| c == "kind")); // V5
        assert!(chat_cols.iter().any(|c| c == "image_path")); // V5
        let v: i32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn v4_to_v5_upgrade_adds_image_columns() {
        // A pre-V5 database (simulated through V4) gains kind + image_path.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(V1).unwrap();
        conn.execute_batch(V2).unwrap();
        conn.execute_batch(V3).unwrap();
        conn.execute_batch(V4).unwrap();
        conn.pragma_update(None, "user_version", 4).unwrap();
        let before = columns(&conn, "chat_messages");
        assert!(!before.iter().any(|c| c == "image_path"));

        migrate(&conn).unwrap();

        let after = columns(&conn, "chat_messages");
        assert!(after.iter().any(|c| c == "kind"));
        assert!(after.iter().any(|c| c == "image_path"));
        let v: i32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn v1_to_v4_upgrade_adds_chat_messages() {
        // A pre-V4 database (simulated as V3) gains chat_messages after migrate.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(V1).unwrap();
        conn.execute_batch(V2).unwrap();
        conn.execute_batch(V3).unwrap();
        conn.pragma_update(None, "user_version", 3).unwrap();
        assert!(!table_exists(&conn, "chat_messages"));

        migrate(&conn).unwrap();

        assert!(table_exists(&conn, "chat_messages"));
        let v: i32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn v1_to_v3_upgrade_backfills_subject_to_user() {
        // A V1 DB with a pre-existing row → after migrate, subject defaults 'user'.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(V1).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO memories (kind, content, importance, created_at, last_access, reflected)
             VALUES ('observation', 'x', 0.5, 1, 1, 0)",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        let subject: String = conn
            .query_row("SELECT subject FROM memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(subject, "user");
    }

    #[test]
    fn v1_to_v2_upgrade_backfills_updated_at() {
        // Simulate a V1 database, then migrate forward.
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(V1).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute(
            "INSERT INTO memories (kind, content, importance, created_at, last_access, reflected)
             VALUES ('observation', 'x', 0.5, 1234, 1234, 0)",
            [],
        )
        .unwrap();

        migrate(&conn).unwrap();

        // New columns exist and updated_at was backfilled from created_at.
        let updated_at: i64 = conn
            .query_row("SELECT updated_at FROM memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(updated_at, 1234);
        let superseded: i64 = conn
            .query_row("SELECT superseded FROM memories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(superseded, 0);
    }
}
