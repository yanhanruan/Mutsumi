//! Local image archival for vision chat turns.
//!
//! Validated images are persisted under `<app_data_dir>/media/<YYYY>/<MM>/` and
//! referenced from the transcript by their **relative** path only (the DB never
//! holds image bytes — see [`crate::db::history`]). Compressible formats are
//! re-encoded to JPEG (downscaled, quality-capped) to conserve disk; `.heic` is
//! stored byte-for-byte (the `image` crate can't decode it without libheif).
//!
//! The pure core (`compress_to_jpeg`, `store_in`, `resolve_in`) takes a root
//! `Path` so it is unit-testable without a Tauri `AppHandle`.

use std::path::{Component, Path, PathBuf};

use tauri::{AppHandle, Manager};

/// Longest-side cap (px) for stored images.
const MAX_SIDE: u32 = 1920;
/// JPEG quality for re-encoded images (0–100).
const JPEG_QUALITY: u8 = 82;

/// `<app_data_dir>/media`.
pub fn media_root(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("media"))
}

/// Staging dir for clipboard images before they're sent (`media/.staging`).
pub fn staging_dir(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(media_root(app)?.join(".staging"))
}

/// Compress + archive an image under the media root; returns its **relative**
/// path (e.g. `media/2026/06/<id>.jpg`). HEIC is stored uncompressed.
pub fn store(app: &AppHandle, bytes: &[u8], ext: &str) -> Result<String, String> {
    store_in(&media_root(app)?, bytes, ext)
}

/// Resolve a stored relative path to an absolute one, rejecting any attempt to
/// escape the media root.
pub fn resolve(app: &AppHandle, rel: &str) -> Result<PathBuf, String> {
    resolve_in(&media_root(app)?, rel)
}

/// Whether `ext` is a format the `image` crate can decode + re-encode here.
fn is_compressible(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "jpg" | "jpe" | "jpeg" | "png" | "bmp" | "tif" | "tiff" | "webp"
    )
}

/// Decode, downscale (longest side ≤ [`MAX_SIDE`]), and re-encode to JPEG.
pub fn compress_to_jpeg(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let img = if img.width().max(img.height()) > MAX_SIDE {
        img.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    // JPEG has no alpha channel — flatten to RGB.
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .encode_image(&rgb)
        .map_err(|e| e.to_string())?;
    Ok(out)
}

/// Core of [`store`] against an explicit root (testable). Writes into
/// `root/<YYYY>/<MM>/<unique>.<ext>` and returns the path relative to `root`'s
/// parent (i.e. prefixed with `media/`).
pub fn store_in(root: &Path, bytes: &[u8], ext: &str) -> Result<String, String> {
    let now = chrono::Utc::now();
    let (year, month) = (now.format("%Y").to_string(), now.format("%m").to_string());
    // Unique-enough id: seconds + sub-second nanos.
    let id = format!("{}-{}", now.timestamp(), now.timestamp_subsec_nanos());

    let dir = root.join(&year).join(&month);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let ext = ext.to_ascii_lowercase();
    let (data, out_ext): (Vec<u8>, &str) = if is_compressible(&ext) {
        (compress_to_jpeg(bytes)?, "jpg")
    } else {
        // HEIC (and any non-decodable whitelisted format): keep the original.
        (bytes.to_vec(), if ext.is_empty() { "bin" } else { &ext })
    };

    let filename = format!("{id}.{out_ext}");
    std::fs::write(dir.join(&filename), &data).map_err(|e| e.to_string())?;

    // Relative path stored in the DB, always forward-slashed for portability.
    Ok(format!("media/{year}/{month}/{filename}"))
}

/// Core of [`resolve`] against an explicit root (testable). Rejects absolute
/// paths and any `..` component so a stored value can't escape the media tree.
pub fn resolve_in(root: &Path, rel: &str) -> Result<PathBuf, String> {
    // DB stores `media/…`; the root already *is* media, so strip a leading `media/`.
    let rel = rel.strip_prefix("media/").unwrap_or(rel);
    let p = Path::new(rel);
    if p.is_absolute() || p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("invalid media path".into());
    }
    Ok(root.join(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Encode a solid-color RGB image of the given size to PNG bytes.
    fn png_bytes(w: u32, h: u32) -> Vec<u8> {
        let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            w,
            h,
            image::Rgb([120, 180, 120]),
        ));
        let mut buf = Vec::new();
        img.write_to(&mut Cursor::new(&mut buf), image::ImageFormat::Png)
            .unwrap();
        buf
    }

    #[test]
    fn compress_downscales_and_reencodes_to_jpeg() {
        let big = png_bytes(4000, 1000);
        let out = compress_to_jpeg(&big).unwrap();
        let decoded = image::load_from_memory(&out).unwrap();
        // Longest side capped to MAX_SIDE, aspect preserved.
        assert_eq!(decoded.width(), MAX_SIDE);
        assert!(decoded.height() < decoded.width());
        // JPEG of a flat image is far smaller than the PNG source.
        assert!(out.len() < big.len());
    }

    #[test]
    fn store_in_compresses_and_returns_relative_path() {
        let root = std::env::temp_dir().join(format!("mutsumi-media-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let rel = store_in(&root, &png_bytes(100, 100), "png").unwrap();

        assert!(rel.starts_with("media/"));
        assert!(rel.ends_with(".jpg")); // png → recompressed to jpg
        let abs = resolve_in(&root, &rel).unwrap();
        assert!(abs.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn store_in_preserves_heic_bytes() {
        let root = std::env::temp_dir().join(format!("mutsumi-heic-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let fake_heic = b"\x00\x00\x00\x20ftypheic-not-really-decodable".to_vec();
        let rel = store_in(&root, &fake_heic, "heic").unwrap();

        assert!(rel.ends_with(".heic"));
        let abs = resolve_in(&root, &rel).unwrap();
        assert_eq!(std::fs::read(&abs).unwrap(), fake_heic); // byte-identical
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_in_rejects_escapes() {
        let root = Path::new("/tmp/whatever/media");
        assert!(resolve_in(root, "../secret.txt").is_err());
        assert!(resolve_in(root, "media/../../etc/passwd").is_err());
        // A normal stored value resolves under the root.
        let ok = resolve_in(root, "media/2026/06/x.jpg").unwrap();
        assert!(ok.starts_with(root));
    }
}
