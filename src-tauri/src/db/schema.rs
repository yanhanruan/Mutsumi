//! SQLite schema + forward-only migrations.
//!
//! Versioning uses SQLite's built-in `user_version` pragma: each migration bumps
//! it by one, and [`migrate`] applies only the steps newer than the stored
//! version. To evolve the schema, add a new `const V_N` block and an `if` arm
//! below — never edit a shipped migration.

use rusqlite::Connection;

/// The schema version this build expects. Equals the number of migration steps.
pub const SCHEMA_VERSION: i32 = 1;

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

/// Apply all migrations newer than the connection's stored `user_version`.
pub fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    let current: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;

    if current < 1 {
        conn.execute_batch(V1)?;
    }

    // Future: `if current < 2 { conn.execute_batch(V2)?; }` …

    if current < SCHEMA_VERSION {
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}
