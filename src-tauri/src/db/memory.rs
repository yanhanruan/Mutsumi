//! The `memories` table: the core memory stream.
//!
//! Holds both raw observations (Pipeline B — silent extraction) and high-level
//! reflections (Pipeline C — cognitive reflection). [`search`] implements the
//! weighted retrieval from the blueprint: a blend of **relevance** (cosine vs.
//! the query embedding), **recency** (exponential time decay), and stored
//! **importance**.

use rusqlite::{params, Connection, OptionalExtension, Row};

use super::vector::{cosine_similarity, decode_embedding, encode_embedding, relevance_score};

/// Whether a memory is a raw observation or a synthesized reflection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryKind {
    Observation,
    Reflection,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryKind::Observation => "observation",
            MemoryKind::Reflection => "reflection",
        }
    }

    pub fn from_str(s: &str) -> MemoryKind {
        match s {
            "reflection" => MemoryKind::Reflection,
            _ => MemoryKind::Observation,
        }
    }
}

/// A row from the `memories` table.
#[derive(Debug, Clone)]
pub struct Memory {
    pub id: i64,
    pub kind: MemoryKind,
    pub category: Option<String>,
    pub content: String,
    pub importance: f32,
    pub embedding: Option<Vec<f32>>,
    pub created_at: i64,
    pub last_access: i64,
    pub reflected: bool,
}

impl Memory {
    fn from_row(row: &Row) -> rusqlite::Result<Memory> {
        let kind: String = row.get("kind")?;
        let blob: Option<Vec<u8>> = row.get("embedding")?;
        Ok(Memory {
            id: row.get("id")?,
            kind: MemoryKind::from_str(&kind),
            category: row.get("category")?,
            content: row.get("content")?,
            importance: row.get("importance")?,
            embedding: blob.as_deref().and_then(decode_embedding),
            created_at: row.get("created_at")?,
            last_access: row.get("last_access")?,
            reflected: row.get::<_, i64>("reflected")? != 0,
        })
    }
}

/// Fields needed to insert a new memory. `created_at` / `last_access` are set to
/// `now` by [`insert`]; `reflected` starts false.
#[derive(Debug, Clone)]
pub struct NewMemory {
    pub kind: MemoryKind,
    pub category: Option<String>,
    pub content: String,
    pub importance: f32,
    pub embedding: Option<Vec<f32>>,
}

/// A memory paired with its computed retrieval score (for ranking/debugging).
#[derive(Debug, Clone)]
pub struct ScoredMemory {
    pub memory: Memory,
    pub score: f32,
    pub relevance: f32,
}

/// Weights for the retrieval blend. Should roughly sum to 1.0 but need not.
#[derive(Debug, Clone, Copy)]
pub struct RetrievalWeights {
    pub relevance: f32,
    pub recency: f32,
    pub importance: f32,
    /// Half-life (seconds) for recency decay — recency = 0.5^(age / half_life).
    pub recency_half_life_secs: f32,
}

impl Default for RetrievalWeights {
    fn default() -> Self {
        Self {
            relevance: 0.6,
            recency: 0.2,
            importance: 0.2,
            recency_half_life_secs: 7.0 * 24.0 * 3600.0, // one week
        }
    }
}

