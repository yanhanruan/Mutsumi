/**
 * updateFlow — the WHAT of the update pop-up: an explicit, pure state machine
 * for the check → download → install lifecycle. No Vue, no Tauri — the HOW
 * (calling the updater plugin, resizing the window, persisting check results)
 * lives in UpdateWindow.vue, which just dispatches events into `transition`.
 *
 * Why a reducer instead of ad-hoc `view.value = …` writes: the failure modes
 * of an updater are mostly *ordering* bugs — a progress callback that fires
 * after the download already failed, a stray "finished" after an abort — and
 * the way to make those impossible rather than merely untested is to make
 * every transition explicit and drop the illegal ones. `transition` returns
 * the state unchanged for any event that is not legal in the current phase,
 * so e.g. an interrupted download can never surface as "installed".
 *
 * Phase graph (events on arrows):
 *
 *   checking ──CHECK_FOUND──▶ available ──INSTALL_START──▶ downloading
 *      │                          ▲                            │
 *      ├─CHECK_NONE─▶ notAvailable│                 PROGRESS (self)
 *      └─CHECK_FAIL─▶ failed      │                            │
 *                        │        │                     DOWNLOAD_DONE
 *                        │RETRY   │                            ▼
 *                        └────────┴──(check)          installing ──INSTALL_DONE──▶ installed
 *                                                              │
 *   downloading/installing ──FAIL──▶ failed                    │
 *   (RETRY from an install failure returns to `available`) ◀───┘
 *
 * `installed` and `notAvailable` are terminal (the app relaunches / the user
 * closes the window).
 */

// ── State ──────────────────────────────────────────────────────────

export type UpdatePhase =
  | 'checking'
  | 'available'
  | 'notAvailable'
  | 'downloading'
  | 'installing'
  | 'installed'
  | 'failed'

export interface UpdateFlowState {
  phase: UpdatePhase
  /** New version on offer (set by CHECK_FOUND, kept through the install). */
  version: string
  /** Release notes for the offered version. */
  notes: string
  /** Download progress 0–100 (only meaningful in `downloading`). */
  percent: number
  /** Human-readable reason shown in the failed view. */
  errorDetail: string
  /**
   * Which stage failed — drives Retry: a failed check retries the check,
   * a failed download/install returns to the `available` offer.
   */
  failedDuring: 'check' | 'install' | null
}

export const INITIAL_UPDATE_FLOW: UpdateFlowState = {
  phase: 'checking',
  version: '',
  notes: '',
  percent: 0,
  errorDetail: '',
  failedDuring: null,
}

// ── Events ─────────────────────────────────────────────────────────

export type UpdateFlowEvent =
  | { type: 'CHECK_START' }
  | { type: 'CHECK_FOUND'; version: string; notes: string }
  | { type: 'CHECK_NONE' }
  | { type: 'CHECK_FAIL'; detail: string }
  | { type: 'INSTALL_START' }
  | { type: 'PROGRESS'; percent: number }
  | { type: 'DOWNLOAD_DONE' }
  | { type: 'INSTALL_DONE' }
  | { type: 'FAIL'; detail: string }
  | { type: 'RETRY' }

// ── Reducer ────────────────────────────────────────────────────────

/**
 * Advance the flow by one event. Events that are not legal in the current
 * phase return the state **unchanged** — this is the machine's core safety
 * property, asserted directly by the unit tests.
 */
export function transition(state: UpdateFlowState, event: UpdateFlowEvent): UpdateFlowState {
  switch (event.type) {
    case 'CHECK_START':
      // Legal from anywhere except mid-install (a re-check while downloading
      // would tear the rug out from under downloadAndInstall).
      if (state.phase === 'downloading' || state.phase === 'installing') return state
      return { ...INITIAL_UPDATE_FLOW }

    case 'CHECK_FOUND':
      if (state.phase !== 'checking') return state
      return { ...state, phase: 'available', version: event.version, notes: event.notes }

    case 'CHECK_NONE':
      if (state.phase !== 'checking') return state
      return { ...state, phase: 'notAvailable' }

    case 'CHECK_FAIL':
      if (state.phase !== 'checking') return state
      return { ...state, phase: 'failed', errorDetail: event.detail, failedDuring: 'check' }

    case 'INSTALL_START':
      if (state.phase !== 'available') return state
      return { ...state, phase: 'downloading', percent: 0 }

    case 'PROGRESS':
      if (state.phase !== 'downloading') return state
      return { ...state, percent: clampPercent(event.percent) }

    case 'DOWNLOAD_DONE':
      if (state.phase !== 'downloading') return state
      return { ...state, phase: 'installing', percent: 100 }

    case 'INSTALL_DONE':
      if (state.phase !== 'installing') return state
      return { ...state, phase: 'installed' }

    case 'FAIL':
      // Legal only while actively downloading/installing; a failure elsewhere
      // is either CHECK_FAIL's job or a stray late callback to be ignored.
      if (state.phase !== 'downloading' && state.phase !== 'installing') return state
      return { ...state, phase: 'failed', errorDetail: event.detail, failedDuring: 'install' }

    case 'RETRY':
      if (state.phase !== 'failed') return state
      if (state.failedDuring === 'install') {
        // Return to the offer (version/notes are still in state).
        return { ...state, phase: 'available', percent: 0, errorDetail: '', failedDuring: null }
      }
      return { ...INITIAL_UPDATE_FLOW } // failed check → check again
  }
}

function clampPercent(p: number): number {
  if (!Number.isFinite(p)) return 0
  return Math.min(100, Math.max(0, Math.round(p)))
}
