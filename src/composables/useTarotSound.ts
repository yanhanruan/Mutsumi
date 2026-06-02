/**
 * useTarotSound — sound-effect hooks for the tarot module.
 *
 * Real audio assets do not exist yet (see TAROT_ASSETS.audio in config/tarot.ts).
 * Until they do, each hook:
 *   1. logs the intent via console.log (so the call site is observable in tests
 *      and during development), and
 *   2. plays a short synthesized beep through the Web Audio API as an audible
 *      placeholder.
 *
 * When real files arrive, set the paths in TAROT_ASSETS.audio and this module
 * will play them via an <audio>-less HTMLAudioElement instead of the beep.
 * No call site changes required.
 */
import { TAROT_ASSETS } from '../config/tarot'

// A single lazily-created AudioContext, reused across beeps. Created on first
// user-gesture-driven call so browsers don't block it as autoplay.
let audioCtx: AudioContext | null = null

function getCtx(): AudioContext | null {
  if (typeof window === 'undefined') return null
  const Ctor = window.AudioContext ?? (window as unknown as { webkitAudioContext?: typeof AudioContext }).webkitAudioContext
  if (!Ctor) return null
  if (!audioCtx) audioCtx = new Ctor()
  return audioCtx
}

/**
 * Play a short sine beep as an audio placeholder.
 * @param freq    tone frequency in Hz
 * @param durMs   duration in milliseconds
 */
function beep(freq: number, durMs: number): void {
  const ctx = getCtx()
  if (!ctx) return
  // Resume if the context was suspended (common before the first gesture).
  if (ctx.state === 'suspended') void ctx.resume()

  const osc  = ctx.createOscillator()
  const gain = ctx.createGain()
  osc.type = 'sine'
  osc.frequency.value = freq

  const now = ctx.currentTime
  const dur = durMs / 1000
  // Quick attack + exponential release so it sounds like a soft chime, not a click.
  gain.gain.setValueAtTime(0.0001, now)
  gain.gain.exponentialRampToValueAtTime(0.18, now + 0.01)
  gain.gain.exponentialRampToValueAtTime(0.0001, now + dur)

  osc.connect(gain).connect(ctx.destination)
  osc.start(now)
  osc.stop(now + dur)
}

/** Play a real audio file if a path is configured. Returns true if it tried. */
function playFile(path: string): boolean {
  if (!path) return false
  try {
    const el = new Audio(path)
    void el.play().catch(() => { /* best-effort */ })
    return true
  } catch {
    return false
  }
}

/** Sound for drawing / dealing a new card. */
export function playDrawSound(): void {
  console.log('[tarot] playDrawSound()')
  if (playFile(TAROT_ASSETS.audio.draw)) return
  beep(523.25, 140) // C5
}

/** Sound for flipping a card face-up. */
export function playFlipSound(): void {
  console.log('[tarot] playFlipSound()')
  if (playFile(TAROT_ASSETS.audio.flip)) return
  beep(783.99, 180) // G5
}
