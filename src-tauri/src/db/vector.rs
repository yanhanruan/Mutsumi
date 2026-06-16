//! Embedding (de)serialization + similarity helpers.
//!
//! Pure and DOM-free so it can be unit-tested in isolation. Embeddings are
//! stored in SQLite as a `BLOB` of little-endian `f32`s; retrieval ranking
//! computes cosine similarity in Rust (see [`crate::db::memory::search`]).

/// Encode an `f32` embedding as little-endian bytes for `BLOB` storage.
pub fn encode_embedding(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Decode a `BLOB` back into an `f32` vector.
///
/// Returns `None` if the byte length is not a multiple of 4 (corrupt/foreign
/// data), so callers can skip such rows rather than panic.
pub fn decode_embedding(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

/// Cosine similarity in `[-1, 1]`.
///
/// Returns `0.0` when either vector is empty, the lengths differ, or either has
/// zero magnitude — all "no usable signal" cases that should rank as unrelated.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Map a cosine score into `[0, 1]` for blending with other normalized signals.
/// Negative similarity (unrelated) collapses to `0.0`.
pub fn relevance_score(cos: f32) -> f32 {
    cos.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_embedding() {
        let v = vec![0.0_f32, 1.5, -2.25, 3.125];
        let decoded = decode_embedding(&encode_embedding(&v)).unwrap();
        assert_eq!(v, decoded);
    }

    #[test]
    fn rejects_misaligned_blob() {
        assert!(decode_embedding(&[1, 2, 3]).is_none());
    }

    #[test]
    fn cosine_identical_is_one() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_orthogonal_is_zero() {
        assert!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
    }

    #[test]
    fn cosine_handles_degenerate_inputs() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
        assert_eq!(cosine_similarity(&[1.0, 2.0], &[1.0]), 0.0);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }

    #[test]
    fn relevance_clamps_negative() {
        assert_eq!(relevance_score(-0.5), 0.0);
        assert_eq!(relevance_score(0.42), 0.42);
        assert_eq!(relevance_score(1.5), 1.0);
    }
}
