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

/// Dot product of two equal-length `f32` slices.
///
/// On `x86_64` with AVX2 + FMA this runs an 8-wide fused-multiply-add hot loop
/// (`vector::dot_avx2`); everywhere else it falls back to the scalar sum. This is
/// the per-row kernel behind both [`cosine_similarity`] and the cached
/// [`crate::db::index::MemoryIndex`] ranking path. Lengths are assumed equal
/// (debug-asserted); callers that can't guarantee that must check first.
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len(), "dot_product on mismatched lengths");
    #[cfg(target_arch = "x86_64")]
    {
        if is_x86_feature_detected!("avx2") && is_x86_feature_detected!("fma") {
            // SAFETY: guarded by runtime feature detection; slices are equal length.
            return unsafe { dot_avx2(a, b) };
        }
    }
    dot_scalar(a, b)
}

/// Portable scalar dot product — the fallback and the correctness reference.
fn dot_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// AVX2 + FMA dot product. 8 lanes per FMA, scalar tail for the remainder.
///
/// # Safety
/// Caller must ensure the `avx2` and `fma` target features are available (we
/// gate on `is_x86_feature_detected!`). `a` and `b` should be the same length;
/// only `min(len)` lanes are read so over-read is impossible regardless.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2,fma")]
unsafe fn dot_avx2(a: &[f32], b: &[f32]) -> f32 {
    use std::arch::x86_64::*;
    let n = a.len().min(b.len());
    let mut acc = _mm256_setzero_ps();
    let mut i = 0;
    while i + 8 <= n {
        let va = _mm256_loadu_ps(a.as_ptr().add(i));
        let vb = _mm256_loadu_ps(b.as_ptr().add(i));
        acc = _mm256_fmadd_ps(va, vb, acc);
        i += 8;
    }
    // Horizontal sum of the 8 lanes.
    let mut lanes = [0.0f32; 8];
    _mm256_storeu_ps(lanes.as_mut_ptr(), acc);
    let mut sum: f32 = lanes.iter().sum();
    // Scalar tail (n not a multiple of 8).
    while i < n {
        sum += a[i] * b[i];
        i += 1;
    }
    sum
}

/// Cosine similarity in `[-1, 1]`.
///
/// Returns `0.0` when either vector is empty, the lengths differ, or either has
/// zero magnitude — all "no usable signal" cases that should rank as unrelated.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let na = dot_product(a, a);
    let nb = dot_product(b, b);
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot_product(a, b) / (na.sqrt() * nb.sqrt())
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
    fn dot_matches_scalar_reference() {
        // Length deliberately not a multiple of 8 to exercise the AVX2 tail.
        let a: Vec<f32> = (0..1023).map(|i| (i as f32 * 0.013).sin()).collect();
        let b: Vec<f32> = (0..1023).map(|i| (i as f32 * 0.027).cos()).collect();
        let got = dot_product(&a, &b);
        let want = dot_scalar(&a, &b);
        assert!((got - want).abs() <= 1e-2 * want.abs().max(1.0), "got {got}, want {want}");
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
