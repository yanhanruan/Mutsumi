/**
 * useRhythmGame — lightweight QQ炫舞-style rhythm game engine.
 *
 * Pure state machine (no DOM/Canvas dependencies). Handles:
 *   - Song + beatmap loading
 *   - Timing and note judgment (PERFECT / GOOD / OK / MISS)
 *   - Score + COMBO tracking
 *   - Input key processing
 *
 * The caller (RhythmGame.vue) owns the Canvas rendering and keyboard events;
 * this composable owns the game rules.
 */

import { ref, computed, readonly } from 'vue'
import type { RhythmSong, SongDifficulty, Note } from '../config/rhythmSongs'

// ── Judgment windows (ms) ──────────────────────────────────────────

export const JUDGMENT = {
  PERFECT: { max: 200, score: 300, label: 'PERFECT', color: '#FFD700' },
  GOOD:    { max: 440, score: 200, label: 'GOOD',    color: '#32CD32' },
  OK:      { max: 720, score: 100, label: 'OK',      color: '#87CEEB' },
  MISS:    { max: 1000, score: 0,  label: 'MISS',    color: '#FF4444' },
} as const

export type JudgmentKey = keyof typeof JUDGMENT

export interface JudgmentResult {
  key: JudgmentKey
  score: number
  color: string
  label: string
}

export type GamePhase = 'idle' | 'selecting' | 'playing' | 'paused' | 'ended'
export type DanceIntensity = 'none' | 'normal' | 'excited' | 'combo_boost'

export interface SongSelectOption {
  id: string
  title: string
  artist: string
  difficulty: string
  rating: number
  bpm: number
}

// ── Composable ─────────────────────────────────────────────────────

