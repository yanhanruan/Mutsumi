//! The `chat_messages` table — the verbatim, persistent chat transcript.
//!
//! A single, ever-growing message stream (no session/conversation boundaries):
//! the app presents it like an IM thread. `id` (AUTOINCREMENT) is monotonic, so
//! it is both the chronological sort key and the keyset-pagination cursor —
//! [`recent`] walks *older* (id descending), [`after`] walks *newer* (id
//! ascending). [`search`] backs the History panel (keyword + date range).
//!
//! Distinct from [`crate::db::memory`], which stores *learned* facts and
//! reflections; this is the raw conversation the user reads and searches.

use rusqlite::{params, Connection};
use serde::Serialize;

/// One persisted chat turn. Serializes to the frontend over `invoke`.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StoredMessage {
    pub id: i64,
    pub role: String, // 'user' | 'assistant'
    pub content: String,
    pub created_at: i64,
    /// 'text' | 'image'.
    pub kind: String,
    /// For image rows, the image path. The DB stores a path **relative** to the
    /// media dir; the command layer resolves it to an absolute path before
    /// returning to the frontend. `None` for text rows.
    pub image_path: Option<String>,
}

/// Column list shared by every read query (positional order matches `row_to_msg`).
const COLS: &str = "id, role, content, created_at, kind, image_path";

fn row_to_msg(r: &rusqlite::Row) -> rusqlite::Result<StoredMessage> {
    Ok(StoredMessage {
        id: r.get(0)?,
        role: r.get(1)?,
        content: r.get(2)?,
        created_at: r.get(3)?,
        kind: r.get(4)?,
        image_path: r.get(5)?,
    })
}

/// Escape LIKE wildcards so a user's literal `%` / `_` aren't treated as
/// patterns (paired with `ESCAPE '\'` in the query).
fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
}

/// Append one text turn; returns its new row id.
pub fn append_message(
    conn: &Connection,
    role: &str,
    content: &str,
    now: i64,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO chat_messages (role, content, created_at, kind) VALUES (?1, ?2, ?3, 'text')",
        params![role, content, now],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Append one image message from the user (kind `image`). `caption` is the text
/// the user sent alongside the image (may be empty); `image_path` is the path
/// **relative** to the media dir. Returns the new row id.
pub fn append_image_message(
    conn: &Connection,
    caption: &str,
    image_path: &str,
    now: i64,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO chat_messages (role, content, created_at, kind, image_path)
         VALUES ('user', ?1, ?2, 'image', ?3)",
        params![caption, now, image_path],
    )?;
    Ok(conn.last_insert_rowid())
}

/// The newest `limit` messages with `id < before` (or newest overall when
/// `before` is `None`), returned **ascending** for direct display. Backs the
/// initial load and upward (older) infinite scroll.
pub fn recent(
    conn: &Connection,
    before: Option<i64>,
    limit: usize,
) -> rusqlite::Result<Vec<StoredMessage>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM chat_messages
         WHERE ?1 IS NULL OR id < ?1
         ORDER BY id DESC LIMIT ?2",
    ))?;
    let mut rows: Vec<StoredMessage> = stmt
        .query_map(params![before, limit as i64], row_to_msg)?
        .collect::<rusqlite::Result<_>>()?;
    rows.reverse(); // DESC fetch → ascending for display
    Ok(rows)
}

/// The oldest `limit` messages with `id > after_id`, ascending. Backs downward
/// (newer) scroll after a History jump drops the user mid-timeline. A short page
/// (fewer than `limit`) means the present tail has been reached.
pub fn after(
    conn: &Connection,
    after_id: i64,
    limit: usize,
) -> rusqlite::Result<Vec<StoredMessage>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM chat_messages
         WHERE id > ?1 ORDER BY id ASC LIMIT ?2",
    ))?;
    let rows = stmt
        .query_map(params![after_id, limit as i64], row_to_msg)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Flexible History query: case-insensitive substring (`content LIKE %q%`) when
