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

/// Whose memory a row is about. Kept distinct in retrieval + the prompt so the
/// user's facts and Mutsumi's own memory are never conflated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MemorySubject {
    /// A fact about the user (the default; all pre-V3 rows are this).
    #[default]
    User,
    /// Mutsumi's own memory: feelings toward / promises to the user.
    Mutsumi,
}

impl MemorySubject {
    pub fn as_str(self) -> &'static str {
        match self {
            MemorySubject::User => "user",
            MemorySubject::Mutsumi => "self",
        }
    }

    pub fn from_str(s: &str) -> MemorySubject {
        match s {
            "self" => MemorySubject::Mutsumi,
            _ => MemorySubject::User,
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
    /// When the content was last written/refreshed (dedup-merge bumps this).
    /// Distinct from `last_access`, which tracks retrieval. Used for the prompt's
    /// "as-of" recency hint + conflict resolution.
    pub updated_at: i64,
    /// Retired (0/1). Superseded rows are excluded from retrieval but kept for
    /// history.
    pub superseded: bool,
    pub reflected: bool,
    /// Whose memory this is (user fact vs Mutsumi's own).
    pub subject: MemorySubject,
}

impl Memory {
    fn from_row(row: &Row) -> rusqlite::Result<Memory> {
        let kind: String = row.get("kind")?;
        let subject: String = row.get("subject")?;
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
            updated_at: row.get("updated_at")?,
            superseded: row.get::<_, i64>("superseded")? != 0,
            reflected: row.get::<_, i64>("reflected")? != 0,
            subject: MemorySubject::from_str(&subject),
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
    pub subject: MemorySubject,
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
    /// Score multiplier (<1) applied to `Reflection` rows so a stated **fact**
    /// outranks a same-relevance **insight** — speculation shouldn't compete with
    /// what the user actually said.
    pub reflection_factor: f32,
}

impl Default for RetrievalWeights {
    fn default() -> Self {
        // Relevance dominates; recency is a gentle tiebreaker (a long half-life
        // so highly-relevant *old* memories still surface). An earlier
        // relevance=0.6/recency=0.2/half-life=7d default buried 30-day-old
        // memories under fresh-but-irrelevant ones (long-term-consistency
        // benchmark: old-memory recall@5 0.07 → 1.00 after this retune, with
        // no regression on the accuracy set: Recall@5/MRR stay 1.00).
        Self {
            relevance: 0.7,
            recency: 0.1,
            importance: 0.2,
            recency_half_life_secs: 30.0 * 24.0 * 3600.0, // one month
            reflection_factor: 0.8,
        }
    }
}

/// Cosine similarity at/above which a freshly-extracted observation is treated as
/// a restatement of an existing active memory in the same category — folded into
/// it (refresh in place) instead of stored as an overlapping near-duplicate row.
pub const DEDUP_SIMILARITY: f32 = 0.90;

/// Insert a new memory, returning its row id. Sets `created_at` / `last_access` /
/// `updated_at` to `now`; `superseded` + `reflected` start 0.
pub fn insert(conn: &Connection, m: &NewMemory, now: i64) -> rusqlite::Result<i64> {
    let blob = m.embedding.as_deref().map(encode_embedding);
    conn.execute(
        "INSERT INTO memories
            (kind, category, content, importance, embedding, created_at, last_access, updated_at, reflected, subject)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?6, 0, ?7)",
        params![
            m.kind.as_str(),
            m.category,
            m.content,
            m.importance,
            blob,
            now,
            m.subject.as_str(),
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Store an extracted observation, **merging it into a near-duplicate** active
/// memory of the same category when one exists (cosine ≥ [`DEDUP_SIMILARITY`])
/// rather than inserting an overlapping row. Returns the row id written.
///
/// This is what keeps the store from accumulating "用户喜欢小黄瓜 / 喜欢吃黄瓜 /
/// 经常吃黄瓜"-style overlapping triples: a restatement refreshes the existing row
/// (newer phrasing, max importance, bumped `updated_at`).
pub fn store_observation(conn: &Connection, m: &NewMemory, now: i64) -> rusqlite::Result<i64> {
    if let Some(emb) = m.embedding.as_deref() {
        if let Some((existing, sim)) = best_active_match(conn, m.subject, m.category.as_deref(), emb)? {
            if sim >= DEDUP_SIMILARITY {
                refresh(
                    conn,
                    existing.id,
                    &m.content,
                    m.importance.max(existing.importance),
                    Some(emb),
                    now,
                )?;
                return Ok(existing.id);
            }
        }
    }
    insert(conn, m, now)
}

/// The single best-matching *active* memory of the same **subject** + category for
/// `embedding`, paired with its raw cosine similarity (no threshold — the caller
/// decides what to do per band). Brute-force over active embedded rows (same scan
/// `search` uses); subject + category are matched in Rust so a self-memory never
/// matches a user-fact (and the nullable-category case is handled cleanly).
pub fn best_active_match(
    conn: &Connection,
    subject: MemorySubject,
    category: Option<&str>,
    embedding: &[f32],
) -> rusqlite::Result<Option<(Memory, f32)>> {
    let mut stmt =
        conn.prepare("SELECT * FROM memories WHERE superseded = 0 AND embedding IS NOT NULL")?;
    let rows = stmt.query_map([], Memory::from_row)?;
    let mut best: Option<(f32, Memory)> = None;
    for mem in rows {
        let mem = mem?;
        if mem.subject != subject || mem.category.as_deref() != category {
            continue;
        }
        let Some(e) = &mem.embedding else { continue };
        let sim = cosine_similarity(embedding, e);
        if best.as_ref().map_or(true, |(b, _)| sim > *b) {
            best = Some((sim, mem));
        }
    }
    Ok(best.map(|(sim, m)| (m, sim)))
}

/// Retire a memory: exclude it from retrieval (set `superseded = 1`) while keeping
/// the row for history. Used when a newer memory contradicts/updates it.
pub fn supersede(conn: &Connection, id: i64, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE memories SET superseded = 1, updated_at = ?1 WHERE id = ?2",
        params![now, id],
    )?;
    Ok(())
}

/// Overwrite an existing memory's content/importance/embedding and bump its
/// `updated_at` + `last_access`. Used by dedup-merge to fold a restatement into
/// the row it duplicates rather than store a near-identical new row.
pub fn refresh(
    conn: &Connection,
    id: i64,
    content: &str,
    importance: f32,
    embedding: Option<&[f32]>,
    now: i64,
) -> rusqlite::Result<()> {
    let blob = embedding.map(encode_embedding);
    conn.execute(
        "UPDATE memories
           SET content = ?1, importance = ?2, embedding = ?3, updated_at = ?4, last_access = ?4
         WHERE id = ?5",
        params![content, importance.clamp(0.0, 1.0), blob, now, id],
    )?;
    Ok(())
}

/// Delete every memory row. Backs the user-facing "clear memory" reset; returns
/// the number of rows removed.
pub fn clear_all(conn: &Connection) -> rusqlite::Result<usize> {
    conn.execute("DELETE FROM memories", [])
}

/// Weighted semantic search over all embedded memories (any subject).
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
    search_inner(conn, query_embedding, None, limit, weights, now)
}

/// Like [`search`], but restricted to one [`MemorySubject`] — used by the chat
/// path to fetch user-facts and Mutsumi's own memory separately so both surface.
pub fn search_subject(
    conn: &Connection,
    query_embedding: &[f32],
    subject: MemorySubject,
    limit: usize,
    weights: RetrievalWeights,
    now: i64,
) -> rusqlite::Result<Vec<ScoredMemory>> {
    search_inner(conn, query_embedding, Some(subject), limit, weights, now)
}

fn search_inner(
    conn: &Connection,
    query_embedding: &[f32],
    subject: Option<MemorySubject>,
    limit: usize,
    weights: RetrievalWeights,
    now: i64,
) -> rusqlite::Result<Vec<ScoredMemory>> {
    // Active rows only — superseded memories are retired from retrieval.
    let mut stmt =
        conn.prepare("SELECT * FROM memories WHERE embedding IS NOT NULL AND superseded = 0")?;
    let rows = stmt.query_map([], Memory::from_row)?;

    let mut scored: Vec<ScoredMemory> = Vec::new();
    for mem in rows {
        let mem = mem?;
        if let Some(s) = subject {
            if mem.subject != s {
                continue;
            }
        }
        let relevance = match &mem.embedding {
            Some(e) => relevance_score(cosine_similarity(query_embedding, e)),
            None => 0.0,
        };
        let age = (now - mem.created_at).max(0) as f32;
        let recency = 0.5_f32.powf(age / weights.recency_half_life_secs);
        let mut score = relevance * weights.relevance
            + recency * weights.recency
            + mem.importance.clamp(0.0, 1.0) * weights.importance;
        // Insights (reflections) are down-weighted so a stated fact of equal
        // relevance always ranks above a speculative insight.
        if mem.kind == MemoryKind::Reflection {
            score *= weights.reflection_factor;
        }
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

/// Fetch up to `limit` un-reflected **user** observations, oldest first — the
/// input batch for Pipeline C (cognitive reflection). Self-memories are excluded:
/// reflection synthesizes insights *about the user*, not about Mutsumi.
pub fn unreflected_observations(conn: &Connection, limit: usize) -> rusqlite::Result<Vec<Memory>> {
    let mut stmt = conn.prepare(
        "SELECT * FROM memories
         WHERE kind = 'observation' AND reflected = 0 AND subject = 'user'
         ORDER BY created_at ASC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], Memory::from_row)?;
    rows.collect()
}

/// Count un-reflected user observations — drives the reflection threshold.
pub fn unreflected_count(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row(
        "SELECT COUNT(*) FROM memories WHERE kind = 'observation' AND reflected = 0 AND subject = 'user'",
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
            subject: MemorySubject::User,
        }
    }

    fn new_self(content: &str, emb: Vec<f32>) -> NewMemory {
        NewMemory {
            kind: MemoryKind::Observation,
            category: Some("bond".into()),
            content: content.into(),
            importance: 0.6,
            embedding: Some(emb),
            subject: MemorySubject::Mutsumi,
        }
    }

    fn new_insight(content: &str, importance: f32, emb: Vec<f32>) -> NewMemory {
        NewMemory {
            kind: MemoryKind::Reflection,
            category: Some("insight".into()),
            content: content.into(),
            importance,
            embedding: Some(emb),
            subject: MemorySubject::User,
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

    #[test]
    fn insert_sets_updated_at_and_active() {
        let conn = mem_db();
        let id = insert(&conn, &new_obs("x", 0.5, vec![1.0, 0.0]), 777).unwrap();
        let got = get(&conn, id).unwrap().unwrap();
        assert_eq!(got.updated_at, 777);
        assert!(!got.superseded);
    }

    #[test]
    fn store_observation_refreshes_near_duplicate() {
        let conn = mem_db();
        let id1 = store_observation(&conn, &new_obs("用户喜欢黄瓜", 0.5, vec![1.0, 0.0]), 100).unwrap();
        // Near-identical embedding, same category → folded into the existing row.
        let id2 =
            store_observation(&conn, &new_obs("用户喜欢吃黄瓜", 0.7, vec![0.98, 0.02]), 200).unwrap();
        assert_eq!(id1, id2, "near-duplicate should refresh, not insert");

        let got = get(&conn, id1).unwrap().unwrap();
        assert_eq!(got.content, "用户喜欢吃黄瓜"); // newer phrasing adopted
        assert_eq!(got.importance, 0.7); // max importance kept
        assert_eq!(got.updated_at, 200);

        let n: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1, "only one row should exist");
    }

    #[test]
    fn store_observation_inserts_distinct() {
        let conn = mem_db();
        let a = store_observation(&conn, &new_obs("about cats", 0.5, vec![1.0, 0.0]), 100).unwrap();
        let b = store_observation(&conn, &new_obs("about dogs", 0.5, vec![0.0, 1.0]), 100).unwrap();
        assert_ne!(a, b, "orthogonal embeddings must not merge");
    }

    #[test]
    fn store_observation_respects_category() {
        let conn = mem_db();
        let mk = |cat: &str| NewMemory {
            kind: MemoryKind::Observation,
            category: Some(cat.into()),
            content: "c".into(),
            importance: 0.5,
            embedding: Some(vec![1.0, 0.0]),
            subject: MemorySubject::User,
        };
        // Identical embedding but different category → not merged.
        let a = store_observation(&conn, &mk("preference"), 100).unwrap();
        let b = store_observation(&conn, &mk("habit"), 100).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn store_observation_does_not_merge_across_subjects() {
        let conn = mem_db();
        // Same embedding + same category text, but different subjects → not merged.
        let u = store_observation(&conn, &new_obs("一起调音", 0.5, vec![1.0, 0.0]), 100).unwrap();
        let mut s = new_self("一起调音", vec![1.0, 0.0]);
        s.category = Some("preference".into()); // force same category as new_obs
        let m = store_observation(&conn, &s, 100).unwrap();
        assert_ne!(u, m, "a self-memory must never merge into a user-fact");
    }

    #[test]
    fn search_subject_filters_by_subject() {
        let conn = mem_db();
        insert(&conn, &new_obs("用户喜欢猫", 0.5, vec![1.0, 0.0]), 100).unwrap();
        insert(&conn, &new_self("睦答应陪用户练习", vec![1.0, 0.0]), 100).unwrap();

        let users = search_subject(&conn, &[1.0, 0.0], MemorySubject::User, 10, RetrievalWeights::default(), 100).unwrap();
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].memory.subject, MemorySubject::User);

        let selfs = search_subject(&conn, &[1.0, 0.0], MemorySubject::Mutsumi, 10, RetrievalWeights::default(), 100).unwrap();
        assert_eq!(selfs.len(), 1);
        assert_eq!(selfs[0].memory.subject, MemorySubject::Mutsumi);

        // Unfiltered search still returns both.
        assert_eq!(search(&conn, &[1.0, 0.0], 10, RetrievalWeights::default(), 100).unwrap().len(), 2);
    }

    #[test]
    fn reflection_ignores_self_memories() {
        let conn = mem_db();
        insert(&conn, &new_obs("用户喜欢猫", 0.5, vec![1.0]), 100).unwrap();
        insert(&conn, &new_self("睦觉得和用户聊天很安心", vec![1.0]), 100).unwrap();
        // Only the user observation counts toward the reflection threshold.
        assert_eq!(unreflected_count(&conn).unwrap(), 1);
        let batch = unreflected_observations(&conn, 10).unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].subject, MemorySubject::User);
    }

    #[test]
    fn search_excludes_superseded() {
        let conn = mem_db();
        let id = insert(&conn, &new_obs("hidden", 0.5, vec![1.0, 0.0]), 100).unwrap();
        conn.execute("UPDATE memories SET superseded = 1 WHERE id = ?1", params![id]).unwrap();
        let hits = search(&conn, &[1.0, 0.0], 10, RetrievalWeights::default(), 100).unwrap();
        assert!(hits.is_empty(), "superseded rows must not be retrieved");
    }

    #[test]
    fn best_active_match_returns_top_with_similarity() {
        let conn = mem_db();
        insert(&conn, &new_obs("用户喜欢猫", 0.5, vec![1.0, 0.0]), 100).unwrap();
        // Same subject+category, near-identical embedding → high similarity.
        let m = best_active_match(&conn, MemorySubject::User, Some("preference"), &[0.98, 0.02])
            .unwrap()
            .expect("a match");
        assert_eq!(m.0.content, "用户喜欢猫");
        assert!(m.1 > 0.95, "expected high cosine, got {}", m.1);
        // Different category → no match.
        assert!(best_active_match(&conn, MemorySubject::User, Some("habit"), &[1.0, 0.0]).unwrap().is_none());
    }

    #[test]
    fn supersede_excludes_from_retrieval() {
        let conn = mem_db();
        let id = insert(&conn, &new_obs("旧的事实", 0.5, vec![1.0, 0.0]), 100).unwrap();
        supersede(&conn, id, 200).unwrap();
        assert!(get(&conn, id).unwrap().unwrap().superseded);
        let hits = search(&conn, &[1.0, 0.0], 10, RetrievalWeights::default(), 300).unwrap();
        assert!(hits.is_empty(), "superseded row must not be retrieved");
    }

    #[test]
    fn reflection_is_down_weighted_below_a_same_relevance_fact() {
        let conn = mem_db();
        // A fact and an insight with identical embedding + importance.
        insert(&conn, &new_obs("用户喜欢猫", 0.85, vec![1.0, 0.0]), 100).unwrap();
        insert(&conn, &new_insight("用户可能孤独", 0.85, vec![1.0, 0.0]), 100).unwrap();
        let hits = search(&conn, &[1.0, 0.0], 10, RetrievalWeights::default(), 100).unwrap();
        assert_eq!(hits.len(), 2);
        // The fact outranks the same-relevance insight thanks to reflection_factor.
        assert_eq!(hits[0].memory.kind, MemoryKind::Observation);
        assert_eq!(hits[1].memory.kind, MemoryKind::Reflection);
        assert!(hits[0].score > hits[1].score);
    }

    #[test]
    fn clear_all_empties_the_table() {
        let conn = mem_db();
        insert(&conn, &new_obs("a", 0.5, vec![1.0]), 100).unwrap();
        insert(&conn, &new_obs("b", 0.5, vec![1.0]), 100).unwrap();
        assert_eq!(clear_all(&conn).unwrap(), 2);
        let cnt: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0)).unwrap();
        assert_eq!(cnt, 0);
    }
}
