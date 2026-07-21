/**
 * generateBeatmap — procedural beatmap generator.
 *
 * Creates a playable 4-direction beatmap from BPM / duration / difficulty.
 * Useful for quickly adding preset songs without hand-crafting every note.
 *
 * The generator divides the track into sections (intro → verses → choruses → outro)
 * and places notes on a quantized grid (quarter / eighth / sixteenth notes).
 * Directions cycle in natural-feeling patterns (LRUD, diagonal pairs, etc.).
 */

import type { Note } from '../config/rhythmSongs'

export interface GenOptions {
  /** BPM of the song */
  bpm: number
  /** ms before the first note */
  offsetMs: number
  /** total duration of the playable section in ms */
  durationMs: number
  /** 'easy' | 'normal' | 'hard' */
  difficulty: 'easy' | 'normal' | 'hard'
}

/**
 * Song section descriptors for structural variety.
 * Each section has a relative duration multiplier and a fill density.
 */
interface Section {
  label: string
  /** fraction of total play time */
  timeFraction: number
  /** 0–1 density multiplier vs the base density */
  density: number
}

// ── Default section layout (common J-Pop 2-min structure) ─────────
const SECTIONS: Section[] = [
  { label: 'intro',     timeFraction: 0.08, density: 0.25 },
  { label: 'verse1',    timeFraction: 0.17, density: 0.55 },
  { label: 'pre-chorus',timeFraction: 0.08, density: 0.70 },
  { label: 'chorus1',   timeFraction: 0.17, density: 1.00 },
  { label: 'verse2',    timeFraction: 0.15, density: 0.60 },
  { label: 'pre-chorus2',timeFraction: 0.08, density: 0.75 },
  { label: 'chorus2',   timeFraction: 0.17, density: 1.00 },
  { label: 'outro',     timeFraction: 0.10, density: 0.30 },
]

const DIRECTIONS: Note['direction'][] = ['left', 'down', 'up', 'right']

/**
 * Generate a procedural beatmap.
 */
export function generateBeatmap(opts: GenOptions): Note[] {
  const { bpm, offsetMs, durationMs, difficulty } = opts
  const beatMs = 60000 / bpm

  // Base grid resolution (fraction of a beat)
  const gridFraction = difficulty === 'easy' ? 2   // quarter notes
    : difficulty === 'normal' ? 4   // eighth notes
    : 8 // sixteenth notes
  const gridMs = beatMs / (gridFraction / 2) // gridFraction=2 → gridMs=beatMs

  // Base fill probability per grid slot
  const baseFill = difficulty === 'easy' ? 0.35
    : difficulty === 'normal' ? 0.55
    : 0.75

  const notes: Note[] = []
  let dirIdx = 0
  let playableMs = durationMs
  let sectionStartMs = 0

  for (const section of SECTIONS) {
    const sectionDur = playableMs * section.timeFraction
    const sectionEnd = sectionStartMs + sectionDur
    const density = Math.min(1, baseFill * section.density * 1.2)

    // Walk through the grid slots in this section
    let t = sectionStartMs
    while (t < sectionEnd) {
      // Quantize to grid
      const quantized = Math.round(t / gridMs) * gridMs
      if (quantized < sectionStartMs) {
        t += gridMs
        continue
      }

      // Direction pattern: cycle with occasional repetition for musicality
      const dice = Math.random()
      if (dice < density) {
        const direction = DIRECTIONS[dirIdx % 4]
        dirIdx++

        // Hold notes: ~10% chance on easy, ~15% on normal/hard
        let holdMs = 0
        const holdChance = difficulty === 'easy' ? 0.10 : 0.15
        if (Math.random() < holdChance) {
          holdMs = beatMs * (0.5 + Math.random() * 1.5) // 0.5–2 beats
        }

        notes.push({
          time: offsetMs + Math.round(quantized),
          direction,
          holdMs,
        })
      }

      t += gridMs
    }

    sectionStartMs = sectionEnd
  }

  return notes
}

/**
 * Generate the 春日影 Easy beatmap with deterministic seed for reproducibility.
 * Uses the known data from BanG Dream! wiki: BPM 97, ~115 notes, ~118s.
 */
export function generateHaruhikageEasy(): Note[] {
  // Use a simple deterministic approach (no randomness)
  const BPM = 97
  const beatMs = 60000 / BPM // ≈ 618.56 ms
  const offsetMs = 2000
  const playableMs = 116000 // ~116s from 118s total minus offset
  const notes: Note[] = []

  // Section layout (timings in ms from offset)
  const sections: { startMs: number; endMs: number; noteGap: number; pattern: number[] }[] = [
    // Intro: sparse, half notes
    { startMs: 0,      endMs: 9000,  noteGap: beatMs * 2,  pattern: [0, 2] },
    // Verse 1: quarter notes, mostly down/up
    { startMs: 9000,   endMs: 29000, noteGap: beatMs,      pattern: [1, 2, 1, 2] },
    // Pre-chorus: quarter + eighth alternating
    { startMs: 29000,  endMs: 38000, noteGap: beatMs * 0.75, pattern: [0, 1, 2, 3] },
    // Chorus 1: mostly quarter, all directions
    { startMs: 38000,  endMs: 58000, noteGap: beatMs * 0.75, pattern: [0, 1, 2, 3, 1, 2, 3, 0] },
    // Verse 2
    { startMs: 58000,  endMs: 76000, noteGap: beatMs,      pattern: [1, 2, 0, 3] },
    // Pre-chorus 2
    { startMs: 76000,  endMs: 85000, noteGap: beatMs * 0.75, pattern: [0, 1, 2, 3] },
    // Chorus 2 (climax)
    { startMs: 85000,  endMs: 105000, noteGap: beatMs * 0.75, pattern: [0, 1, 2, 3, 1, 2, 3, 0] },
    // Outro: fading
    { startMs: 105000, endMs: 116000, noteGap: beatMs * 1.5, pattern: [1, 2, 1] },
  ]

  for (const sec of sections) {
    let t = sec.startMs
    let patIdx = 0
    while (t < sec.endMs) {
      const dirIdx = sec.pattern[patIdx % sec.pattern.length]
      notes.push({
        time: offsetMs + Math.round(t),
        direction: DIRECTIONS[dirIdx],
        holdMs: 0,
      })
      patIdx++
      t += sec.noteGap
    }
  }

  return notes
}