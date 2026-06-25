//! In-memory embedding index — the optimized retrieval hot path.
//!
//! Brute-force [`memory::search`](crate::db::memory::search) re-runs `SELECT *`,
//! decodes every 1024-dim f32 embedding BLOB, and recomputes each stored vector's
//! norm — on *every* query. A single chat turn retrieves twice (user + self
//! subjects), so that is two full O(N·D) BLOB decodes per message. This index pays
//! that cost **once**: it caches the decoded embeddings (plus the scalar fields
//! scoring needs) in RAM, precomputes each vector's inverse L2 norm, and scores
//! rows with a SIMD dot product ([`vector::dot_product`]). Cosine then collapses to
//! one dot per row times two precomputed reciprocals — no per-query sqrt over N, no
//! BLOB decode, no per-row allocation.
//!
//! **Freshness without scattered invalidation.** Rather than make every writer
//! remember to invalidate, the cache stores a cheap `(active_count,
//! max_updated_at)` fingerprint and rebuilds whenever it changes. Insert, refresh
//! (dedup-merge), supersede, and clear-all each move one of the two values, while a
//! `last_access` "touch" moves neither — so retrieval never needlessly rebuilds.
//!
//! Ranking is identical to [`memory::search`]'s blend (relevance · recency ·
//! importance, with the reflection down-weight); a unit test asserts parity.

use std::sync::Mutex;

use rusqlite::Connection;

use super::memory::{self, MemoryKind, MemorySubject, RetrievalWeights, ScoredMemory};
use super::vector::{decode_embedding, dot_product, relevance_score};

/// One cached row: the decoded embedding + precomputed inverse L2 norm + the
/// scalar fields scoring needs. Text/content is intentionally NOT cached — it is
/// read from SQLite only for the handful of top-K winners, keeping the cache small
/// and the hot loop free of string work.
struct IndexedRow {
    id: i64,
    embedding: Vec<f32>,
    /// `1/‖embedding‖` (0.0 for a zero vector) — so cosine is `dot · q_inv · inv_norm`.
    inv_norm: f32,
    importance: f32,
    created_at: i64,
    kind: MemoryKind,
    subject: MemorySubject,
}

/// The decoded-embedding snapshot plus the table fingerprint it was built at.
#[derive(Default)]
struct MemoryIndex {
    rows: Vec<IndexedRow>,
    fingerprint: (i64, i64),
    built: bool,
}

