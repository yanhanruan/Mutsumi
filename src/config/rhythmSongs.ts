/**
 * rhythmSongs.ts — Rhythm game preset songs and beatmap definitions.
 *
 * ──── HOW TO ADD A NEW SONG / BEATMAP ────────────────────────────
 *
 * ## 1. Create the song entry
 * Add a new object to the PRESET_SONGS array with the following fields:
 *
 *   ```ts
 *   {
 *     id: 'my-song',            // unique kebab-case ID
 *     title: '歌名',            // display title
 *     artist: '歌手名',         // artist name
 *     bpm: 120,                 // beats per minute — affects scroll speed & timing
 *     offsetMs: 2000,           // ms of silence before the first note
 *     audioSrc: '',             // path under public/assets/rhythm/ (WIP)
 *     difficulty: 'easy',       // 'easy' | 'normal' | 'hard'
 *     rating: 3,                // 1–10 star rating
 *     notes: [ ... ],           // array of Note objects (see below)
 *   }
 *   ```
 *
 * ## 2. Note format
 *
 *   ```ts
 *   { time: 2500, direction: 'up',    holdMs: 0 }
 *   { time: 3000, direction: 'down',  holdMs: 500 }
 *   ```
 *
 *   - `time`     — absolute ms from song start when the note reaches the judgment line
 *   - `direction`— 'up' | 'down' | 'left' | 'right'  (maps to ▲ ▼ ◀ ▶)
 *   - `holdMs`   — 0 for tap note, >0 for hold note (hold duration in ms)
 *
 * ## 3. Timing calculation
 *
 *   At BPM = 120, one beat = 60000 / 120 = 500 ms.
 *   At BPM = 160, one beat = 60000 / 160 = 375 ms.
 *
 *   Common note spacings:
 *   | Note type       | 120 BPM  | 140 BPM  | 160 BPM  |
 *   |-----------------|----------|----------|----------|
 *   | Quarter note    |  500 ms  |  428 ms  |  375 ms  |
 *   | Eighth note     |  250 ms  |  214 ms  |  187 ms  |
 *   | Sixteenth note  |  125 ms  |  107 ms  |   93 ms  |
 *
 *   Tip: Use a spreadsheet to calculate note timings:
 *         note_n_time = offsetMs + n * (60000 / bpm) * beatDivider
 *
 * ## 4. Difficulty guidelines
 *
 *   - easy:   quarter notes only, ≤ 4 notes per 4 beats, simple direction patterns
 *   - normal: mixed quarter + eighth notes, some holds, syncopation OK
 *   - hard:   eighth + sixteenth notes, jackhammer patterns, complex holds
 *
 * ## 5. Import from other games / quick contribution
 *
 *   ### 5a. Plain CSV (hand-craft / spreadsheet export)
 *   Write one note per line and use `parseCsvNotes()` from
 *   `src/composables/useBeatmapImporter.ts` to convert:
 *
 *   ```
 *   2500, up
 *   3000, down, 500
 *   3500, left
 *   4000, right
 *   ```
 *
 *   Paste directly into the notes array — no tools needed.
 *
 *   ### 5b. StepMania / DDR (.sm files)
 *   Use `parseSm()` from `useBeatmapImporter`:
 *   ```ts
 *   import { parseSm } from '../composables/useBeatmapImporter'
 *   const smText = await fetch('/path/to/song.sm').then(r => r.text())
 *   const notes = parseSm(smText, { bpmOverride: 140 })
 *   ```
 *   Supports dance-single 4-panel charts. Hold notes are approximated.
 *
 *   ### 5c. osu!mania (.osu files, 4K mode)
 *   Use `parseOsuMania()` from `useBeatmapImporter`:
 *   ```ts
 *   import { parseOsuMania } from '../composables/useBeatmapImporter'
 *   const osuRaw = await fetch('/path/to/song.osu').then(r => r.text())
 *   const notes = parseOsuMania(osuRaw)
 *   ```
 *   Columns are auto-detected from the .osu file's X coordinates.
 *   Hold notes (hold heads) are parsed with correct end times.
 *
 *   ### 5d. Tools & references
 *   - **Spreadsheet** → Google Sheets / Excel with `offsetMs + n * (60000 / bpm) * beatDivider`
 *   - **BPM finder** — https://songbpm.com / https://tunebat.com
 *   - **StepMania songs** — https://zenius-i-vanisher.com (StepMania / DDR packs)
 *   - **osu! beatmaps** — https://osu.ppy.sh/beatmapsets (filter: mania 4K)
 *   - **Future**: in-app import button + visual chart editor via community PR
 *
 * ## 6. Audio files
 *
 *   Place audio files under `public/assets/rhythm/<id>.mp3` (or .wav/.ogg).
 *   Set `audioSrc` to `'/assets/rhythm/<id>.mp3'` in the song entry.
 *
 *   The game engine will automatically:
 *   - Play audio when the song starts
 *   - Sync the game clock with `audio.currentTime` (more accurate than wall clock)
 *   - End the game when the audio finishes
 *   - Pause/resume audio with the game pause toggle
 *
 *   Songs without `audioSrc` can still be played "silent" for testing.
 *
 *   **Supported formats:** .mp3, .wav, .ogg, .flac (anything the browser's `<audio>` can play)
 *
 *   **Tips:**
 *   - Use MP3 @ 192kbps or higher for good quality
 *   - Clip the audio file to match the beatmap duration (trim silence)
 *   - Use tools like Audacity, FFmpeg, or online MP3 cutter to trim
 *
 * ### How to get audio for a song
 *
 *   1. Purchase / download the song legally (CD rip, digital purchase, etc.)
 *   2. Trim to the playable section (~2 min for most beatmaps)
 *   3. Export as MP3 (192 kbps, 44100 Hz stereo)
 *   4. Place at `public/assets/rhythm/<song-id>.mp3`
 *   5. Set `audioSrc: '/assets/rhythm/<song-id>.mp3'` in the song entry
 *
 *   **For 春日影 specifically:** You need to find/obtain the audio file
 *   (BanG Dream! game rip, CD purchase, etc.), trim it to ~118s keeping
 *   the full song structure, and place it at `public/assets/rhythm/haruhikage.mp3`.
 *
 * ──── WINDOW DIMS ─────────────────────────────────────────────────
 * Main-window size (logical px) while the rhythm game overlay is open.
 */

