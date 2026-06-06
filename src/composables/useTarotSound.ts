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
 * Schedule a single tone on the shared context.
 * @param freqStart  starting frequency (Hz)
 * @param atSec      offset from "now" to begin (s)
 * @param durSec     tone duration (s)
 * @param type       oscillator waveform
 * @param peak       peak gain (0–1)
 * @param freqEnd    optional end frequency for a glide (sweep)
 */
function tone(
  ctx: AudioContext,
  freqStart: number,
  atSec: number,
  durSec: number,
  type: OscillatorType = 'sine',
  peak = 0.16,
  freqEnd?: number,
): void {
  const osc  = ctx.createOscillator()
  const gain = ctx.createGain()
  osc.type = type

  const start = ctx.currentTime + atSec
  osc.frequency.setValueAtTime(freqStart, start)
  if (freqEnd !== undefined) {
    osc.frequency.exponentialRampToValueAtTime(freqEnd, start + durSec)
  }
  // Soft attack + exponential release — chime-like, not a click.
  gain.gain.setValueAtTime(0.0001, start)
  gain.gain.exponentialRampToValueAtTime(peak, start + 0.012)
  gain.gain.exponentialRampToValueAtTime(0.0001, start + durSec)

  osc.connect(gain).connect(ctx.destination)
  osc.start(start)
  osc.stop(start + durSec + 0.02)
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

/** Sound for drawing / dealing a new card — a soft ascending arpeggio. */
export function playDrawSound(): void {
  console.log('[tarot] playDrawSound()')
  if (playFile(TAROT_ASSETS.audio.draw)) return
  const ctx = getCtx()
  if (!ctx) return
  if (ctx.state === 'suspended') void ctx.resume()
  // C5 → E5 → G5 plucks, lightly staggered like cards being dealt.
  tone(ctx, 523.25, 0.00, 0.13, 'triangle', 0.13)
  tone(ctx, 659.25, 0.06, 0.13, 'triangle', 0.13)
  tone(ctx, 783.99, 0.12, 0.16, 'triangle', 0.15)
}

/** Sound for flipping a card face-up — a quick upward swish + chime. */
export function playFlipSound(): void {
  console.log('[tarot] playFlipSound()')
  if (playFile(TAROT_ASSETS.audio.flip)) return
  const ctx = getCtx()
  if (!ctx) return
  if (ctx.state === 'suspended') void ctx.resume()
  // Rising swish (the flip) then a bright settle tone (the reveal).
  tone(ctx, 420, 0.00, 0.18, 'sine', 0.12, 1100)
  tone(ctx, 987.77, 0.16, 0.22, 'sine', 0.14) // B5 settle
}
