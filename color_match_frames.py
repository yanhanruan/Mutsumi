"""
Color-tone matching preprocessor.

normalize_frames.py aligns *position* and *size* across action sets, but
different AI-generated clips still have subtle differences in brightness,
contrast, and color tint. Playing them back-to-back produces a visible
flicker even when the character is in the same spot.

This script applies a classic per-channel mean/std color transfer
(Reinhard-style) to each source frame so its RGB statistics match a
single reference image. The transparent pixels are excluded from the
statistics so the background does not skew the result. Alpha is left
unchanged.

For each channel C ∈ {R, G, B}, every non-transparent source pixel is
adjusted by:

    new = (old - src_mean_C) * (ref_std_C / src_std_C) + ref_mean_C

with src_std_C clamped to 1.0 to avoid division by zero.

Recommended workflow:
    1. ffmpeg               → frames_raw/{idle,click}/
    2. normalize_frames.py  → frames/{idle,click}/         (align size & position)
    3. color_match_frames.py → frames/click_matched/        (align color to idle)
    4. Playback uses frames/idle/ and frames/click_matched/.

Tip: pick a mid-sequence frame from the target animation as the
reference (e.g. frame_010 or frame_015 of idle). frame_001 may still be
slightly off even after trimming, and using a stable mid-sequence frame
gives more representative statistics.
"""
from __future__ import annotations
import argparse
import os
from pathlib import Path
import numpy as np
from PIL import Image


def _rgba_array(path: str) -> np.ndarray:
    """Load an image as float32 RGBA array of shape (H, W, 4)."""
    return np.asarray(Image.open(path).convert('RGBA'), dtype=np.float32)


def _channel_stats(arr: np.ndarray) -> tuple[np.ndarray, np.ndarray] | None:
    """
    Compute per-channel (R, G, B) mean and std over non-transparent pixels.

    Returns (mean[3], std[3]) — or None if the image has no opaque pixels.
    """
    mask = arr[..., 3] > 0
    if not mask.any():
        return None
    pixels = arr[mask][:, :3]  # (N, 3)
    mean = pixels.mean(axis=0)
    std  = pixels.std(axis=0)
    return mean, std


def color_match_frame(
    src_path: str,
    dst_path: str,
    ref_mean: np.ndarray,
    ref_std:  np.ndarray,
) -> bool:
    """
    Color-match a single frame to the reference statistics.

    Returns False if the frame has no opaque pixels (in which case it is
    copied through unmodified).
    """
    arr = _rgba_array(src_path)

    stats = _channel_stats(arr)
    if stats is None:
        # Nothing to match against — save as-is.
        Image.fromarray(arr.astype(np.uint8), 'RGBA').save(dst_path, 'WEBP', quality=25)
        return False

    src_mean, src_std = stats
    src_std = np.where(src_std == 0, 1.0, src_std)  # avoid division by zero

    rgb = arr[..., :3]
    adjusted = (rgb - src_mean) * (ref_std / src_std) + ref_mean
    adjusted = np.clip(adjusted, 0.0, 255.0)

    out = arr.copy()
    out[..., :3] = adjusted
    out_u8 = out.astype(np.uint8)

    Image.fromarray(out_u8, 'RGBA').save(dst_path, 'WEBP', quality=25)
    return True


def color_match_dir(src_dir: str, dst_dir: str, reference_path: str) -> int:
    os.makedirs(dst_dir, exist_ok=True)

    # Compute reference stats ONCE.
    ref_arr   = _rgba_array(reference_path)
    ref_stats = _channel_stats(ref_arr)
    if ref_stats is None:
        raise ValueError(f'reference image has no opaque pixels: {reference_path}')
    ref_mean, ref_std = ref_stats

    paths = sorted(Path(src_dir).glob('*.webp'))
    for src in paths:
        dst = os.path.join(dst_dir, src.name)
        color_match_frame(str(src), dst, ref_mean, ref_std)

    return len(paths)


def main() -> None:
    p = argparse.ArgumentParser(
        description='Color-match WebP animation frames to a reference image '
                    'by aligning per-channel mean and std of non-transparent '
                    'pixels. Run AFTER normalize_frames.py.'
    )
    p.add_argument('--src',       required=True, help='source directory of normalized .webp frames')
    p.add_argument('--dst',       required=True, help='destination directory for color-matched frames')
    p.add_argument('--reference', required=True, help='reference image whose color the source should match')
    args = p.parse_args()

    count = color_match_dir(args.src, args.dst, args.reference)
    print(f'Processed {count} frames: {args.src} -> {args.dst}')
    print(f'Reference: {args.reference}')


if __name__ == '__main__':
    main()
