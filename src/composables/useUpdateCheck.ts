/**
 * useUpdateCheck — the HOW of update checking: runs in the always-on pet window,
 * decides when to poll GitHub Releases (via tauri-plugin-updater), and opens the
 * update pop-up when a newer version exists. The WHEN/snooze policy is the pure
 * module src/config/updatePolicy.ts; this file only wires it to the plugin, the
 * app config, and the `open_update_window` command.
 *
 * Scheduling: rather than a single 24 h timer (which drifts across sleep /
 * suspend), it re-evaluates the gate every hour AND whenever the window regains
 * focus / becomes visible. `updateLastCheck` in the config is the single source
 * of truth for "when did we last check", so those frequent ticks still perform
 * at most one real network check per CHECK_INTERVAL_HOURS.
 *
 * Only display metadata (version + notes) is handed to the pop-up — the
 * updater's Update object can't cross windows, so the pop-up re-fetches it at
 * install time (see update_window.rs / UpdateWindow.vue).
 */
import { onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { check } from '@tauri-apps/plugin-updater'
import { isSnoozed, shouldCheckNow } from '../config/updatePolicy'
import { useAppConfig } from './useAppConfig'

/** Minimal shape of what the updater's `check()` returns that we consume. */
export interface UpdateInfo {
  version: string
  body?: string
}

/** Everything runUpdateCheck needs, injected so it stays Vue/Tauri-free. */
export interface UpdateCheckContext {
  now: number
  autoCheck: boolean
  lastCheckMs: number | null
  snoozeUntilMs: number | null
  check: () => Promise<UpdateInfo | null>
  openUpdateWindow: (version: string, notes: string) => Promise<void>
  markChecked: (nowIso: string) => Promise<void>
}

/** What a single evaluation decided — surfaced for tests and logging. */
export type UpdateCheckOutcome =
  | 'skipped-disabled'
  | 'skipped-snoozed'
  | 'skipped-throttled'
  | 'checked-none'
  | 'checked-available'
  | 'error'

/**
 * Pure decision + side-effect orchestrator. Gates on auto-check / snooze /
 * throttle, then (if the gate passes) queries the updater. `markChecked` is only
 * recorded on a *successful* query — a network error leaves `updateLastCheck`
 * untouched so the next tick can retry rather than going quiet for a full day.
 */
export async function runUpdateCheck(ctx: UpdateCheckContext): Promise<UpdateCheckOutcome> {
  if (!ctx.autoCheck) return 'skipped-disabled'
  if (isSnoozed(ctx.now, ctx.snoozeUntilMs)) return 'skipped-snoozed'
  if (!shouldCheckNow(ctx.now, ctx.lastCheckMs)) return 'skipped-throttled'

  let update: UpdateInfo | null
  try {
    update = await ctx.check()
  } catch {
    return 'error' // leave lastCheck unchanged → retry on the next tick
  }

  await ctx.markChecked(new Date(ctx.now).toISOString())
  if (!update) return 'checked-none'

  await ctx.openUpdateWindow(update.version, update.body ?? '')
  return 'checked-available'
}

const HOURLY_MS = 60 * 60 * 1000

function parseMs(iso: string | null): number | null {
  if (!iso) return null
  const ms = Date.parse(iso)
  return Number.isNaN(ms) ? null : ms
}

/**
 * Mount in the pet window. Checks on startup, then hourly and on focus/visibility
 * — each re-evaluation is throttled by shouldCheckNow, so this is cheap.
 */
export function useUpdateCheck() {
  const { config, updateConfig } = useAppConfig()
  let timer: ReturnType<typeof setInterval> | null = null
  let running = false

  async function tick(): Promise<void> {
    if (running) return // don't overlap a slow network check with the next tick
    running = true
    try {
      const c = config.value
      await runUpdateCheck({
        now: Date.now(),
        autoCheck: c.updateAutoCheck,
        lastCheckMs: parseMs(c.updateLastCheck),
        snoozeUntilMs: parseMs(c.updateSnoozeUntil),
        check: () => check(),
        openUpdateWindow: (version, notes) =>
          invoke('open_update_window', { version, notes }),
        markChecked: (iso) => updateConfig({ updateLastCheck: iso }),
      })
    } finally {
      running = false
    }
  }

  function onFocus(): void {
    void tick()
  }
  function onVisibility(): void {
    if (document.visibilityState === 'visible') void tick()
  }

  onMounted(() => {
    void tick()
    timer = setInterval(() => void tick(), HOURLY_MS)
    window.addEventListener('focus', onFocus)
    document.addEventListener('visibilitychange', onVisibility)
  })

  onUnmounted(() => {
    if (timer !== null) clearInterval(timer)
    window.removeEventListener('focus', onFocus)
    document.removeEventListener('visibilitychange', onVisibility)
  })
}
