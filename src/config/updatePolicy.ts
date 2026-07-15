/**
 * updatePolicy — the WHAT of update checking: pure scheduling/snooze policy,
 * no Vue and no Tauri. The HOW (calling the updater plugin, opening the pop-up)
 * lives in the useUpdateCheck composable. Kept here, next to the other pure
 * policy modules (animations.ts::resolveBaseline, tarot.ts), so the decision
 * rules are trivially unit-testable in isolation.
 *
 * There is deliberately no version comparison here: tauri-plugin-updater's
 * `check()` already returns `null` when the local build is current, so a
 * hand-rolled semver compare would be dead code. These functions only decide
 * *when* to check and *how long* to stay quiet after a "remind me later".
 *
 * All functions take absolute epoch-millisecond timestamps (Date.now()) rather
 * than Date objects, so tests can pin "now" without mocking the clock.
 */

/** Minimum "remind me later" horizon, in days. */
export const SNOOZE_MIN_DAYS = 1
/** Maximum "remind me later" horizon, in days. */
export const SNOOZE_MAX_DAYS = 30
/** How often the background check may run, in hours. */
export const CHECK_INTERVAL_HOURS = 24

const MS_PER_HOUR = 60 * 60 * 1000
const MS_PER_DAY = 24 * MS_PER_HOUR

/**
 * Clamp a requested snooze length to the allowed [SNOOZE_MIN_DAYS,
 * SNOOZE_MAX_DAYS] range, rounding to whole days. Non-finite input falls back
 * to the minimum so a bad UI value can never disable reminders entirely.
 */
export function clampSnoozeDays(days: number): number {
  if (!Number.isFinite(days)) return SNOOZE_MIN_DAYS
  const whole = Math.round(days)
  if (whole < SNOOZE_MIN_DAYS) return SNOOZE_MIN_DAYS
  if (whole > SNOOZE_MAX_DAYS) return SNOOZE_MAX_DAYS
  return whole
}

/**
 * Should the background updater run now? True when it has never run, or when at
 * least `intervalHours` have elapsed since the last check. A last-check
 * timestamp in the future (clock skew) is treated as "checked", so we wait.
 */
export function shouldCheckNow(
  nowMs: number,
  lastCheckMs: number | null,
  intervalHours: number = CHECK_INTERVAL_HOURS,
): boolean {
  if (lastCheckMs === null) return true
  return nowMs - lastCheckMs >= intervalHours * MS_PER_HOUR
}

/**
 * Is the update pop-up currently snoozed? True while `now` is before the stored
 * snooze deadline. `null` (never snoozed) and past deadlines are not snoozed.
 */
export function isSnoozed(nowMs: number, snoozeUntilMs: number | null): boolean {
  if (snoozeUntilMs === null) return false
  return nowMs < snoozeUntilMs
}

/**
 * Compute the snooze deadline for a "remind me later" of `days` days from now,
 * as an ISO string. `days` is clamped to the allowed range first.
 */
export function computeSnoozeUntil(nowMs: number, days: number): string {
  const clampedDays = clampSnoozeDays(days)
  return new Date(nowMs + clampedDays * MS_PER_DAY).toISOString()
}
