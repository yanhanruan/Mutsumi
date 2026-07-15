/**
 * Tests for the update-flow state machine (src/config/updateFlow.ts).
 *
 * These encode the client-side reliability table for the updater: every row
 * below maps to a scenario from the release-readiness spec. The machine's core
 * safety property — illegal transitions are ignored — is what turns "we didn't
 * test that ordering" into "that ordering cannot happen".
 *
 * Note on version comparison: "remote version equal/lower than current" is
 * decided inside tauri-plugin-updater (check() returns null), so from the
 * client's point of view both are CHECK_NONE. The plugin-side contract is
 * exercised by the fake-update-server integration scenarios (Layer 2), not
 * mocked here.
 */
import { describe, it, expect } from 'vitest'
import {
  INITIAL_UPDATE_FLOW,
  transition,
  type UpdateFlowEvent,
  type UpdateFlowState,
} from './updateFlow'

/** Run a sequence of events from the initial state. */
function run(...events: UpdateFlowEvent[]): UpdateFlowState {
  return events.reduce(transition, INITIAL_UPDATE_FLOW)
}

const FOUND: UpdateFlowEvent = { type: 'CHECK_FOUND', version: '1.5.0', notes: 'notes' }

// ── Check outcomes ─────────────────────────────────────────────────

describe('check outcomes', () => {
  it('no new version → notAvailable ("already on the latest version")', () => {
    const s = run({ type: 'CHECK_NONE' })
    expect(s.phase).toBe('notAvailable')
  })

  it('equal or lower remote version is the same CHECK_NONE contract (plugin returns null)', () => {
    // Documented mapping: the client never sees a version to compare — the
    // plugin already decided. Both rows of the table collapse to notAvailable.
    expect(run({ type: 'CHECK_NONE' }).phase).toBe('notAvailable')
  })

  it('new version available → available with version + notes', () => {
    const s = run(FOUND)
    expect(s.phase).toBe('available')
    expect(s.version).toBe('1.5.0')
    expect(s.notes).toBe('notes')
  })

  it('network timeout → failed with the reason, marked as a check failure', () => {
    const s = run({ type: 'CHECK_FAIL', detail: 'update check timed out' })
    expect(s.phase).toBe('failed')
    expect(s.errorDetail).toBe('update check timed out')
    expect(s.failedDuring).toBe('check')
  })

  it('invalid latest.json → failed with detail (no crash, no update offered)', () => {
    const s = run({ type: 'CHECK_FAIL', detail: 'invalid manifest: missing platforms key' })
    expect(s.phase).toBe('failed')
    expect(s.version).toBe('') // nothing was offered
  })
})

// ── Happy path ─────────────────────────────────────────────────────

describe('happy path', () => {
  it('checking → available → downloading → installing → installed', () => {
    let s = run(FOUND, { type: 'INSTALL_START' })
    expect(s.phase).toBe('downloading')
    s = transition(s, { type: 'PROGRESS', percent: 40 })
    expect(s.percent).toBe(40)
    s = transition(s, { type: 'DOWNLOAD_DONE' })
    expect(s.phase).toBe('installing')
    expect(s.percent).toBe(100)
    s = transition(s, { type: 'INSTALL_DONE' })
    expect(s.phase).toBe('installed')
  })

  it('clamps out-of-range progress values', () => {
    const base = run(FOUND, { type: 'INSTALL_START' })
    expect(transition(base, { type: 'PROGRESS', percent: 250 }).percent).toBe(100)
    expect(transition(base, { type: 'PROGRESS', percent: -5 }).percent).toBe(0)
    expect(transition(base, { type: 'PROGRESS', percent: NaN }).percent).toBe(0)
  })
})

// ── Download / install failures ────────────────────────────────────

