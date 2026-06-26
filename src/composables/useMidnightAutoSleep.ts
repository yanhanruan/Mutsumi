/**
 * useMidnightAutoSleep — put the pet to sleep when the local clock passes
 * midnight while she is idle.
 *
 * A lightweight poll samples the local time on an interval and fires once,
 * exactly on the calendar-day rollover (00:00). The decision of whether it's
 * appropriate to sleep right now (idle, not already asleep, no overlay open)
 * is delegated to the caller via `canSleep`, so this composable stays unaware
 * of the animation state machine.
 *
 * `crossedMidnight` is a pure helper, exported for unit testing.
 */
import { onMounted, onUnmounted } from 'vue'

/** Default sample cadence — within this of 00:00 she'll nod off. */
export const DEFAULT_POLL_MS = 30_000

/**
 * True iff `now` falls on a later (or different) calendar day than `prev` in
 * local time — i.e. the clock crossed midnight between the two samples.
 */
export function crossedMidnight(prev: Date, now: Date): boolean {
  return (
    now.getFullYear() !== prev.getFullYear() ||
    now.getMonth()    !== prev.getMonth()    ||
    now.getDate()     !== prev.getDate()
  )
}

export interface MidnightAutoSleepOptions {
  /** Whether it's currently appropriate to fall asleep (e.g. idle & visible). */
  canSleep: () => boolean
  /** Invoked once when midnight is crossed and `canSleep()` is true. */
  sleep: () => void
  /** Poll cadence in ms (default DEFAULT_POLL_MS). */
  intervalMs?: number
  /** Clock source, injectable for tests (default: () => new Date()). */
  now?: () => Date
}

export function useMidnightAutoSleep(opts: MidnightAutoSleepOptions) {
  const { canSleep, sleep } = opts
  const intervalMs = opts.intervalMs ?? DEFAULT_POLL_MS
  const now = opts.now ?? (() => new Date())

  let last = now()
  let timer: ReturnType<typeof setInterval> | null = null

  function check() {
    const t = now()
    if (crossedMidnight(last, t) && canSleep()) {
      sleep()
    }
    last = t
  }

  onMounted(() => {
    timer = setInterval(check, intervalMs)
  })

  onUnmounted(() => {
    if (timer !== null) clearInterval(timer)
  })
}
