# Sprite Sheet Format

Drop PNG sprite sheets here to replace the Pillow-generated placeholder animations.
If a sheet is missing, the app falls back to the procedural placeholder automatically —
you can replace them one at a time.

---

## File Naming

| File name       | Animation state | Loops? | Notes                                    |
|-----------------|-----------------|--------|------------------------------------------|
| `idle.png`      | idle floating   | yes    | Plays continuously while pet is at rest  |
| `click.png`     | click / poke    | no     | Plays once on left-click, then back idle |
| `drag.png`      | being dragged   | yes    | Loops while mouse button held            |
| `fly.png`       | thrown / flying | yes    | Frame selected by velocity direction     |
| `dizzy.png`     | just landed     | no     | Plays once after a throw lands           |
| `sleep.png`     | sleepy mood     | yes    | Auto-plays when energy < 20 (SLEEPY)     |

`slow_click` (the sluggish click when sleepy) is derived from `click.png` automatically —
there is no separate `slow_click.png`.

---

## Image Format

- **Format**: PNG with transparency (RGBA recommended)
- **Layout**: horizontal strip — all frames left-to-right in a single row
- **Frame shape**: **square** — each frame is exactly `height × height` pixels
- **Frame count**: auto-detected as `floor(image_width / image_height)`

### Example dimensions

| Animation | Recommended frames | Suggested sheet size  |
|-----------|--------------------|-----------------------|
| idle      | 8                  | 1200 × 150 px         |
| click     | 6                  | 900 × 150 px          |
| drag      | 4                  | 600 × 150 px          |
| fly       | 16                 | 2400 × 150 px         |
| dizzy     | 8                  | 1200 × 150 px         |
| sleep     | 6–8                | 900–1200 × 150 px     |

The frame height (150 px in the table above) becomes the frame width too, giving square crops.
You can use any height as long as the sheet is a single row of square frames.

---

## Playback

- **FPS**: all sheets play at **12 fps** by default
- **Loop**: see the table above; non-looping animations return to `idle` after one cycle
- The `play(name)` method on `DesktopPet` lets you switch to any loaded animation
  programmatically:

  ```python
  pet.play('sleep')   # only works if sleep.png exists
  pet.play('dizzy')
  ```

---

## Art tips

- Keep the character centered in each frame cell; the engine composites it on a
  200 × 200 magenta chroma-key canvas, so extra transparent padding is fine.
- The engine scales nothing — draw at the native frame size you choose.
  150 × 150 px matches the internal `CHAR_SIZE` constant used by the placeholder renderer.
- Use a transparent background (alpha = 0) for the areas outside the character.
