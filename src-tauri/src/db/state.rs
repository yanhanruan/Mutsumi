//! The `user_profile` (key/value) and `relationship_state` (singleton) tables.
//!
//! These hold the small, frequently-updated facts and metrics that frame every
//! prompt: who the user is ([`get_profile`] / [`set_profile`]) and how the
//! relationship currently stands ([`get_relationship`] / [`update_relationship`]).

use rusqlite::{params, Connection, OptionalExtension};

// ── user_profile ────────────────────────────────────────────────────

/// Upsert a single profile fact.
pub fn set_profile(conn: &Connection, key: &str, value: &str, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO user_profile (key, value, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        params![key, value, now],
    )?;
    Ok(())
}

/// Read a single profile fact, if present.
pub fn get_profile(conn: &Connection, key: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM user_profile WHERE key = ?1",
        params![key],
        |r| r.get(0),
    )
    .optional()
}

/// All profile facts as `(key, value)` pairs, for prompt assembly.
pub fn all_profile(conn: &Connection) -> rusqlite::Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare("SELECT key, value FROM user_profile ORDER BY key")?;
    let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
    rows.collect()
}

// ── relationship_state ──────────────────────────────────────────────

/// The singleton relationship metrics row.
#[derive(Debug, Clone)]
pub struct RelationshipState {
    pub affection: f32,
    pub trust: f32,
    pub mood: String,
    pub updated_at: i64,
}

impl Default for RelationshipState {
    fn default() -> Self {
        Self {
            affection: 0.0,
            trust: 0.0,
            mood: "neutral".into(),
            updated_at: 0,
        }
    }
}

/// Read the relationship state, creating the default singleton row if absent.
pub fn get_relationship(conn: &Connection) -> rusqlite::Result<RelationshipState> {
    let existing = conn
        .query_row(
            "SELECT affection, trust, mood, updated_at FROM relationship_state WHERE id = 1",
            [],
            |r| {
                Ok(RelationshipState {
                    affection: r.get(0)?,
                    trust: r.get(1)?,
                    mood: r.get(2)?,
                    updated_at: r.get(3)?,
                })
            },
        )
        .optional()?;

    match existing {
        Some(s) => Ok(s),
        None => {
            let def = RelationshipState::default();
            conn.execute(
                "INSERT INTO relationship_state (id, affection, trust, mood, updated_at)
                 VALUES (1, ?1, ?2, ?3, ?4)",
                params![def.affection, def.trust, def.mood, def.updated_at],
            )?;
            Ok(def)
        }
    }
}

/// Wipe all dynamic user state (profile + relationship). Backs the user-facing
/// "clear memory" reset alongside [`crate::db::memory::clear_all`]. The
/// relationship row is deleted so it is recreated at its neutral default on the
/// next [`get_relationship`].
pub fn reset(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute("DELETE FROM user_profile", [])?;
    conn.execute("DELETE FROM relationship_state", [])?;
    Ok(())
}

/// Overwrite the relationship state singleton.
pub fn update_relationship(conn: &Connection, s: &RelationshipState) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO relationship_state (id, affection, trust, mood, updated_at)
         VALUES (1, ?1, ?2, ?3, ?4)
         ON CONFLICT(id) DO UPDATE SET
            affection = excluded.affection,
            trust = excluded.trust,
            mood = excluded.mood,
            updated_at = excluded.updated_at",
        params![s.affection, s.trust, s.mood, s.updated_at],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        conn
    }

    #[test]
    fn profile_upsert() {
        let conn = mem_db();
        set_profile(&conn, "name", "Ren", 1).unwrap();
        assert_eq!(get_profile(&conn, "name").unwrap().as_deref(), Some("Ren"));
        set_profile(&conn, "name", "Ren-kun", 2).unwrap();
        assert_eq!(get_profile(&conn, "name").unwrap().as_deref(), Some("Ren-kun"));
        assert_eq!(all_profile(&conn).unwrap().len(), 1);
    }

    #[test]
    fn relationship_defaults_then_updates() {
        let conn = mem_db();
        let s = get_relationship(&conn).unwrap();
        assert_eq!(s.mood, "neutral");

        update_relationship(
            &conn,
            &RelationshipState {
                affection: 12.0,
                trust: 5.0,
                mood: "amused".into(),
                updated_at: 99,
            },
        )
        .unwrap();
        let s = get_relationship(&conn).unwrap();
        assert_eq!(s.affection, 12.0);
        assert_eq!(s.mood, "amused");
    }

    #[test]
    fn reset_clears_profile_and_relationship() {
        let conn = mem_db();
        set_profile(&conn, "name", "Ren", 1).unwrap();
        update_relationship(
            &conn,
            &RelationshipState { affection: 50.0, trust: 40.0, mood: "warm".into(), updated_at: 9 },
        )
        .unwrap();

        reset(&conn).unwrap();

        assert!(all_profile(&conn).unwrap().is_empty());
        // Relationship row removed → recreated at neutral default on next read.
        let s = get_relationship(&conn).unwrap();
        assert_eq!(s.affection, 0.0);
        assert_eq!(s.mood, "neutral");
    }
}