export function useRhythmGame() {
  // ── State ───────────────────────────────────────────────────────
  const phase        = ref<GamePhase>('idle')
  const song         = ref<RhythmSong | null>(null)
  const currentDifficulty = ref<SongDifficulty | null>(null)
  const score        = ref(0)
  /** Raw score WITHOUT combo bonuses, used for accuracy calculation. */
  const rawScore     = ref(0)
  const combo        = ref(0)
  const maxCombo     = ref(0)
  const totalNotes   = ref(0)
  const judgedNotes  = ref(0)
  const missedNotes  = ref(0)
  // Stats per judgment type
  const perfectCount = ref(0)
  const goodCount    = ref(0)
  const okCount      = ref(0)
  const missCount    = ref(0)
  // Last judgment for UI flash feedback
  const lastJudgment = ref<JudgmentResult | null>(null)
  // Current dance intensity (for pet animation)
  const danceIntensity = ref<DanceIntensity>('none')
  // Elapsed time in ms
  const elapsedMs = ref(0)

  // Internal state (not reactive — touched on every RAF frame)
  let sortedNotes: Note[] = []
  let noteIndex = 0
  let startTime = 0
  let animFrameId = 0
  let songDurationMs = 0
  // Tracks recently judged notes so we don't double-judge them
  let judgedSet = new Set<Note>()
  // Callbacks
  let onPhaseChange: ((p: GamePhase) => void) | null = null
  let onJudgment: ((j: JudgmentResult, note: Note) => void) | null = null

  // Audio
  let currentAudio: HTMLAudioElement | null = null

  // ── Computed ────────────────────────────────────────────────────

  const accuracy = computed(() => {
    if (totalNotes.value === 0) return 100
    const maxPossible = totalNotes.value * JUDGMENT.PERFECT.score
    const actual = rawScore.value
    return Math.round((actual / maxPossible) * 100)
  })

  const grade = computed(() => {
    const a = accuracy.value
    if (a >= 95) return 'SS'
    if (a >= 85) return 'S'
    if (a >= 70) return 'A'
    if (a >= 55) return 'B'
    if (a >= 40) return 'C'
    return 'D'
  })

  const progress = computed(() => {
    if (totalNotes.value === 0) return 0
    return Math.round((judgedNotes.value / totalNotes.value) * 100)
  })

  // ── Public API ──────────────────────────────────────────────────

  /** Start the song selection phase. */
  function enterSelection() {
    reset()
    phase.value = 'selecting'
    onPhaseChange?.('selecting')
  }

  /** Load a song and begin gameplay. */
  function startSong(s: RhythmSong, diff: SongDifficulty, _onJudgment?: (j: JudgmentResult, note: Note) => void) {
    reset()
    song.value = s
    currentDifficulty.value = diff
    // Sort notes by time
    sortedNotes = [...diff.notes].sort((a, b) => a.time - b.time)
    totalNotes.value = sortedNotes.length
    noteIndex = 0
    startTime = performance.now()
    phase.value = 'playing'
    danceIntensity.value = 'normal'
    onJudgment = _onJudgment ?? null

    // Calculate approximate song duration (last note + 1s)
    if (sortedNotes.length > 0) {
      const last = sortedNotes[sortedNotes.length - 1]
      songDurationMs = last.time + (last.holdMs || 0) + 1000
    } else {
      songDurationMs = s.offsetMs + 5000
    }

    // Start audio if available
    if (s.audioSrc) {
      const audio = new Audio(s.audioSrc)
      audio.preload = 'auto'
      audio.onloadedmetadata = () => {
        const audioDurMs = audio.duration * 1000
        if (audioDurMs > 0) {
          // Use audio duration only if it's SHORTER than the beatmap
          // (e.g. a short audio snippet); if audio is longer, keep beatmap end.
          songDurationMs = Math.min(songDurationMs, Math.round(audioDurMs))
        }
      }
      audio.onended = () => {
        // Audio finished naturally — end the game
        if (phase.value === 'playing') {
          endGame()
        }
      }
      audio.onerror = () => {
        console.warn('[useRhythmGame] Failed to load audio:', s.audioSrc)
        currentAudio = null
      }
      audio.play().catch((e: Error) => {
        console.warn('[useRhythmGame] Audio play failed:', e.message)
        currentAudio = null
      })
      currentAudio = audio
    }

    // Restart the RAF loop — reset() above cancelled it.
    startLoop()

    onPhaseChange?.('playing')
  }

  /** Pause / resume the game. */
  function togglePause() {
    if (phase.value === 'playing') {
      phase.value = 'paused'
      currentAudio?.pause()
      // Remember the elapsed time so we can resume correctly
      onPhaseChange?.('paused')
    } else if (phase.value === 'paused') {
      phase.value = 'playing'
      startTime = performance.now() - elapsedMs.value
      // Resume audio if available
      if (currentAudio) {
        currentAudio.play().catch((e: Error) => {
          console.warn('[useRhythmGame] Audio resume failed:', e.message)
        })
      }
      onPhaseChange?.('playing')
    }
  }

  /** End current game and return to selection. */
  function endGame() {
    phase.value = 'ended'
    danceIntensity.value = 'none'
    // Fade out audio then pause
    if (currentAudio) {
      const fadeStep = 50
      const fadeDuration = 800
      const steps = fadeDuration / fadeStep
      const initialVol = currentAudio.volume || 1
      const volStep = initialVol / steps
      const fadeInterval = setInterval(() => {
        if (currentAudio && currentAudio.volume > volStep) {
          currentAudio.volume = Math.max(0, currentAudio.volume - volStep)
        } else {
          clearInterval(fadeInterval)
          if (currentAudio) {
            currentAudio.pause()
            currentAudio.volume = initialVol // restore for next play
          }
        }
      }, fadeStep)
      // Still release reference after fade
      setTimeout(() => {
        if (currentAudio) {
          currentAudio.pause()
          currentAudio.volume = initialVol
          currentAudio = null
        }
      }, fadeDuration + 100)
    }
    onPhaseChange?.('ended')
  }

  /** Return to idle. */
  function exit() {
    reset()
    phase.value = 'idle'
    onPhaseChange?.('idle')
  }

  // ── Game tick (called from the Canvas RAF loop) ─────────────────
  function tick(now: number) {
    if (phase.value !== 'playing' && phase.value !== 'paused') {
      animFrameId = requestAnimationFrame(tick)
      return
    }

    // Use audio clock when available (more accurate than wall clock),
    // otherwise fall back to performance.now() delta.
    if (currentAudio && currentAudio.readyState >= 1) {
      elapsedMs.value = currentAudio.currentTime * 1000
    } else {
      elapsedMs.value = now - startTime
    }

    if (phase.value === 'paused') {
      animFrameId = requestAnimationFrame(tick)
      return
    }

    const currentTime = elapsedMs.value

    // Check for misses (notes that passed the judgment line without being hit)
    const missThreshold = JUDGMENT.MISS.max
    while (noteIndex < sortedNotes.length) {
      const note = sortedNotes[noteIndex]
      if (currentTime - note.time > missThreshold) {
        if (!judgedSet.has(note)) {
          recordMiss(note)
        }
        noteIndex++
      } else {
        break
      }
    }

    // Check game end
    if (judgedSet.size >= sortedNotes.length && currentTime > songDurationMs) {
      endGame()
      animFrameId = requestAnimationFrame(tick)
      return
    }

    animFrameId = requestAnimationFrame(tick)
  }

  /** Process a key press. Called from keyboard event handler. */
  function pressKey(direction: Note['direction']) {
    if (phase.value !== 'playing') return

    const currentTime = elapsedMs.value
    let bestNote: Note | null = null
    let bestDiff = Infinity

    // Find the closest note in this direction within the OK window
    for (let i = 0; i < sortedNotes.length; i++) {
      const note = sortedNotes[i]
      if (judgedSet.has(note)) continue
      if (note.direction !== direction) continue

      const diff = Math.abs(currentTime - note.time)
      if (diff <= JUDGMENT.MISS.max) {
        if (diff < bestDiff) {
          bestDiff = diff
          bestNote = note
        }
      } else if (currentTime < note.time) {
        // Future notes — stop searching (notes are sorted by time)
        break
      }
    }

    if (!bestNote) return

    // Determine judgment
    let jk: JudgmentKey
    if (bestDiff <= JUDGMENT.PERFECT.max) jk = 'PERFECT'
    else if (bestDiff <= JUDGMENT.GOOD.max) jk = 'GOOD'
    else if (bestDiff <= JUDGMENT.OK.max) jk = 'OK'
    else jk = 'MISS'

    judgeNote(bestNote, jk)
  }

  /** Handle hold-note release. */
  function releaseKey(_direction: Note['direction']) {
    // For now, hold notes just need to be pressed in time.
    // TODO: implement hold duration tracking for scoring.
  }

  // ── Internal ────────────────────────────────────────────────────

  function judgeNote(note: Note, jk: JudgmentKey) {
    if (judgedSet.has(note)) return
    judgedSet.add(note)
    judgedNotes.value++

    const j = JUDGMENT[jk]
    const result: JudgmentResult = { key: jk, score: j.score, color: j.color, label: j.label }

    if (jk === 'MISS') {
      combo.value = 0
      missCount.value++
      missedNotes.value++
      danceIntensity.value = 'none'
    } else {
      combo.value++
      if (combo.value > maxCombo.value) maxCombo.value = combo.value
      rawScore.value += j.score
      score.value += j.score
      // COMBO bonus: +50 per 10 combo (doesn't affect accuracy)
      if (combo.value >= 10 && combo.value % 10 === 0) {
        score.value += 50
      }

      if (jk === 'PERFECT') perfectCount.value++
      else if (jk === 'GOOD') goodCount.value++
      else if (jk === 'OK') okCount.value++

      // Dance intensity based on judgment quality
      danceIntensity.value = jk === 'PERFECT'
        ? (combo.value >= 20 ? 'combo_boost' : 'excited')
        : 'normal'
    }

    lastJudgment.value = result
    onJudgment?.(result, note)
  }

  function recordMiss(note: Note) {
    judgeNote(note, 'MISS')
  }

  function reset() {
    score.value = 0
    rawScore.value = 0
    combo.value = 0
    maxCombo.value = 0
    totalNotes.value = 0
    judgedNotes.value = 0
    missedNotes.value = 0
    perfectCount.value = 0
    goodCount.value = 0
    okCount.value = 0
    missCount.value = 0
    elapsedMs.value = 0
    lastJudgment.value = null
    danceIntensity.value = 'none'
    sortedNotes = []
    noteIndex = 0
    startTime = 0
    songDurationMs = 0
    judgedSet = new Set()
    song.value = null
    // Stop & clean up audio
    if (currentAudio) {
      currentAudio.pause()
      currentAudio.src = ''
      currentAudio.load()
      currentAudio = null
    }
    cancelAnimationFrame(animFrameId)
  }

  function hasJudged(note: Note): boolean {
    return judgedSet.has(note)
  }

  // ── Lifecycle helpers ───────────────────────────────────────────

  function startLoop() {
    startTime = performance.now()
    animFrameId = requestAnimationFrame(tick)
  }

  function stopLoop() {
    cancelAnimationFrame(animFrameId)
  }

  // ── Register phase change listener ──────────────────────────────
  function setOnPhaseChange(cb: (p: GamePhase) => void) {
    onPhaseChange = cb
  }

  // ── Return ──────────────────────────────────────────────────────

  return {
    // State (readonly external)
    phase:       readonly(phase),
    song:        readonly(song),
    currentDifficulty: readonly(currentDifficulty),
    score:       readonly(score),
    combo:       readonly(combo),
    maxCombo:    readonly(maxCombo),
    totalNotes:  readonly(totalNotes),
    judgedNotes: readonly(judgedNotes),
    missedNotes: readonly(missedNotes),
    perfectCount: readonly(perfectCount),
    goodCount:   readonly(goodCount),
    okCount:     readonly(okCount),
    missCount:   readonly(missCount),
    lastJudgment: readonly(lastJudgment),
    danceIntensity: readonly(danceIntensity),
    elapsedMs:   readonly(elapsedMs),

    // Computed
    accuracy,
    grade,
    progress,

    // Actions
    enterSelection,
    startSong,
    togglePause,
    endGame,
    exit,
    tick,
    pressKey,
    releaseKey,
    startLoop,
    stopLoop,
    setOnPhaseChange,
    hasJudged,

    // Constants
    JUDGMENT,
  }
}