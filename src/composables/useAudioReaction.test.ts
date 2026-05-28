/**
 * Tests for useAudioReaction.
 *
 * Strategy: mock @tauri-apps/api/event so we can capture the listener
 * registered for each event name and fire it manually. Each test mounts a
 * tiny Vue component that calls useAudioReaction(), then asserts how the
 * mocked queueAnim / cancelPending get called as we simulate audio events.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { defineComponent, h, ref } from 'vue'
import { mount } from '@vue/test-utils'

import type { AnimationName } from './useAnimator'

// ── Mock @tauri-apps/api/core ──────────────────────────────────────
// `invoke('get_audio_state')` returns false by default (no audio).
// Individual tests can override it via mockInvokeAudioState().
let _invokeAudioState = false
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(() => Promise.resolve(_invokeAudioState)),
}))

function mockInvokeAudioState(playing: boolean) {
  _invokeAudioState = playing
}

// ── Mock @tauri-apps/api/event ─────────────────────────────────────
// We replace `listen` with a function that stores the supplied handler
// keyed by event name. Tests call `fire('audio-started')` (etc.) to
// invoke the handler synchronously.
const handlers: Record<string, (e: { payload: unknown }) => void> = {}

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn((name: string, cb: (e: { payload: unknown }) => void) => {
    handlers[name] = cb
    return Promise.resolve(() => { delete handlers[name] })
  }),
}))

function fire(name: 'audio-started' | 'audio-stopped') {
  const h = handlers[name]
  if (!h) throw new Error(`no handler for ${name}`)
  h({ payload: undefined })
}

// Imported AFTER the mock is registered.
import { useAudioReaction } from './useAudioReaction'

// ── Test harness ───────────────────────────────────────────────────
interface Harness {
  queueAnim:     ReturnType<typeof vi.fn>
  cancelPending: ReturnType<typeof vi.fn>
  setCurrent:    (n: AnimationName) => void
  setPending:    (n: AnimationName | null) => void
}

async function mountReaction(initial: AnimationName = 'idle'): Promise<Harness> {
  const currentName = ref<AnimationName>(initial)
  let pending: AnimationName | null = null

  const queueAnim     = vi.fn((n: AnimationName) => { pending = n })
  const cancelPending = vi.fn(() => { pending = null })
  const getPending    = () => pending

  const Comp = defineComponent({
    setup() {
      useAudioReaction(queueAnim, currentName, getPending, cancelPending)
      return () => h('div')
    },
  })

  mount(Comp)
  // The listeners are registered in onMounted with `await`; flush microtasks.
  await Promise.resolve()
  await Promise.resolve()

  return {
    queueAnim,
    cancelPending,
    setCurrent: n => { currentName.value = n },
    setPending: n => { pending = n },
  }
}

beforeEach(() => {
  for (const k of Object.keys(handlers)) delete handlers[k]
  _invokeAudioState = false   // default: no audio playing at mount time
})

afterEach(() => {
  vi.clearAllMocks()
})

// ── Tests ──────────────────────────────────────────────────────────

describe('useAudioReaction → audio-started', () => {
  it('queues headphones_on when currently idle', async () => {
    const hx = await mountReaction('idle')
    fire('audio-started')
    expect(hx.queueAnim).toHaveBeenCalledWith('headphones_on')
    expect(hx.cancelPending).not.toHaveBeenCalled()
  })

  it('queues headphones_on when in click animation', async () => {
    const hx = await mountReaction('click')
    fire('audio-started')
    expect(hx.queueAnim).toHaveBeenCalledWith('headphones_on')
  })

  it('queues headphones_on when currently exiting via headphones_off (the regression case)', async () => {
    // This is the bug the user found: audio resuming while the pet is in
    // the headphones_off exit animation should queue a re-entry, NOT
    // treat headphones_off as "still in music mode".
    const hx = await mountReaction('headphones_off')
    fire('audio-started')
    expect(hx.queueAnim).toHaveBeenCalledWith('headphones_on')
  })

  it('does NOT queue when already in music', async () => {
    const hx = await mountReaction('music1')
    fire('audio-started')
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('does NOT queue when already in music2', async () => {
    const hx = await mountReaction('music2')
    fire('audio-started')
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('does NOT queue when already in music3', async () => {
    const hx = await mountReaction('music3')
    fire('audio-started')
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('does NOT queue when already in music4', async () => {
    const hx = await mountReaction('music4')
    fire('audio-started')
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('does NOT queue when entering via headphones_on', async () => {
    const hx = await mountReaction('headphones_on')
    fire('audio-started')
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('cancels a stale pending headphones_off when audio resumes mid-music', async () => {
    // Simulates: music → audio-stopped → queue headphones_off → audio-started
    // before the music loop ended. The stale pending must be cancelled.
    const hx = await mountReaction('music1')
    hx.setPending('headphones_off')
    fire('audio-started')
    expect(hx.cancelPending).toHaveBeenCalled()
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('does NOT cancel pending if pending is something other than headphones_off', async () => {
    const hx = await mountReaction('music1')
    hx.setPending('click')
    fire('audio-started')
    expect(hx.cancelPending).not.toHaveBeenCalled()
  })
})

describe('useAudioReaction → audio-stopped', () => {
  it('queues headphones_off when in music', async () => {
    const hx = await mountReaction('music1')
    fire('audio-stopped')
    expect(hx.queueAnim).toHaveBeenCalledWith('headphones_off')
  })

  it('queues headphones_off when in music2', async () => {
    const hx = await mountReaction('music2')
    fire('audio-stopped')
    expect(hx.queueAnim).toHaveBeenCalledWith('headphones_off')
  })

  it('queues headphones_off when in music3', async () => {
    const hx = await mountReaction('music3')
    fire('audio-stopped')
    expect(hx.queueAnim).toHaveBeenCalledWith('headphones_off')
  })

  it('queues headphones_off when in music4', async () => {
    const hx = await mountReaction('music4')
    fire('audio-stopped')
    expect(hx.queueAnim).toHaveBeenCalledWith('headphones_off')
  })

  it('queues headphones_off when in headphones_on (we never reached music yet)', async () => {
    const hx = await mountReaction('headphones_on')
    fire('audio-stopped')
    expect(hx.queueAnim).toHaveBeenCalledWith('headphones_off')
  })

  it('does NOT queue headphones_off when in idle', async () => {
    const hx = await mountReaction('idle')
    fire('audio-stopped')
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('does NOT queue headphones_off when already in headphones_off', async () => {
    const hx = await mountReaction('headphones_off')
    fire('audio-stopped')
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('cancels a stale pending headphones_on when in idle', async () => {
    // Simulates: idle → audio-started → queue headphones_on → audio-stopped
    // before headphones_on actually started. The stale pending must be cancelled.
    const hx = await mountReaction('idle')
    hx.setPending('headphones_on')
    fire('audio-stopped')
    expect(hx.cancelPending).toHaveBeenCalled()
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('does NOT cancel pending headphones_on if currently in music (handled via queue instead)', async () => {
    // When in music with pending=headphones_on (unusual but possible), audio-stopped
    // should take the queue-headphones_off branch, not the cancel branch.
    const hx = await mountReaction('music1')
    hx.setPending('headphones_on')
    fire('audio-stopped')
    expect(hx.queueAnim).toHaveBeenCalledWith('headphones_off')
    expect(hx.cancelPending).not.toHaveBeenCalled()
  })
})

describe('useAudioReaction → realistic event sequences', () => {
  it('audio stop → music loop ends → headphones_off plays → audio resumes mid-exit → re-enters', async () => {
    const hx = await mountReaction('music1')

    // 1. User stops audio. Pet is in music.
    fire('audio-stopped')
    expect(hx.queueAnim).toHaveBeenNthCalledWith(1, 'headphones_off')

    // 2. Music loop ends, pet transitions to headphones_off. Simulate by
    // updating currentName + clearing the (consumed) pending.
    hx.setCurrent('headphones_off')
    hx.setPending(null)

    // 3. User restarts audio while headphones_off is still playing.
    fire('audio-started')
    expect(hx.queueAnim).toHaveBeenNthCalledWith(2, 'headphones_on')
  })

  it('stop → fast restart while still in music: pending cancelled, no glitch', async () => {
    const hx = await mountReaction('music1')
    fire('audio-stopped')
    expect(hx.queueAnim).toHaveBeenCalledWith('headphones_off')
    // music loop has not ended yet — pending still 'headphones_off'
    hx.setPending('headphones_off')
    fire('audio-started')
    expect(hx.cancelPending).toHaveBeenCalled()
    // Only the initial headphones_off was queued; nothing else.
    expect(hx.queueAnim).toHaveBeenCalledTimes(1)
  })

  it('start → fast stop while still in idle: pending cancelled', async () => {
    const hx = await mountReaction('idle')
    fire('audio-started')
    expect(hx.queueAnim).toHaveBeenCalledWith('headphones_on')
    hx.setPending('headphones_on')
    fire('audio-stopped')
    expect(hx.cancelPending).toHaveBeenCalled()
    expect(hx.queueAnim).toHaveBeenCalledTimes(1)
  })
})

describe('useAudioReaction → initial-state bootstrap (get_audio_state)', () => {
  // Covers the WebView2 cold-start race: audio-started fires before the
  // listener registers, so we pull AudioState via invoke on mount.

  it('queues headphones_on immediately when audio is already playing at mount (idle)', async () => {
    mockInvokeAudioState(true)
    const hx = await mountReaction('idle')
    expect(hx.queueAnim).toHaveBeenCalledWith('headphones_on')
  })

  it('queues headphones_on immediately when audio is already playing at mount (click)', async () => {
    mockInvokeAudioState(true)
    const hx = await mountReaction('click')
    expect(hx.queueAnim).toHaveBeenCalledWith('headphones_on')
  })

  it('does NOT queue when audio is already playing but pet is in music1', async () => {
    mockInvokeAudioState(true)
    const hx = await mountReaction('music1')
    // Already in music — no need to re-enter via headphones_on.
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('does NOT queue when audio is already playing but pet is in music2', async () => {
    mockInvokeAudioState(true)
    const hx = await mountReaction('music2')
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('does NOT queue when audio is already playing but pet is in music3', async () => {
    mockInvokeAudioState(true)
    const hx = await mountReaction('music3')
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('does NOT queue when audio is already playing but pet is in music4', async () => {
    mockInvokeAudioState(true)
    const hx = await mountReaction('music4')
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('does NOT queue when audio is already playing but pet is entering via headphones_on', async () => {
    mockInvokeAudioState(true)
    const hx = await mountReaction('headphones_on')
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('does NOT queue when get_audio_state returns false (normal cold start)', async () => {
    mockInvokeAudioState(false)
    const hx = await mountReaction('idle')
    expect(hx.queueAnim).not.toHaveBeenCalled()
  })

  it('live audio-started event still works normally after a false initial state', async () => {
    mockInvokeAudioState(false)
    const hx = await mountReaction('idle')
    fire('audio-started')
    expect(hx.queueAnim).toHaveBeenCalledWith('headphones_on')
    expect(hx.queueAnim).toHaveBeenCalledTimes(1)
  })
})