export interface Note {
  /** Time in ms from song start when this note reaches the judgment line. */
  time: number
  /** Arrow direction. */
  direction: 'up' | 'down' | 'left' | 'right'
  /** Hold duration in ms. 0 = tap note. >0 = hold note. */
  holdMs: number
}

export interface SongDifficulty {
  /** Difficulty identifier (for display and selection). */
  difficulty: 'easy' | 'normal' | 'hard'
  /** Overall difficulty rating (1–10). */
  rating: number
  /** Beatmap notes for this difficulty. */
  notes: Note[]
  /** Optional per-difficulty audio override (falls back to song's audioSrc). */
  audioSrc?: string
}

export interface RhythmSong {
  id: string
  title: string
  artist: string
  /** Beats per minute — drives the scroll speed. */
  bpm: number
  /** ms offset before the first note (intro silence / count-in). */
  offsetMs: number
  /** Path to audio file under public/assets/rhythm/. Empty string = no audio yet. */
  audioSrc: string
  /** Available difficulty levels. */
  difficulties: SongDifficulty[]
}

/**
 * Main-window size (logical px) while the rhythm game overlay is open,
 * per character size tier.
 * These are taller than normal pet dims to fit both the pet on top
 * and the game canvas at the bottom.
 */
export const RHYTHM_GAME_DIMS: Record<'small' | 'medium' | 'large', [number, number]> = {
  small:  [400, 460],
  medium: [480, 560],
  large:  [560, 660],
}

