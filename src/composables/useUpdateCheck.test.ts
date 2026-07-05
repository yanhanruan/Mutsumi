/**
 * Tests for the update-check decision orchestrator (runUpdateCheck).
 *
 * runUpdateCheck is deliberately Vue/Tauri-free — it takes its collaborators as
 * plain functions — so these tests use hand-rolled spies with no module mocking.
 * The pure gating math (shouldCheckNow / isSnoozed) is covered separately in
 * src/config/updatePolicy.test.ts; here we assert the orchestration: which
 * branch runs, and whether the check / markChecked / openUpdateWindow
 * side-effects fire.
 */
import { describe, it, expect, vi } from 'vitest'
import { runUpdateCheck, type UpdateCheckContext, type UpdateInfo } from './useUpdateCheck'

const HOUR = 60 * 60 * 1000
const DAY = 24 * HOUR
const NOW = Date.UTC(2026, 6, 5)

function makeCtx(over: Partial<UpdateCheckContext> = {}): UpdateCheckContext {
  return {
    now: NOW,
    autoCheck: true,
    lastCheckMs: null,
    snoozeUntilMs: null,
    check: vi.fn(async (): Promise<UpdateInfo | null> => null),
    openUpdateWindow: vi.fn(async () => {}),
    markChecked: vi.fn(async () => {}),
    ...over,
  }
}

describe('runUpdateCheck()', () => {
  it('skips (and never touches the network) when auto-check is off', async () => {
    const ctx = makeCtx({ autoCheck: false })
    expect(await runUpdateCheck(ctx)).toBe('skipped-disabled')
    expect(ctx.check).not.toHaveBeenCalled()
    expect(ctx.markChecked).not.toHaveBeenCalled()
    expect(ctx.openUpdateWindow).not.toHaveBeenCalled()
  })

  it('skips while snoozed', async () => {
    const ctx = makeCtx({ snoozeUntilMs: NOW + 3 * DAY })
    expect(await runUpdateCheck(ctx)).toBe('skipped-snoozed')
    expect(ctx.check).not.toHaveBeenCalled()
  })

  it('skips when checked less than a day ago', async () => {
    const ctx = makeCtx({ lastCheckMs: NOW - 2 * HOUR })
    expect(await runUpdateCheck(ctx)).toBe('skipped-throttled')
    expect(ctx.check).not.toHaveBeenCalled()
  })

  it('checks after the snooze deadline has passed', async () => {
    const ctx = makeCtx({ snoozeUntilMs: NOW - 1 * DAY })
    expect(await runUpdateCheck(ctx)).toBe('checked-none')
    expect(ctx.check).toHaveBeenCalledOnce()
  })

  it('records the check but opens nothing when already up to date', async () => {
    const ctx = makeCtx({ check: vi.fn(async () => null) })
    expect(await runUpdateCheck(ctx)).toBe('checked-none')
    expect(ctx.markChecked).toHaveBeenCalledOnce()
    expect(ctx.openUpdateWindow).not.toHaveBeenCalled()
  })

  it('opens the pop-up with version + notes when an update is available', async () => {
    const ctx = makeCtx({
      check: vi.fn(async () => ({ version: '1.5.0', body: 'Fixed things' })),
    })
    expect(await runUpdateCheck(ctx)).toBe('checked-available')
    expect(ctx.markChecked).toHaveBeenCalledOnce()
    expect(ctx.openUpdateWindow).toHaveBeenCalledWith('1.5.0', 'Fixed things')
  })

  it('passes empty notes when the release body is missing', async () => {
    const ctx = makeCtx({ check: vi.fn(async () => ({ version: '1.5.0' })) })
    await runUpdateCheck(ctx)
    expect(ctx.openUpdateWindow).toHaveBeenCalledWith('1.5.0', '')
  })

  it('reports an error and leaves lastCheck untouched when the check throws', async () => {
    const ctx = makeCtx({
      check: vi.fn(async () => {
        throw new Error('offline')
      }),
    })
    expect(await runUpdateCheck(ctx)).toBe('error')
    expect(ctx.markChecked).not.toHaveBeenCalled() // so the next tick retries
    expect(ctx.openUpdateWindow).not.toHaveBeenCalled()
  })
})
