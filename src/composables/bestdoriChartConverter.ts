/**
 * bestdoriChartConverter.ts
 *
 * Converts Bestdori community chart data (beat/lane format) into
 * the project's Note[] format (time/direction/holdMs).
 *
 * Usage:
 *   import { convertBestdoriChart } from '@/composables/bestdoriChartConverter'
 *   const notes = convertBestdoriChart(bestdoriChartData, { bpm: 194, offsetMs: 2000 })
 *
 * Bestdori note types:
 *   - Single         → regular tap
 *   - Directional    → directional tap (with direction + width)
 *   - Slide           → slide with connection points → start + end as regular taps
 *   - BPM             → BPM change events (ignored, use source BPM)
 *
 * Lane mapping (7-lane Bestdori → 4-direction):
 *   lanes 0,1 → left
 *   lane 2    → down
 *   lane 3    → up
 *   lanes 4,5,6 → right
 */

export interface BestdoriChartNote {
  type?: 'Single' | 'Directional' | 'Slide' | 'BPM'
  beat?: number
  lane?: number
  width?: number
  direction?: string
  flick?: boolean
  connections?: Array<{ beat: number; lane: number; flick?: boolean; hidden?: boolean }>
  bpm?: number
}

export interface BestdoriConverterOptions {
  /** BPM used for beat→ms conversion */
  bpm: number
  /** Offset in ms added to all timestamps */
  offsetMs: number
}

export interface Note {
  time: number
  direction: 'left' | 'down' | 'up' | 'right'
  holdMs: number
}

const LANE_TO_DIR: Record<number, Note['direction']> = {
  0: 'left',
  1: 'left',
  2: 'down',
  3: 'up',
  4: 'right',
  5: 'right',
  6: 'right',
}

function beatToMs(beat: number, bpm: number, offsetMs: number): number {
  return Math.round(beat * (60000 / bpm) + offsetMs)
}

function nearestLane(laneVal: number): number {
  return Math.max(0, Math.min(6, Math.round(laneVal)))
}

function laneToDir(laneVal: number): Note['direction'] {
  return LANE_TO_DIR[nearestLane(laneVal)] ?? 'right'
}

export function convertBestdoriChart(
  chartEntries: BestdoriChartNote[],
  options: BestdoriConverterOptions,
): Note[] {
  const { bpm, offsetMs } = options
  const notes: Note[] = []

  for (const entry of chartEntries) {
    const type = entry.type ?? 'Single'

    if (type === 'BPM') continue

    if (type === 'Single' || type === 'Directional') {
      const beat = entry.beat!
      const lane = entry.lane!
      notes.push({
        time: beatToMs(beat, bpm, offsetMs),
        direction: laneToDir(lane),
        holdMs: 0,
      })
    }

    if (type === 'Slide') {
      const conns = entry.connections ?? []
      if (conns.length === 0) continue

      // Start point
      const start = conns[0]
      notes.push({
        time: beatToMs(start.beat, bpm, offsetMs),
        direction: laneToDir(start.lane),
        holdMs: 0,
      })

      // End point (only if time diff > 50ms)
      if (conns.length > 1) {
        const end = conns[conns.length - 1]
        const startMs = beatToMs(start.beat, bpm, offsetMs)
        const endMs = beatToMs(end.beat, bpm, offsetMs)
        if (endMs - startMs > 50) {
          notes.push({
            time: endMs,
            direction: laneToDir(end.lane),
            holdMs: 0,
          })
        }
      }
    }
  }

  notes.sort((a, b) => a.time - b.time)
  return notes
}