/// Insert a new memory, returning its row id.
pub fn insert(conn: &Connection, m: &NewMemory, now: i64) -> rusqlite::Result<i64> {
    let blob = m.embedding.as_deref().map(encode_embedding);
    conn.execute(
        "INSERT INTO memories
            (kind, category, content, importance, embedding, created_at, last_access, reflected)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, 0)",
        params![
            m.kind.as_str(),
            m.category,
            m.content,
            m.importance,
            blob,
            now,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Weighted semantic search over all embedded memories.
///
/// Loads candidate rows (those with an embedding), scores each as
/// `relevance*w_r + recency*w_t + importance*w_i`, returns the top `limit`
/// sorted by descending score, and bumps their `last_access` to `now`.
pub fn search(
    conn: &Connection,
    query_embedding: &[f32],
    limit: usize,
    weights: RetrievalWeights,
    now: i64,
) -> rusqlite::Result<Vec<ScoredMemory>> {
    let mut stmt = conn.prepare("SELECT * FROM memories WHERE embedding IS NOT NULL")?;
    let rows = stmt.query_map([], Memory::from_row)?;

    let mut scored: Vec<ScoredMemory> = Vec::new();
    for mem in rows {
        let mem = mem?;
        let relevance = match &mem.embedding {
            Some(e) => relevance_score(cosine_similarity(query_embedding, e)),
            None => 0.0,
        };
        let age = (now - mem.created_at).max(0) as f32;
        let recency = 0.5_f32.powf(age / weights.recency_half_life_secs);
        let score = relevance * weights.relevance
            + recency * weights.recency
            + mem.importance.clamp(0.0, 1.0) * weights.importance;
        scored.push(ScoredMemory {
            memory: mem,
            score,
            relevance,
        });
    }

    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);

    // Touch the retrieved memories so recency reflects actual use.
    if !scored.is_empty() {
        let ids: Vec<i64> = scored.iter().map(|s| s.memory.id).collect();
        touch(conn, &ids, now)?;
    }
    Ok(scored)
}

/// Update `last_access` for the given memory ids.
pub fn touch(conn: &Connection, ids: &[i64], now: i64) -> rusqlite::Result<()> {
    for id in ids {
        conn.execute(
            "UPDATE memories SET last_access = ?1 WHERE id = ?2",
            params![now, id],
        )?;
    }
    Ok(())
}

/// Fetch up to `limit` un-reflected observations, oldest first — the input batch
/// for Pipeline C (cognitive reflection).
pub fn unreflected_observations(conn: &Connection, limit: usize) -> rusqlite::Result<Vec<Memory>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM memories
         WHERE kind = 'observation' AND reflected = 0
         ORDER BY created_at ASC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], Memory::from_row)?;
    rows.collect()
}

/// Count un-reflected observations — drives the reflection threshold.
pub fn unreflected_count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE kind = 'observation' AND reflected = 0",
        [],
        |r| r.get(0),
    )
}

/// Mark the given memories as folded into a reflection.
pub fn mark_reflected(conn: &Connection, ids: &[i64]) -> rusqlite::Result<()> {
    for id in ids {
        conn.execute("UPDATE memories SET reflected = 1 WHERE id = ?1", params![id])?;
    }
    Ok(())
}

/// Look up a single memory by id (used in tests / introspection).
pub fn get(conn: &Connection, id: i64) -> rusqlite::Result<Option<Memory>> {
    conn.query_row("SELECT * FROM memories WHERE id = ?1", params![id], Memory::from_row)
        .optional()
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

    fn new_obs(content: &str, importance: f32, emb: Vec<f32>) -> NewMemory {
        NewMemory {
            kind: MemoryKind::Observation,
            category: Some("preference".into()),
            content: content.into(),
            importance,
            embedding: Some(emb),
        }
    }

    #[test]
    fn insert_and_get_roundtrip() {
        let conn = mem_db();
        let id = insert(&conn, &new_obs("likes cats", 0.7, vec![1.0, 0.0, 0.0]), 100).unwrap();
        let got = get(&conn, id).unwrap().unwrap();
        assert_eq!(got.content, "likes cats");
        assert_eq!(got.importance, 0.7);
        assert_eq!(got.embedding, Some(vec![1.0, 0.0, 0.0]));
        assert!(!got.reflected);
    }

    #[test]
    fn search_ranks_by_relevance() {
        let conn = mem_db();
        insert(&conn, &new_obs("about cats", 0.5, vec![1.0, 0.0, 0.0]), 100).unwrap();
        insert(&conn, &new_obs("about dogs", 0.5, vec![0.0, 1.0, 0.0]), 100).unwrap();

        let hits = search(&conn, &[1.0, 0.0, 0.0], 10, RetrievalWeights::default(), 100).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].memory.content, "about cats"); // most relevant first
        assert!(hits[0].relevance > hits[1].relevance);
    }

    #[test]
    fn reflection_workflow() {
        let conn = mem_db();
        let a = insert(&conn, &new_obs("a", 0.5, vec![1.0]), 100).unwrap();
        let b = insert(&conn, &new_obs("b", 0.5, vec![1.0]), 100).unwrap();
        assert_eq!(unreflected_count(&conn).unwrap(), 2);

        mark_reflected(&conn, &[a, b]).unwrap();
        assert_eq!(unreflected_count(&conn).unwrap(), 0);
        assert!(unreflected_observations(&conn, 10).unwrap().is_empty());
    }

    #[test]
    fn search_touches_last_access() {
        let conn = mem_db();
        let id = insert(&conn, &new_obs("x", 0.5, vec![1.0, 0.0]), 100).unwrap();
        search(&conn, &[1.0, 0.0], 10, RetrievalWeights::default(), 500).unwrap();
        assert_eq!(get(&conn, id).unwrap().unwrap().last_access, 500);
    }
}
