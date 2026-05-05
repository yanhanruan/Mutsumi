# 🌸 Mutsumi Desktop Pet

A lightweight Python desktop companion featuring **Wakaba Mutsumi** (chibi style).  
Runs as a transparent, always-on-top window with no taskbar icon.

---

## Requirements

| Tool | Version |
|------|---------|
| Python | 3.9 + |
| Pillow | 10.0 + |
| PyInstaller | 6.0 + (build only) |

Install runtime dependencies:

```bash
pip install -r requirements.txt
```

---

## Running

```bash
python src/main.py
```

The pet spawns at the **bottom-right corner** of your screen.

---

## Interactions

| Action | Result |
|--------|--------|
| **Left click** | Shake animation + random speech bubble (2 s) |
| **Drag** | Move the pet anywhere on screen |
| **Right click** | Context menu → Exit / Toggle Animation / About |

---

## Replacing the Character Art

1. Prepare a **PNG file with a transparent background** (recommended: 200×200 px or larger square).
2. Name it **`mutsumi.png`** (or `character.png` / `pet.png`).
3. Place it in the **`assets/`** folder next to `src/`.
4. Restart the app — animation frames are generated automatically:
   - **Idle** (8 frames) — gentle vertical float via sine wave
   - **Click** (6 frames) — left/right shake
   - **Drag** (4 frames) — slight rotation tilt

> **Note:** Avoid using pure magenta (`#FF00FF`) in your artwork — that colour is used as the transparency key and will appear as a transparent hole.

If no image is found, a built-in placeholder character is generated on the fly.

---

## Building the `.exe`

Double-click **`build.bat`** (or run it in a terminal):

```bat
build.bat
```

What it does:
1. Runs `pip install -r requirements.txt` to ensure dependencies are up-to-date.
2. Calls PyInstaller with `--onefile --windowed` to produce a single portable `.exe`.
3. Output: **`dist\MutsumiPet.exe`**

**To use custom art with the packaged exe:**  
Create an `assets\` folder in the **same directory as `MutsumiPet.exe`** and place your PNG there.  
The app checks that folder first before falling back to the built-in placeholder.

---

## Project Structure

```
Mutsumi/
├── assets/             ← place your character PNG here
├── src/
│   ├── main.py         ← entry point
│   ├── pet.py          ← window, animation state machine, mouse events
│   ├── bubble.py       ← speech bubble (follows character position)
│   ├── tray.py         ← right-click context menu
│   ├── animator.py     ← frame generation (idle / click / drag)
│   └── placeholder.py  ← built-in fallback character drawn with Pillow
├── requirements.txt
├── build.bat           ← one-click PyInstaller build
└── README.md
```

---

## How Transparency Works

The window background is set to **magenta (`#FF00FF`)**, and Windows'  
`-transparentcolor` attribute makes every pixel of that colour invisible.  
Character frames are composited onto a magenta canvas so transparent areas  
in the original PNG become invisible at runtime — no additional libraries needed.
