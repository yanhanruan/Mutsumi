"""
Frame normalization preprocessor.

AI-generated video clips for different actions (idle, click, ...) come out
with slightly inconsistent character size, framing, and vertical position.
Playing them back-to-back makes the pet visibly "jump" at each transition.

This script normalizes every frame so the character is the same height and
anchored to the same point on the canvas, regardless of which action clip
it came from.

It also trims unstable frames from the start and end of each sequence.
AI video generators tend to produce poorly-converged frames at clip
boundaries (the model has less context at t=0 and t=N), which appear as a
visible flicker when one animation transitions to another. Use --trim-head
and --trim-tail to drop those frames.

After trimming, the output is renamed contiguously as frame_001.webp,
frame_002.webp, ... so the playback code (which expects unbroken
sequential numbering) still works.

⚠️  CRITICAL: every action set MUST be normalized with the *same*
    canvas/anchor/resize parameters (--canvas-size, --target-height,
    --anchor-y-ratio). Identical parameters are what guarantees the
    frames line up across actions. Pick one set of values and reuse them
    for every action directory.

    --trim-head / --trim-tail may differ per action set — they only
    affect how many boundary frames are discarded, not character
    placement.

Pipeline per directory:
    1. Sort all .webp files in the source directory by filename.
    2. Drop the first --trim-head and last --trim-tail frames.
    3. For each remaining frame:
        a. Open RGBA.
        b. getbbox() → tight bounding box of non-transparent pixels.
        c. Crop to that box (drops transparent padding).
        d. Resize the character to --target-height, preserving aspect ratio.
        e. Paste onto a fresh transparent canvas of --canvas-size,
           anchored so the character's bottom-center sits at
           (canvas_w / 2, canvas_h * anchor_y_ratio).
        f. Save as frame_NNN.webp (WebP quality 25), numbered from 1.
"""
from __future__ import annotations
import argparse
import os
from pathlib import Path
from PIL import Image


def normalize_frame(
    src_path:        str,
    dst_path:        str,
    canvas_size:     tuple[int, int],
    target_height:   int,
    anchor_y_ratio:  float,
) -> bool:
    """Normalize a single frame. Returns False if the frame is fully transparent."""
    img = Image.open(src_path).convert('RGBA')

    bbox = img.getbbox()
    if bbox is None:
        return False  # fully transparent — skip

    char = img.crop(bbox)

    # Resize preserving aspect ratio so height == target_height.
    scale = target_height / char.height
    new_w = max(1, int(round(char.width * scale)))
    char  = char.resize((new_w, target_height), Image.LANCZOS)

    canvas_w, canvas_h = canvas_size
    canvas = Image.new('RGBA', canvas_size, (0, 0, 0, 0))

    anchor_x = canvas_w // 2
    anchor_y = int(round(canvas_h * anchor_y_ratio))

    # Bottom-center of the character lands on (anchor_x, anchor_y).
    paste_x = anchor_x - new_w // 2
    paste_y = anchor_y - target_height

    canvas.paste(char, (paste_x, paste_y), char)
    canvas.save(dst_path, 'WEBP', quality=25)
    return True


def normalize_dir(
    src_dir:         str,
    dst_dir:         str,
    canvas_size:     tuple[int, int],
    target_height:   int,
    anchor_y_ratio:  float,
    trim_head:       int = 0,
    trim_tail:       int = 0,
) -> tuple[int, int, int]:
    """
    Normalize every .webp in src_dir into dst_dir.

    Returns (input_count, trimmed_count, output_count).
    """
    os.makedirs(dst_dir, exist_ok=True)

    paths = sorted(Path(src_dir).glob('*.webp'))
    input_count = len(paths)

    if trim_tail > 0:
        paths = paths[trim_head:-trim_tail]
    else:
        paths = paths[trim_head:]

    trimmed_count = input_count - len(paths)

    written = 0
    for idx, src in enumerate(paths, start=1):
        dst = os.path.join(dst_dir, f'frame_{idx:03d}.webp')
        if normalize_frame(str(src), dst, canvas_size, target_height, anchor_y_ratio):
            written += 1

    return input_count, trimmed_count, written


def _parse_size(text: str) -> tuple[int, int]:
    parts = text.lower().replace('x', ',').split(',')
    if len(parts) != 2:
        raise argparse.ArgumentTypeError('canvas size must be WxH or W,H')
    return int(parts[0]), int(parts[1])


def main() -> None:
    p = argparse.ArgumentParser(
        description='Normalize WebP animation frames so character size and '
                    'position are consistent across action sets, optionally '
                    'trimming unstable boundary frames.'
    )
    p.add_argument('--src', required=True, help='source directory of raw .webp frames')
    p.add_argument('--dst', required=True, help='destination directory for normalized frames')
    p.add_argument('--canvas-size',    type=_parse_size, default=(720, 1280),
                   help='canvas size as WxH (default 720x1280)')
    p.add_argument('--target-height',  type=int,         default=800,
                   help='character height in pixels after normalization (default 800)')
    p.add_argument('--anchor-y-ratio', type=float,       default=0.9,
                   help="character's feet position as fraction of canvas height (default 0.9)")
    p.add_argument('--trim-head',      type=int,         default=0,
                   help='number of unstable frames to drop from the START of the sequence (default 0)')
    p.add_argument('--trim-tail',      type=int,         default=0,
                   help='number of unstable frames to drop from the END of the sequence (default 0)')
    args = p.parse_args()

    input_count, trimmed_count, output_count = normalize_dir(
        src_dir        = args.src,
        dst_dir        = args.dst,
        canvas_size    = args.canvas_size,
        target_height  = args.target_height,
        anchor_y_ratio = args.anchor_y_ratio,
        trim_head      = args.trim_head,
        trim_tail      = args.trim_tail,
    )

    print(f'Source:       {args.src}')
    print(f'Destination:  {args.dst}')
    print(f'Input frames: {input_count}')
    print(f'Trimmed:      {trimmed_count}  (head={args.trim_head}, tail={args.trim_tail})')
    print(f'Output frames: {output_count}')


if __name__ == '__main__':
    main()
