/**
 * useBeatmapImporter — convert beatmaps from other rhythm games
 * into Mutsumi's internal Note[] format.
 *
 * Supported input formats:
 *   - StepMania / DDR      (.sm)  4-panel dance-single
 *   - osu!mania             (.osu) 4K mode
 *   - Plain CSV / TSV       (time,direction)  (simplest for hand-crafting)
 *
 * Usage:
 *   import { parseSm, parseOsuMania, parseCsvNotes } from './useBeatmapImporter'
 *
 *   const notes = parseSm(rawSmText)
 *   const song  = { id: 'imported', title: '...', bpm: 120, offsetMs: 2000,
 *                   difficulty: 'normal', rating: 3, notes }
 */

import type { Note } from '../config/rhythmSongs'

const DIR_MAP_4K: Record<number, Note['direction']> = {
  0: 'left',
  1: 'down',
  2: 'up',
  3: 'right',
}

// ── Helpers ───────────────────────────────────────────────────────

/** Grab the first value of a header tag, e.g. #TITLE:foo; → "foo" */
function smHeader(text: string, tag: string): string {
  const m = text.match(new RegExp(`#${tag}:(.+?);`))
  return m ? m[1].trim() : ''
}

/** Parse BPMs from #BPMS:0.000=140.000; or #BPMS:0.000=140.000,4.000=160.000; */
function smBpm(text: string): number {
  const m = text.match(/#BPMS:(.+?);/)
  if (!m) return 120
  const first = m[1].split(',')[0] // take first BPM entry
  const val = first.split('=')[1]
  return val ? parseFloat(val) : 120
}

/**
 * Parse a StepMania .sm / .ssc chart (dance-single, 4 panels).
 *
 * The measure-based notation uses rows of 4+ digits inside
 *   #NOTES:
 *       dance-single:
 *       :
 *       :
 *       :
 *       0000
 *       1000
 *       …
 *       ,
 *   ;
 *
 * Each row is a beat fraction: 4 rows = quarter notes, 8 = eighth, etc.
 */
export function parseSm(text: string, options?: {
  bpmOverride?: number
  offsetMsOverride?: number
}): Note[] {
  // ── Parse BPM & offset ───────────────────────────────────────
  const bpm = options?.bpmOverride ?? smBpm(text)
  const offsetMs = options?.offsetMsOverride ?? 2000
  const beatMs = 60000 / bpm

  // ── Locate chart body after #NOTES: ─────────────────────────
  // Find the first dance-single chart
  const chartStart = text.match(/#NOTES:\s*\n\s*dance-single:/i)
  if (!chartStart) return []

  const body = text.slice((chartStart.index ?? 0) + chartStart[0].length)

  // ── Parse measure blocks ─────────────────────────────────────
  // After the header lines (4 colons), notes are rows of digits
  // ending with a `,;` or just `;`
  const colonSkip = body.split('\n')
  let colonCount = 0
  let startLine = -1
  for (let i = 0; i < colonSkip.length; i++) {
    if (colonSkip[i].trim() === ':') colonCount++
    if (colonCount >= 4) { startLine = i + 1; break }
  }
  if (startLine < 0) return []

  const chartLines = colonSkip.slice(startLine)
  const notes: Note[] = []
  let measureTime = 0   // accumulated time in ms
  let measureRowCount = 0

  for (const rawLine of chartLines) {
    const line = rawLine.trim()

    // End of chart
    if (line === ';' || line.startsWith('//')) break

    // End of measure → finalize
    if (line === ',') {
      measureTime += beatMs * 4  // 4 beats per measure in 4/4 time
      measureRowCount = 0
      continue
    }

    // Skip non-digit rows
    if (!/^[\d0123ML]+$/.test(line)) continue

    measureRowCount++
    const rowTime = measureTime + (beatMs * 4 / line.length) * (measureRowCount - 0.5)

    for (let col = 0; col < line.length && col < 4; col++) {
      const ch = line[col]
      if (ch === '0' || ch === 'M') continue

      const direction = DIR_MAP_4K[col]
      let holdMs = 0

      if (ch === '2' || ch === '3') {
        // Hold note — find end (next row with '2' or '3' in same column,
        // or a row with '0')
        // For simplicity, give a default hold length
        holdMs = beatMs // one beat hold
      }

      notes.push({ time: offsetMs + rowTime, direction, holdMs })
    }
  }

  return notes
}

// ── osu!mania parser ──────────────────────────────────────────────

/**
 * Parse a .osu file (osu!mania 4K mode).
 *
 * In osu!mania .osu files:
 *   [HitObjects]
 *   256,192,1000,1,0,0:0:0:0:         ← column 0 (left) at t=1000
 *   256,192,1500,1,0,128:0:0:0:       ← column 2 (up) at t=1500, hold end=1628
 *
 * Format: x,y,time,type,hitSound,extras
 *   - type: 1 = tap, 128 = hold head
 *   - For 4K: column = Math.floor(x / (512/4))
 *   - Hold end time: extras first part (holds only)
 */
export function parseOsuMania(text: string, options?: {
  bpmOverride?: number
  offsetMsOverride?: number
}): Note[] {
  const bpm = options?.bpmOverride ?? 120
  const offsetMs = options?.offsetMsOverride ?? 2000
  const notes: Note[] = []

  // Find [HitObjects] section
  const hoMatch = text.match(/\[HitObjects\]([\s\S]*)/)
  if (!hoMatch) return []

  const lines = hoMatch[1].split('\n')

  for (const line of lines) {
    const t = line.trim()
    if (!t || t.startsWith('//')) continue

    const parts = t.split(',')
    if (parts.length < 5) continue

    const x = parseFloat(parts[0])
    const y = parseFloat(parts[1]) // not used in mania
    const time = parseFloat(parts[2])
    const type = parseInt(parts[3], 10)
    const hitSound = parts[4] // not used

    if (isNaN(time)) continue

    // For mania mode, column = floor(x / (512 / columns))
    // Default to 4 columns
    const col = Math.floor(x / (512 / 4))
    const direction = DIR_MAP_4K[Math.min(col, 3)]

    let holdMs = 0
    // Check for hold note (type includes 128 = hold head)
    if (type & 128) {
      // Extras format: 0:0:0:0: for tap, or endTime:0:0:0: for hold
      // The extras part starts after hitSound (index 5)
      const extras = parts.slice(5).join(',')
      const endMatch = extras.match(/(\d+):/)
      if (endMatch) {
        const endTime = parseFloat(endMatch[1])
        if (!isNaN(endTime) && endTime > time) {
          holdMs = endTime - time
        }
      }
    }

    notes.push({ time: offsetMs + time, direction, holdMs })
  }

  return notes
}

// ── Plain CSV/TSV parser (biàn yú shǒu xiě) ──────────────────────

/**
 * Parse plain text notes — easiest format for hand-crafting.
 *
 * Format (one note per line):
 *   timeMs, direction
 *   timeMs, direction, holdMs
 *
 * Example:
 *   2500, up
 *   3000, down, 500
 *   3500, left
 *
 * direction: up | down | left | right
 */
export function parseCsvNotes(text: string): Note[] {
  const notes: Note[] = []
  for (const rawLine of text.split('\n')) {
    const line = rawLine.trim()
    if (!line || line.startsWith('#')) continue
    const parts = line.split(/[,;\t]+/)
    if (parts.length < 2) continue

    const time = parseFloat(parts[0])
    const dir = parts[1].trim().toLowerCase() as Note['direction']
    if (isNaN(time) || !['up', 'down', 'left', 'right'].includes(dir)) continue

    const holdMs = parts.length >= 3 ? parseFloat(parts[2]) || 0 : 0
    notes.push({ time, direction: dir, holdMs })
  }
  return notes
}