/// `query` is set, a `created_at` range when `start`/`end` are set, combinable.
/// Newest-first. LIKE (not FTS5) so zh/ja substring matching works without a CJK
/// tokenizer.
pub fn search(
    conn: &Connection,
    query: Option<&str>,
    start: Option<i64>,
    end: Option<i64>,
    limit: usize,
) -> rusqlite::Result<Vec<StoredMessage>> {
    let mut sql = format!("SELECT {COLS} FROM chat_messages");
    let mut conds: Vec<&str> = Vec::new();
    let mut binds: Vec<&dyn rusqlite::ToSql> = Vec::new();

    // `like_pat` must outlive `binds`; only borrowed inside the branch below.
    let like_pat: String;
    let q = query.map(str::trim).filter(|q| !q.is_empty());
    if let Some(q) = q {
        like_pat = format!("%{}%", escape_like(q));
        conds.push("content LIKE ? ESCAPE '\\'");
        binds.push(&like_pat);
    }
    if let Some(ref s) = start {
        conds.push("created_at >= ?");
        binds.push(s);
    }
    if let Some(ref e) = end {
        conds.push("created_at <= ?");
        binds.push(e);
    }
    if !conds.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conds.join(" AND "));
    }
    sql.push_str(" ORDER BY id DESC LIMIT ?");
    let limit = limit as i64;
    binds.push(&limit);

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(binds.as_slice(), row_to_msg)?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
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

    /// Insert `n` alternating user/assistant turns at increasing timestamps;
    /// returns their ids in insertion order.
    fn seed(conn: &Connection, contents: &[&str]) -> Vec<i64> {
        contents
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let role = if i % 2 == 0 { "user" } else { "assistant" };
                append_message(conn, role, c, 1000 + i as i64).unwrap()
            })
            .collect()
    }

    #[test]
    fn append_returns_monotonic_ids() {
        let conn = mem_db();
        let ids = seed(&conn, &["a", "b", "c"]);
        assert!(ids[0] < ids[1] && ids[1] < ids[2]);
    }

    #[test]
    fn image_message_round_trips_kind_and_path() {
        let conn = mem_db();
        append_message(&conn, "user", "hi", 1000).unwrap();
        append_image_message(&conn, "look at this", "media/2026/06/x.jpg", 1001).unwrap();
        let page = recent(&conn, None, 10).unwrap();

        assert_eq!(page[0].kind, "text");
        assert_eq!(page[0].image_path, None);
        assert_eq!(page[1].kind, "image");
        assert_eq!(page[1].role, "user");
        assert_eq!(page[1].content, "look at this"); // caption rides on the image row
        assert_eq!(page[1].image_path.as_deref(), Some("media/2026/06/x.jpg"));
    }

    #[test]
    fn recent_returns_ascending_tail() {
        let conn = mem_db();
        seed(&conn, &["m1", "m2", "m3", "m4", "m5"]);
        let page = recent(&conn, None, 3).unwrap();
        // Newest 3, but displayed oldest-first.
        let contents: Vec<&str> = page.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(contents, ["m3", "m4", "m5"]);
    }

    #[test]
    fn recent_paginates_older_with_keyset() {
        let conn = mem_db();
        seed(&conn, &["m1", "m2", "m3", "m4", "m5"]);
        let tail = recent(&conn, None, 2).unwrap(); // m4, m5
        let oldest = tail.first().unwrap().id;
        let older = recent(&conn, Some(oldest), 2).unwrap(); // m2, m3
        let contents: Vec<&str> = older.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(contents, ["m2", "m3"]);
    }

    #[test]
    fn after_paginates_newer_and_signals_tail() {
        let conn = mem_db();
        let ids = seed(&conn, &["m1", "m2", "m3", "m4"]);
        // From m1, the next 2 newer are m2, m3 (full page).
        let next = after(&conn, ids[0], 2).unwrap();
        let contents: Vec<&str> = next.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(contents, ["m2", "m3"]);
        // From m3, only m4 remains → short page signals the present tail.
        let tail = after(&conn, ids[2], 2).unwrap();
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].content, "m4");
    }

    #[test]
    fn search_matches_substring_case_insensitive_and_cjk() {
        let conn = mem_db();
        seed(&conn, &["Hello World", "再来一杯抹茶拿铁", "goodbye"]);
        // ASCII case-insensitive.
        let hits = search(&conn, Some("hello"), None, None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "Hello World");
        // CJK substring.
        let hits = search(&conn, Some("抹茶"), None, None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "再来一杯抹茶拿铁");
    }

    #[test]
    fn search_filters_by_date_range_and_combines() {
        let conn = mem_db();
        // created_at = 1000, 1001, 1002, 1003.
        seed(&conn, &["alpha", "beta", "alpha again", "gamma"]);
        // Date range only.
        let in_range = search(&conn, None, Some(1001), Some(1002), 10).unwrap();
        let contents: Vec<&str> = in_range.iter().map(|m| m.content.as_str()).collect();
        // Newest-first within [1001,1002].
        assert_eq!(contents, ["alpha again", "beta"]);
        // Keyword + range combined.
        let combined = search(&conn, Some("alpha"), Some(1002), None, 10).unwrap();
        assert_eq!(combined.len(), 1);
        assert_eq!(combined[0].content, "alpha again");
    }

    #[test]
    fn search_treats_wildcards_as_literals() {
        let conn = mem_db();
        seed(&conn, &["50% off today", "discount info"]);
        // The '%' is escaped, so this matches the literal percent, not "anything".
        let hits = search(&conn, Some("50%"), None, None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "50% off today");
    }

    #[test]
    fn search_blank_query_returns_all_newest_first() {
        let conn = mem_db();
        seed(&conn, &["one", "two", "three"]);
        let hits = search(&conn, Some("   "), None, None, 10).unwrap();
        assert_eq!(hits.len(), 3);
        assert_eq!(hits[0].content, "three"); // newest first
    }
}