/// `(active_count, max_updated_at)` over non-superseded rows — a cheap change
/// detector, not an exact mirror of the cached set. Any content-changing mutation
/// (insert / refresh / supersede / clear) moves at least one component; a
/// `last_access` touch moves neither. The `embedding IS NOT NULL` filter is left
/// to [`MemoryIndex::rebuild`] so this query is covered by the
/// `idx_memories_active(superseded, updated_at)` index (no table scan).
fn fingerprint(conn: &Connection) -> rusqlite::Result<(i64, i64)> {
    conn.query_row(
        "SELECT COUNT(*), COALESCE(MAX(updated_at), 0)
           FROM memories WHERE superseded = 0",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )
}

impl MemoryIndex {
    /// Rebuild the cache iff the table fingerprint has moved since the last build.
    fn refresh(&mut self, conn: &Connection) -> rusqlite::Result<()> {
        let fp = fingerprint(conn)?;
        if self.built && fp == self.fingerprint {
            return Ok(());
        }
        self.rebuild(conn, fp)
    }

    /// Decode every active embedded row once and precompute its inverse norm.
    fn rebuild(&mut self, conn: &Connection, fp: (i64, i64)) -> rusqlite::Result<()> {
        let mut stmt = conn.prepare(
            "SELECT id, embedding, importance, created_at, kind, subject
               FROM memories WHERE embedding IS NOT NULL AND superseded = 0",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, f32>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?;

        let mut out = Vec::new();
        for r in rows {
            let (id, blob, importance, created_at, kind, subject) = r?;
            let Some(embedding) = decode_embedding(&blob) else { continue };
            let norm = dot_product(&embedding, &embedding).sqrt();
            let inv_norm = if norm == 0.0 { 0.0 } else { 1.0 / norm };
            out.push(IndexedRow {
                id,
                embedding,
                inv_norm,
                importance,
                created_at,
                kind: MemoryKind::from_str(&kind),
                subject: MemorySubject::from_str(&subject),
            });
        }
        self.rows = out;
        self.fingerprint = fp;
        self.built = true;
        Ok(())
    }

    /// Rank the cached rows against `query` (optionally filtered to one subject),
    /// then fetch the full [`Memory`](crate::db::memory::Memory) rows for the top
    /// `limit` and touch their `last_access`. Scoring mirrors
    /// [`memory::search`] exactly.
    fn rank_and_fetch(
        &self,
        conn: &Connection,
        query: &[f32],
        subject: Option<MemorySubject>,
        limit: usize,
        weights: RetrievalWeights,
        now: i64,
    ) -> rusqlite::Result<Vec<ScoredMemory>> {
        let q_norm = dot_product(query, query).sqrt();
        let q_inv = if q_norm == 0.0 { 0.0 } else { 1.0 / q_norm };

        // (score, relevance, id) — keep the loop allocation-free per row.
        let mut scored: Vec<(f32, f32, i64)> = Vec::with_capacity(self.rows.len());
        for row in &self.rows {
            if let Some(s) = subject {
                if row.subject != s {
                    continue;
                }
            }
            let cos = if q_inv == 0.0 || row.inv_norm == 0.0 || query.len() != row.embedding.len() {
                0.0
            } else {
                dot_product(query, &row.embedding) * q_inv * row.inv_norm
            };
            let relevance = relevance_score(cos);
            let age = (now - row.created_at).max(0) as f32;
            let recency = 0.5_f32.powf(age / weights.recency_half_life_secs);
            let mut score = relevance * weights.relevance
                + recency * weights.recency
                + row.importance.clamp(0.0, 1.0) * weights.importance;
            if row.kind == MemoryKind::Reflection {
                score *= weights.reflection_factor;
            }
            scored.push((score, relevance, row.id));
        }

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);

        let ids: Vec<i64> = scored.iter().map(|s| s.2).collect();
        let mut out = Vec::with_capacity(scored.len());
        for (score, relevance, id) in scored {
            if let Some(memory) = memory::get(conn, id)? {
                out.push(ScoredMemory { memory, score, relevance });
            }
        }
        if !ids.is_empty() {
            memory::touch(conn, &ids, now)?;
        }
        Ok(out)
    }
}

/// Thread-safe, self-validating cache around a [`MemoryIndex`]. Lives in
/// [`crate::db::Db`] so the app's retrieval path is served from RAM; the benchmark
/// harness constructs one directly. Lock order: the caller holds the connection
/// mutex (`Db.0`) first, then this cache.
pub struct MemoryIndexCache(Mutex<MemoryIndex>);

impl Default for MemoryIndexCache {
    fn default() -> Self {
        Self(Mutex::new(MemoryIndex::default()))
    }
}

impl MemoryIndexCache {
    /// Cached equivalent of [`memory::search`] (any subject).
    pub fn search(
        &self,
        conn: &Connection,
        query: &[f32],
        limit: usize,
        weights: RetrievalWeights,
        now: i64,
    ) -> rusqlite::Result<Vec<ScoredMemory>> {
        self.run(conn, query, None, limit, weights, now)
    }

    /// Cached equivalent of [`memory::search_subject`].
    pub fn search_subject(
        &self,
        conn: &Connection,
        query: &[f32],
        subject: MemorySubject,
        limit: usize,
        weights: RetrievalWeights,
        now: i64,
    ) -> rusqlite::Result<Vec<ScoredMemory>> {
        self.run(conn, query, Some(subject), limit, weights, now)
    }

    fn run(
        &self,
        conn: &Connection,
        query: &[f32],
        subject: Option<MemorySubject>,
        limit: usize,
        weights: RetrievalWeights,
        now: i64,
    ) -> rusqlite::Result<Vec<ScoredMemory>> {
        let mut idx = self.0.lock().expect("memory index mutex poisoned");
        idx.refresh(conn)?;
        idx.rank_and_fetch(conn, query, subject, limit, weights, now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::memory::{insert, MemoryKind, NewMemory};
    use crate::db::schema;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        schema::migrate(&conn).unwrap();
        conn
    }

    fn obs(content: &str, importance: f32, emb: Vec<f32>, subject: MemorySubject) -> NewMemory {
        NewMemory {
            kind: MemoryKind::Observation,
            category: Some("preference".into()),
            content: content.into(),
            importance,
            embedding: Some(emb),
            subject,
        }
    }

    /// The index must rank identically to the brute-force reference.
    #[test]
    fn index_ranking_matches_brute_force() {
        let conn = mem_db();
        let mut seed = 1u64;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            ((seed >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
        };
        for i in 0..200 {
            let emb: Vec<f32> = (0..64).map(|_| rnd()).collect();
            let subj = if i % 3 == 0 { MemorySubject::Mutsumi } else { MemorySubject::User };
            insert(&conn, &obs(&format!("m{i}"), (i % 10) as f32 / 10.0, emb, subj), 1000 + i).unwrap();
        }

        let cache = MemoryIndexCache::default();
        let w = RetrievalWeights::default();
        let query: Vec<f32> = (0..64).map(|_| rnd()).collect();

        // Unfiltered.
        let brute = memory::search(&conn, &query, 6, w, 5000).unwrap();
        let cached = cache.search(&conn, &query, 6, w, 5000).unwrap();
        let brute_ids: Vec<i64> = brute.iter().map(|s| s.memory.id).collect();
        let cached_ids: Vec<i64> = cached.iter().map(|s| s.memory.id).collect();
        assert_eq!(brute_ids, cached_ids, "top-K ids must match brute force");
        for (a, b) in brute.iter().zip(&cached) {
            assert!((a.score - b.score).abs() < 1e-5, "scores must match");
        }

        // Subject-filtered.
        let brute_u =
            memory::search_subject(&conn, &query, MemorySubject::User, 5, w, 5000).unwrap();
        let cached_u =
            cache.search_subject(&conn, &query, MemorySubject::User, 5, w, 5000).unwrap();
        assert_eq!(
            brute_u.iter().map(|s| s.memory.id).collect::<Vec<_>>(),
            cached_u.iter().map(|s| s.memory.id).collect::<Vec<_>>(),
        );
        assert!(cached_u.iter().all(|s| s.memory.subject == MemorySubject::User));
    }

    /// Inserting a new memory must invalidate the fingerprint so the next search
    /// sees it (no stale cache).
    #[test]
    fn index_picks_up_new_memory_after_insert() {
        let conn = mem_db();
        let cache = MemoryIndexCache::default();
        let w = RetrievalWeights::default();

        insert(&conn, &obs("first", 0.5, vec![1.0, 0.0], MemorySubject::User), 100).unwrap();
        assert_eq!(cache.search(&conn, &[1.0, 0.0], 10, w, 100).unwrap().len(), 1);

        // New row → fingerprint moves → cache rebuilds and returns both.
        insert(&conn, &obs("second", 0.5, vec![0.0, 1.0], MemorySubject::User), 200).unwrap();
        assert_eq!(cache.search(&conn, &[0.0, 1.0], 10, w, 200).unwrap().len(), 2);
    }

    /// Superseding a row must drop it from cached results.
    #[test]
    fn index_drops_superseded_after_refresh() {
        let conn = mem_db();
        let cache = MemoryIndexCache::default();
        let w = RetrievalWeights::default();
        let id = insert(&conn, &obs("gone", 0.5, vec![1.0, 0.0], MemorySubject::User), 100).unwrap();
        assert_eq!(cache.search(&conn, &[1.0, 0.0], 10, w, 100).unwrap().len(), 1);

        memory::supersede(&conn, id, 200).unwrap();
        assert!(cache.search(&conn, &[1.0, 0.0], 10, w, 300).unwrap().is_empty());
    }
}
