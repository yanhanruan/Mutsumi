# Mutsumi (Tauri rewrite)

A desktop pet built on **Tauri 2 + Vue 3 + TypeScript**, rewriting the existing PySide6 implementation in `../Mutsumi/`.

## Status — Feature-complete spike

Both layers are functional and compile cleanly:

- **Rust backend** (`src-tauri/src/`)
  - `app_state.rs` — `Mutex<AppState>` holding pet + pomodoro, ticker thread emits events
  - `audio.rs` — Windows WASAPI session-peak polling via `windows-rs` (Windows only)
  - `pomodoro.rs` — phase state machine (Idle / Focus / Break) with countdown
  - `state.rs` — pet state (energy / affection / mood) with decay tick
  - `persistence.rs` — JSON load/save at `%APPDATA%/com.mutsumi.app/state.json`
  - `tray.rs` — system tray menu (Show/Hide/Pomodoro/Settings/Quit)

- **Vue frontend** (`src/`)
  - `composables/useAnimator.ts` — animation registry, locked-step RAF loop, lag clamp, pending-anim slot, hardwired chains (headphones_on→music→music2↔music, headphones_off→idle)
  - `composables/useAudioReaction.ts` — listens for audio-started/stopped events, queues headphones animations with 3s stop debounce
  - `components/PetWindow.vue` — main pet view with click-vs-drag handling
  - `components/ChatBubble.vue` — speech bubble (glassmorphism, auto-hide)
  - `components/PomodoroBadge.vue` — corner badge showing remaining time + phase color
  - `components/SettingsWindow.vue` — secondary window for Pomodoro durations + pet stats

## Run it

```bash
cd Mutsumi-tauri
npm install                    # only first time
npm run tauri dev
```

The first `tauri dev` triggers a Rust dependency compile that can take 3–10 minutes. Subsequent runs are fast.

## Tauri commands exposed to Vue

| Command | Purpose |
|---|---|
| `get_state` | Snapshot of pet + pomodoro state |
| `pet_click` | Increase energy/affection; recomputes mood; persists |
| `pet_drag_end(rough)` | Adjust affection based on drag style; persists |
| `pet_reset` | Reset pet to defaults; persists |
| `pom_start` / `pom_pause` / `pom_stop` | Pomodoro controls |
| `pom_set_durations(focus_mins, break_mins)` | Adjust durations; persists |

## Events emitted by Rust

| Event | When |
|---|---|
| `audio-started` / `audio-stopped` | WASAPI session peak crosses 0.001 threshold |
| `pomodoro-tick` | Every second while pomodoro is running |
| `pomodoro-phase-change` | When focus → break or break → focus auto-transition fires |
| `pet-state-update` | Every 5 seconds (matches Python original's 5s tick) |

## Project layout

```
Mutsumi-tauri/
├── public/
│   └── assets/
│       ├── idle/             234 WebP frames
│       ├── click_matched/    156 frames
│       ├── headphones_on/    185 frames
│       ├── headphones_off/   185 frames
│       ├── music/            185 frames
│       └── music2/           185 frames
├── src/
│   ├── components/           PetWindow, SettingsWindow, ChatBubble, PomodoroBadge
│   ├── composables/          useAnimator, useAudioReaction
│   ├── App.vue               Routes between Pet and Settings via URL param
│   ├── main.ts
│   └── style.css             Global transparency reset
├── src-tauri/
│   ├── src/                  Rust modules (see above)
│   ├── capabilities/         Tauri 2 permission config
│   ├── tauri.conf.json       Two windows: main (transparent pet) + settings (opaque)
│   └── Cargo.toml
└── index.html
```

## Window architecture

Two Tauri windows defined in `tauri.conf.json`:

- **main** — 240×320, transparent, frameless, always-on-top, no taskbar. The pet lives in the bottom 220px, the chat bubble in the top 100px.
- **settings** — 360×320, opaque, decorated, hidden by default. Shown via tray "Settings…" entry. Loads the same `index.html` with `?window=settings`, which makes `App.vue` render `SettingsWindow` instead of `PetWindow`.

## Not yet ported from the Python original

Minor features deferred until the core spike is validated:

- Stretch reminder (cursor-proximity nudges from `activity.py`)
- Z particles sleep effect (`zparticles.py`)
- "Rough" drag detection — currently `pet_drag_end` defaults to gentle. Add Vue-side drag-duration tracking to flip the `rough` flag.
- Asset bundling — currently served from `public/` in dev; for production builds, move to `src-tauri/assets/` as bundled resources.

The Python original in `../Mutsumi/` stays untouched until parity is reached.