// ── Helper to build notes evenly across a duration ─────────────────
// Useful for prototyping before hand-crafted charts arrive.

/**
 * Generate evenly-spaced notes for prototyping.
 * Useful for testing before hand-crafted beatmaps arrive.
 */
export function generateBeatmap(
  startMs: number,
  endMs: number,
  intervalMs: number,
  directions: Array<Note['direction']> = ['left', 'down', 'up', 'right'],
): Note[] {
  const notes: Note[] = []
  let t = startMs
  let di = 0
  while (t < endMs) {
    notes.push({ time: t, direction: directions[di % directions.length], holdMs: 0 })
    di++
    t += intervalMs
  }
  return notes
}

import { HARUHIKAGE_BESTDORI_NOTES } from '../data/haruhikageChart'

// ── Preset songs ───────────────────────────────────────────────────
// Beatmaps below are hand-crafted for each song. Songs without audio
// can be played "silent" — the timing still works for testing.

export const PRESET_SONGS: RhythmSong[] = [
  {
    id: 'waltz-of-the-cucumber',
    title: '黄瓜 Waltz',
    artist: 'Mutsumi',
    bpm: 120,
    offsetMs: 2000,
    audioSrc: '',
    difficulties: [
      {
        difficulty: 'easy',
        rating: 2,
        notes: [
          // Intro: simple 4-beat pattern  (120 BPM → 500 ms per beat)
          { time: 2500, direction: 'up',    holdMs: 0 },
          { time: 3000, direction: 'down',  holdMs: 0 },
          { time: 3500, direction: 'left',  holdMs: 0 },
          { time: 4000, direction: 'right', holdMs: 0 },

          { time: 4500, direction: 'up',    holdMs: 0 },
          { time: 5000, direction: 'down',  holdMs: 0 },
          { time: 5500, direction: 'left',  holdMs: 0 },
          { time: 6000, direction: 'right', holdMs: 0 },

          // Verse: alternating pairs
          { time: 6500, direction: 'left',  holdMs: 0 },
          { time: 7000, direction: 'right', holdMs: 0 },
          { time: 7500, direction: 'left',  holdMs: 0 },
          { time: 8000, direction: 'right', holdMs: 0 },
          { time: 8500, direction: 'up',    holdMs: 0 },
          { time: 9000, direction: 'down',  holdMs: 0 },
          { time: 9500, direction: 'up',    holdMs: 0 },
          { time: 10000, direction: 'down', holdMs: 0 },

          // Chorus: faster 8th notes (250 ms)
          { time: 10500, direction: 'up',    holdMs: 0 },
          { time: 10750, direction: 'right', holdMs: 0 },
          { time: 11000, direction: 'down',  holdMs: 0 },
          { time: 11250, direction: 'left',  holdMs: 0 },
          { time: 11500, direction: 'up',    holdMs: 0 },
          { time: 11750, direction: 'right', holdMs: 0 },
          { time: 12000, direction: 'down',  holdMs: 0 },
          { time: 12250, direction: 'left',  holdMs: 0 },

          // Hold notes
          { time: 13000, direction: 'up',    holdMs: 1000 },
          { time: 14500, direction: 'down',  holdMs: 500  },
          { time: 15500, direction: 'left',  holdMs: 750  },
          { time: 16500, direction: 'right', holdMs: 1000 },

          // Finale: rapid pattern
          { time: 18000, direction: 'up',    holdMs: 0 },
          { time: 18200, direction: 'down',  holdMs: 0 },
          { time: 18400, direction: 'left',  holdMs: 0 },
          { time: 18600, direction: 'right', holdMs: 0 },
          { time: 18800, direction: 'up',    holdMs: 0 },
          { time: 19000, direction: 'down',  holdMs: 0 },
          { time: 19200, direction: 'left',  holdMs: 0 },
          { time: 19400, direction: 'right', holdMs: 0 },

          // Outro
          { time: 20000, direction: 'up',    holdMs: 1500 },
          { time: 22000, direction: 'down',  holdMs: 1500 },
        ],
      },
    ],
  },

  {
    id: 'midnight-guitar',
    title: '午夜吉他',
    artist: 'Mutsumi',
    bpm: 140,
    offsetMs: 1500,
    audioSrc: '',
    difficulties: [
      {
        difficulty: 'normal',
        rating: 5,
        notes: [
          // Intro (4 beats @ 140 BPM ≈ 428 ms per beat)
          { time: 2000,  direction: 'right', holdMs: 0 },
          { time: 2428,  direction: 'left',  holdMs: 0 },
          { time: 2856,  direction: 'up',    holdMs: 0 },
          { time: 3284,  direction: 'down',  holdMs: 0 },

          // Main riff: 8th notes (214 ms)
          { time: 4000,  direction: 'left',  holdMs: 0 },
          { time: 4214,  direction: 'right', holdMs: 0 },
          { time: 4428,  direction: 'left',  holdMs: 0 },
          { time: 4642,  direction: 'right', holdMs: 0 },
          { time: 4856,  direction: 'up',    holdMs: 0 },
          { time: 5070,  direction: 'down',  holdMs: 0 },
          { time: 5284,  direction: 'up',    holdMs: 0 },
          { time: 5498,  direction: 'down',  holdMs: 0 },

          // Syncopation
          { time: 6000,  direction: 'up',    holdMs: 0 },
          { time: 6428,  direction: 'left',  holdMs: 0 },
          { time: 6856,  direction: 'right', holdMs: 0 },
          { time: 7284,  direction: 'down',  holdMs: 0 },
          { time: 8000,  direction: 'up',    holdMs: 0 },
          { time: 8428,  direction: 'right', holdMs: 0 },
          { time: 8856,  direction: 'down',  holdMs: 0 },
          { time: 9284,  direction: 'left',  holdMs: 0 },

          // Bridge: alternating holds
          { time: 10000, direction: 'left',  holdMs: 800  },
          { time: 11000, direction: 'right', holdMs: 800  },
          { time: 12000, direction: 'up',    holdMs: 1200 },
          { time: 13500, direction: 'down',  holdMs: 600  },
          { time: 14500, direction: 'left',  holdMs: 400  },
          { time: 15000, direction: 'right', holdMs: 400  },

          // Finale burst
          { time: 16000, direction: 'up',    holdMs: 0 },
          { time: 16214, direction: 'down',  holdMs: 0 },
          { time: 16428, direction: 'left',  holdMs: 0 },
          { time: 16642, direction: 'right', holdMs: 0 },
          { time: 16856, direction: 'up',    holdMs: 0 },
          { time: 17070, direction: 'down',  holdMs: 0 },
          { time: 17284, direction: 'left',  holdMs: 0 },
          { time: 17498, direction: 'right', holdMs: 0 },

          // End
          { time: 18000, direction: 'up',    holdMs: 2000 },
        ],
      },
    ],
  },

  {
    id: 'haruhikage',
    title: '春日影',
    artist: 'CRYCHIC (BanG Dream!)',
    bpm: 97,
    offsetMs: 2000,
    audioSrc: '/assets/rhythm/haruhikage.flac',
    difficulties: [
      {
        difficulty: 'easy',
        rating: 3,
        notes: [
          { time: 2000, direction: 'left', holdMs: 0 },
          { time: 3237, direction: 'up', holdMs: 0 },
          { time: 4474, direction: 'left', holdMs: 0 },
          { time: 5711, direction: 'up', holdMs: 0 },
          { time: 6948, direction: 'left', holdMs: 0 },
          { time: 8186, direction: 'up', holdMs: 0 },
          { time: 9423, direction: 'left', holdMs: 0 },
          { time: 10000, direction: 'down', holdMs: 0 },
          { time: 11237, direction: 'up', holdMs: 0 },
          { time: 12474, direction: 'left', holdMs: 0 },
          { time: 13711, direction: 'down', holdMs: 0 },
          { time: 14948, direction: 'up', holdMs: 0 },
          { time: 16186, direction: 'left', holdMs: 0 },
          { time: 17423, direction: 'down', holdMs: 0 },
          { time: 18660, direction: 'up', holdMs: 0 },
          { time: 19897, direction: 'left', holdMs: 0 },
          { time: 21134, direction: 'down', holdMs: 0 },
          { time: 22371, direction: 'up', holdMs: 0 },
          { time: 23608, direction: 'left', holdMs: 0 },
          { time: 24845, direction: 'down', holdMs: 0 },
          { time: 26082, direction: 'up', holdMs: 0 },
          { time: 27320, direction: 'left', holdMs: 0 },
          { time: 28557, direction: 'down', holdMs: 0 },
          { time: 29794, direction: 'up', holdMs: 0 },
          { time: 30000, direction: 'left', holdMs: 0 },
          { time: 30928, direction: 'down', holdMs: 0 },
          { time: 31856, direction: 'up', holdMs: 0 },
          { time: 32784, direction: 'right', holdMs: 0 },
          { time: 33711, direction: 'left', holdMs: 0 },
          { time: 34639, direction: 'down', holdMs: 0 },
          { time: 35567, direction: 'up', holdMs: 0 },
          { time: 36495, direction: 'right', holdMs: 0 },
          { time: 37423, direction: 'left', holdMs: 0 },
          { time: 38351, direction: 'down', holdMs: 0 },
          { time: 39000, direction: 'left', holdMs: 0 },
          { time: 39773, direction: 'down', holdMs: 0 },
          { time: 40546, direction: 'up', holdMs: 0 },
          { time: 41320, direction: 'right', holdMs: 0 },
          { time: 42093, direction: 'down', holdMs: 0 },
          { time: 42866, direction: 'up', holdMs: 0 },
          { time: 43639, direction: 'right', holdMs: 0 },
          { time: 44412, direction: 'left', holdMs: 0 },
          { time: 45186, direction: 'left', holdMs: 0 },
          { time: 45959, direction: 'down', holdMs: 0 },
          { time: 46732, direction: 'up', holdMs: 0 },
          { time: 47505, direction: 'right', holdMs: 0 },
          { time: 48278, direction: 'down', holdMs: 0 },
          { time: 49052, direction: 'up', holdMs: 0 },
          { time: 49825, direction: 'right', holdMs: 0 },
          { time: 50598, direction: 'left', holdMs: 0 },
          { time: 51371, direction: 'left', holdMs: 0 },
          { time: 52144, direction: 'down', holdMs: 0 },
          { time: 52918, direction: 'up', holdMs: 0 },
          { time: 53691, direction: 'right', holdMs: 0 },
          { time: 54464, direction: 'down', holdMs: 0 },
          { time: 55237, direction: 'up', holdMs: 0 },
          { time: 56010, direction: 'right', holdMs: 0 },
          { time: 56784, direction: 'left', holdMs: 0 },
          { time: 57557, direction: 'left', holdMs: 0 },
          { time: 58330, direction: 'down', holdMs: 0 },
          { time: 59000, direction: 'down', holdMs: 0 },
          { time: 60237, direction: 'up', holdMs: 0 },
          { time: 61474, direction: 'left', holdMs: 0 },
          { time: 62711, direction: 'right', holdMs: 0 },
          { time: 63948, direction: 'down', holdMs: 0 },
          { time: 65186, direction: 'up', holdMs: 0 },
          { time: 66423, direction: 'left', holdMs: 0 },
          { time: 67660, direction: 'right', holdMs: 0 },
          { time: 68897, direction: 'down', holdMs: 0 },
          { time: 70134, direction: 'up', holdMs: 0 },
          { time: 71371, direction: 'left', holdMs: 0 },
          { time: 72608, direction: 'right', holdMs: 0 },
          { time: 73845, direction: 'down', holdMs: 0 },
          { time: 75082, direction: 'up', holdMs: 0 },
          { time: 76320, direction: 'left', holdMs: 0 },
          { time: 77000, direction: 'left', holdMs: 0 },
          { time: 77928, direction: 'down', holdMs: 0 },
          { time: 78856, direction: 'up', holdMs: 0 },
          { time: 79784, direction: 'right', holdMs: 0 },
          { time: 80711, direction: 'left', holdMs: 0 },
          { time: 81639, direction: 'down', holdMs: 0 },
          { time: 82567, direction: 'up', holdMs: 0 },
          { time: 83495, direction: 'right', holdMs: 0 },
          { time: 84423, direction: 'left', holdMs: 0 },
          { time: 85351, direction: 'down', holdMs: 0 },
          { time: 86000, direction: 'left', holdMs: 0 },
          { time: 86773, direction: 'down', holdMs: 0 },
          { time: 87546, direction: 'up', holdMs: 0 },
          { time: 88320, direction: 'right', holdMs: 0 },
          { time: 89093, direction: 'down', holdMs: 0 },
          { time: 89866, direction: 'up', holdMs: 0 },
          { time: 90639, direction: 'right', holdMs: 0 },
          { time: 91412, direction: 'left', holdMs: 0 },
          { time: 92186, direction: 'left', holdMs: 0 },
          { time: 92959, direction: 'down', holdMs: 0 },
          { time: 93732, direction: 'up', holdMs: 0 },
          { time: 94505, direction: 'right', holdMs: 0 },
          { time: 95278, direction: 'down', holdMs: 0 },
          { time: 96052, direction: 'up', holdMs: 0 },
          { time: 96825, direction: 'right', holdMs: 0 },
          { time: 97598, direction: 'left', holdMs: 0 },
          { time: 98371, direction: 'left', holdMs: 0 },
          { time: 99144, direction: 'down', holdMs: 0 },
          { time: 99918, direction: 'up', holdMs: 0 },
          { time: 100691, direction: 'right', holdMs: 0 },
          { time: 101464, direction: 'down', holdMs: 0 },
          { time: 102237, direction: 'up', holdMs: 0 },
          { time: 103010, direction: 'right', holdMs: 0 },
          { time: 103784, direction: 'left', holdMs: 0 },
          { time: 104557, direction: 'left', holdMs: 0 },
          { time: 105330, direction: 'down', holdMs: 0 },
          { time: 106000, direction: 'down', holdMs: 0 },
          { time: 107856, direction: 'up', holdMs: 0 },
          { time: 109711, direction: 'down', holdMs: 0 },
          { time: 111567, direction: 'up', holdMs: 0 },
          { time: 113423, direction: 'down', holdMs: 0 },
          { time: 115278, direction: 'up', holdMs: 0 },
          { time: 117134, direction: 'down', holdMs: 0 },
        ],
      },
      {
        difficulty: 'normal',
        rating: 5,
        notes: (() => {
          // Normal: 8th & quarter notes with holds, BPM 97
          const beat = 60000 / 97
          const halfBeat = beat / 2
          const n: Note[] = []
          const dirs: Array<Note['direction']> = ['left', 'down', 'up', 'right']
          let t = 2000
          let di = 0
          while (t < 117000) {
            // Every 4 notes, insert a hold note
            if (di > 0 && di % 4 === 0) {
              const holdDir = dirs[di % dirs.length]
              n.push({ time: t, direction: holdDir, holdMs: beat })
              t += halfBeat
              // Fill the rest of the beat with a light tap
              n.push({ time: t, direction: dirs[(di + 1) % dirs.length], holdMs: 0 })
              t += halfBeat
            } else {
              n.push({ time: t, direction: dirs[di % dirs.length], holdMs: 0 })
              t += beat
            }
            di++
          }
          return n
        })(),
      },
      {
        difficulty: 'hard',
        rating: 9,
        // Bestdori community chart (level 27, post #162425)
        // https://bestdori.com/community/charts/162425
        notes: HARUHIKAGE_BESTDORI_NOTES,
      },
    ],
  },

  {
    id: 'cucumber-express',
    title: '黄瓜特快',
    artist: 'Mutsumi',
    bpm: 160,
    offsetMs: 1000,
    audioSrc: '',
    difficulties: [
      {
        difficulty: 'hard',
        rating: 8,
        notes: [
          // Fast intro (160 BPM → 375 ms per beat, 187.5 ms 8th)
          { time: 1500, direction: 'down',  holdMs: 0 },
          { time: 1875, direction: 'up',    holdMs: 0 },

          // Stream
          { time: 2500, direction: 'left',  holdMs: 0 },
          { time: 2687, direction: 'right', holdMs: 0 },
          { time: 2875, direction: 'left',  holdMs: 0 },
          { time: 3062, direction: 'right', holdMs: 0 },
          { time: 3250, direction: 'down',  holdMs: 0 },
          { time: 3437, direction: 'up',    holdMs: 0 },
          { time: 3625, direction: 'down',  holdMs: 0 },
          { time: 3812, direction: 'up',    holdMs: 0 },

          // Jackhammer
          { time: 4500, direction: 'left',  holdMs: 0 },
          { time: 4687, direction: 'left',  holdMs: 0 },
          { time: 4875, direction: 'left',  holdMs: 0 },
          { time: 5062, direction: 'right', holdMs: 0 },
          { time: 5250, direction: 'right', holdMs: 0 },
          { time: 5437, direction: 'right', holdMs: 0 },

          // Mixed
          { time: 6000, direction: 'up',    holdMs: 0 },
          { time: 6187, direction: 'down',  holdMs: 0 },
          { time: 6375, direction: 'left',  holdMs: 0 },
          { time: 6562, direction: 'right', holdMs: 0 },
          { time: 6750, direction: 'up',    holdMs: 500 },
          { time: 7125, direction: 'down',  holdMs: 500 },
          { time: 7500, direction: 'left',  holdMs: 500 },

          // Hard holds + taps
          { time: 8500, direction: 'right', holdMs: 1000 },
          { time: 9500, direction: 'up',    holdMs: 0 },
          { time: 9687, direction: 'down',  holdMs: 0 },
          { time: 9875, direction: 'left',  holdMs: 0 },
          { time: 10062, direction: 'right', holdMs: 0 },
          { time: 10500, direction: 'up',    holdMs: 1500 },

          // Break
          { time: 12500, direction: 'down',  holdMs: 1000 },
          { time: 14000, direction: 'left',  holdMs: 750  },
          { time: 15000, direction: 'right', holdMs: 750  },

          // Final rush — 16th notes
          { time: 16000, direction: 'up',    holdMs: 0 },
          { time: 16093, direction: 'down',  holdMs: 0 },
          { time: 16187, direction: 'left',  holdMs: 0 },
          { time: 16281, direction: 'right', holdMs: 0 },
          { time: 16375, direction: 'up',    holdMs: 0 },
          { time: 16468, direction: 'down',  holdMs: 0 },
          { time: 16562, direction: 'left',  holdMs: 0 },
          { time: 16656, direction: 'right', holdMs: 0 },
          { time: 16750, direction: 'up',    holdMs: 0 },
          { time: 16843, direction: 'down',  holdMs: 0 },
          { time: 16937, direction: 'left',  holdMs: 0 },
          { time: 17031, direction: 'right', holdMs: 0 },

          // Outro — long hold
          { time: 17500, direction: 'up',    holdMs: 2000 },
          { time: 20000, direction: 'down',  holdMs: 2000 },
        ],
      },
    ],
  },
]