describe('download & install failures', () => {
  const downloading = run(FOUND, { type: 'INSTALL_START' }, { type: 'PROGRESS', percent: 56 })

  it('download failure → failed with detail, marked as an install failure', () => {
    const s = transition(downloading, { type: 'FAIL', detail: 'connection reset' })
    expect(s.phase).toBe('failed')
    expect(s.errorDetail).toBe('connection reset')
    expect(s.failedDuring).toBe('install')
  })

  it('invalid signature (rejected by the plugin) → failed, never installed', () => {
    const s = transition(downloading, { type: 'FAIL', detail: 'signature verification failed' })
    expect(s.phase).toBe('failed')
    expect(s.errorDetail).toContain('signature')
  })

  it('an interrupted download can NEVER reach a success state (stray events ignored)', () => {
    let s = transition(downloading, { type: 'FAIL', detail: 'socket hang up' })
    // Late callbacks from the aborted download arrive after the failure:
    s = transition(s, { type: 'PROGRESS', percent: 100 })
    s = transition(s, { type: 'DOWNLOAD_DONE' })
    s = transition(s, { type: 'INSTALL_DONE' })
    expect(s.phase).toBe('failed')
    expect(s.percent).toBe(56) // untouched by the post-failure PROGRESS
  })
})

// ── Retry semantics ────────────────────────────────────────────────

describe('RETRY', () => {
  it('after a failed check → back to checking (a fresh check)', () => {
    const s = run({ type: 'CHECK_FAIL', detail: 'offline' }, { type: 'RETRY' })
    expect(s).toEqual(INITIAL_UPDATE_FLOW)
  })

  it('after a failed download → back to the available offer, version intact', () => {
    const s = run(
      FOUND,
      { type: 'INSTALL_START' },
      { type: 'FAIL', detail: 'connection reset' },
      { type: 'RETRY' },
    )
    expect(s.phase).toBe('available')
    expect(s.version).toBe('1.5.0') // the offer survives the retry
    expect(s.errorDetail).toBe('')
    expect(s.percent).toBe(0)
  })

  it('is a no-op outside the failed phase', () => {
    const offered = run(FOUND)
    expect(transition(offered, { type: 'RETRY' })).toBe(offered)
  })
})

// ── Illegal-transition safety net ──────────────────────────────────

describe('illegal transitions are no-ops', () => {
  it('check results are ignored outside the checking phase', () => {
    const offered = run(FOUND)
    expect(transition(offered, FOUND)).toBe(offered)
    expect(transition(offered, { type: 'CHECK_NONE' })).toBe(offered)
    expect(transition(offered, { type: 'CHECK_FAIL', detail: 'x' })).toBe(offered)
  })

  it('INSTALL_START requires an offer on the table', () => {
    expect(transition(INITIAL_UPDATE_FLOW, { type: 'INSTALL_START' })).toBe(INITIAL_UPDATE_FLOW)
    const done = run(FOUND, { type: 'INSTALL_START' }, { type: 'DOWNLOAD_DONE' }, { type: 'INSTALL_DONE' })
    expect(transition(done, { type: 'INSTALL_START' })).toBe(done)
  })

  it('a FAIL outside download/install is ignored (stray late callback)', () => {
    const upToDate = run({ type: 'CHECK_NONE' })
    expect(transition(upToDate, { type: 'FAIL', detail: 'late' })).toBe(upToDate)
    const installed = run(FOUND, { type: 'INSTALL_START' }, { type: 'DOWNLOAD_DONE' }, { type: 'INSTALL_DONE' })
    expect(transition(installed, { type: 'FAIL', detail: 'late' })).toBe(installed)
  })

  it('CHECK_START cannot interrupt an active download/install', () => {
    const downloading = run(FOUND, { type: 'INSTALL_START' })
    expect(transition(downloading, { type: 'CHECK_START' })).toBe(downloading)
    const installing = transition(downloading, { type: 'DOWNLOAD_DONE' })
    expect(transition(installing, { type: 'CHECK_START' })).toBe(installing)
  })

  it('installed is terminal', () => {
    const installed = run(FOUND, { type: 'INSTALL_START' }, { type: 'DOWNLOAD_DONE' }, { type: 'INSTALL_DONE' })
    for (const ev of [
      { type: 'PROGRESS', percent: 10 },
      { type: 'DOWNLOAD_DONE' },
      { type: 'INSTALL_DONE' },
      { type: 'RETRY' },
    ] as UpdateFlowEvent[]) {
      expect(transition(installed, ev)).toBe(installed)
    }
  })
})
