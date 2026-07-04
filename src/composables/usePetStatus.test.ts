/**
 * usePetStatus — unit tests.
 *
 * Verifies that:
 *  1. Initial state is read via invoke('get_state') on mount.
 *  2. pet-state-update events update the reactive refs.
 *  3. setIdleVariant is called with the correct AnimationName for each IdleVariant.
 *  4. Unlisten is called on unmount.
 */
import { describe, it, expect, vi, beforeEach, type Mock } from 'vitest'
import { mount, flushPromises } from '@vue/test-utils'
import { defineComponent } from 'vue'
import { usePetStatus } from './usePetStatus'
import type { IdleVariantName, PetStatusSnapshot } from './usePetStatus'
import type { AnimationName } from '../config/animations'

// ── Tauri mocks ────────────────────────────────────────────────────────────

const mockUnlisten = vi.fn()
const mockListen   = vi.fn()
const mockInvoke   = vi.fn()

vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => mockListen(...args),
}))
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}))

// ── Helpers ────────────────────────────────────────────────────────────────

function makeSnapshot(
  energy: number,
  affection: number,
  idleVariant: IdleVariantName,
  mood = 'content',
): PetStatusSnapshot {
  return { energy, affection, mood, idle_variant: idleVariant }
}

function makeStateResponse(snap: PetStatusSnapshot) {
  return { pet: snap, pomodoro: {} }
}

/** Mount a throwaway component that calls usePetStatus and returns the result. */
function mountComposable(setIdleVariant: (name: AnimationName) => void) {
  let result: ReturnType<typeof usePetStatus> | undefined

  const TestComp = defineComponent({
    setup() {
      result = usePetStatus(setIdleVariant)
      return {}
    },
    template: '<div></div>',
  })

  const wrapper = mount(TestComp, { attachTo: document.body })
  return { wrapper, getResult: () => result! }
}

// Captures the listener callback registered by listen('pet-state-update', cb)
function captureListener(): (payload: PetStatusSnapshot) => void {
  const call = (mockListen as Mock).mock.calls.find(
    (args: unknown[]) => args[0] === 'pet-state-update',
  )
  expect(call).toBeDefined()
  return (payload: PetStatusSnapshot) => call![1]({ payload })
}

// ── Tests ──────────────────────────────────────────────────────────────────

describe('usePetStatus', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    mockListen.mockResolvedValue(mockUnlisten)
    // Default: get_state returns a normal pet snapshot.
    mockInvoke.mockResolvedValue(
      makeStateResponse(makeSnapshot(100, 50, 'normal', 'content')),
    )
  })

  // ── Initial state from invoke ────────────────────────────────────────────

  it('calls invoke("get_state") on mount to seed initial state', async () => {
    const setIdleVariant = vi.fn()
    const { wrapper } = mountComposable(setIdleVariant)
    await flushPromises()
    expect(mockInvoke).toHaveBeenCalledWith('get_state')
    wrapper.unmount()
  })

  it('applies initial snapshot: sets energy/affection/mood refs', async () => {
    mockInvoke.mockResolvedValueOnce(
      makeStateResponse(makeSnapshot(42, 18, 'low_affection', 'sad')),
    )
    const setIdleVariant = vi.fn()
    const { wrapper, getResult } = mountComposable(setIdleVariant)
    await flushPromises()
    const { energy, affection, mood } = getResult()
    expect(energy.value).toBe(42)
    expect(affection.value).toBe(18)
    expect(mood.value).toBe('sad')
    wrapper.unmount()
  })

  it('calls setIdleVariant("idle") for normal variant on mount', async () => {
    mockInvoke.mockResolvedValueOnce(
      makeStateResponse(makeSnapshot(80, 60, 'normal')),
    )
    const setIdleVariant = vi.fn()
    const { wrapper } = mountComposable(setIdleVariant)
    await flushPromises()
    expect(setIdleVariant).toHaveBeenCalledWith('idle')
    wrapper.unmount()
  })

  it('calls setIdleVariant("idle_low_energy") for low_energy variant on mount', async () => {
    mockInvoke.mockResolvedValueOnce(
      makeStateResponse(makeSnapshot(20, 60, 'low_energy')),
    )
    const setIdleVariant = vi.fn()
    const { wrapper } = mountComposable(setIdleVariant)
    await flushPromises()
    expect(setIdleVariant).toHaveBeenCalledWith('idle_low_energy')
    wrapper.unmount()
  })

  it('calls setIdleVariant("idle_low_affection") for low_affection variant on mount', async () => {
    mockInvoke.mockResolvedValueOnce(
      makeStateResponse(makeSnapshot(80, 15, 'low_affection')),
    )
    const setIdleVariant = vi.fn()
    const { wrapper } = mountComposable(setIdleVariant)
    await flushPromises()
    expect(setIdleVariant).toHaveBeenCalledWith('idle_low_affection')
    wrapper.unmount()
  })

  it('calls setIdleVariant("idle_exhausted") for exhausted variant on mount', async () => {
    mockInvoke.mockResolvedValueOnce(
      makeStateResponse(makeSnapshot(10, 10, 'exhausted')),
    )
    const setIdleVariant = vi.fn()
    const { wrapper } = mountComposable(setIdleVariant)
    await flushPromises()
    expect(setIdleVariant).toHaveBeenCalledWith('idle_exhausted')
    wrapper.unmount()
  })

  // ── Graceful failure ────────────────────────────────────────────────────

  it('does not throw when get_state rejects; live events correct state', async () => {
    mockInvoke.mockRejectedValueOnce(new Error('backend not ready'))
    const setIdleVariant = vi.fn()
    const { wrapper } = mountComposable(setIdleVariant)
    await flushPromises()
    // Should not throw; setIdleVariant not called (no snapshot applied)
    expect(setIdleVariant).not.toHaveBeenCalled()
    wrapper.unmount()
  })

  // ── Live pet-state-update events ────────────────────────────────────────

  it('updates refs when pet-state-update event fires', async () => {
    const setIdleVariant = vi.fn()
    const { wrapper, getResult } = mountComposable(setIdleVariant)
    await flushPromises()

    const fire = captureListener()
    fire(makeSnapshot(25, 20, 'exhausted', 'tired'))

    const { energy, affection, mood, idleVariant } = getResult()
    expect(energy.value).toBe(25)
    expect(affection.value).toBe(20)
    expect(mood.value).toBe('tired')
    expect(idleVariant.value).toBe('exhausted')
    wrapper.unmount()
  })

  it('calls setIdleVariant on each pet-state-update event', async () => {
    const setIdleVariant = vi.fn()
    const { wrapper } = mountComposable(setIdleVariant)
    await flushPromises()

    const fire = captureListener()

    fire(makeSnapshot(80, 60, 'normal'))
    expect(setIdleVariant).toHaveBeenLastCalledWith('idle')

    fire(makeSnapshot(20, 60, 'low_energy'))
    expect(setIdleVariant).toHaveBeenLastCalledWith('idle_low_energy')

    fire(makeSnapshot(80, 15, 'low_affection'))
    expect(setIdleVariant).toHaveBeenLastCalledWith('idle_low_affection')

    fire(makeSnapshot(10, 10, 'exhausted'))
    expect(setIdleVariant).toHaveBeenLastCalledWith('idle_exhausted')

    wrapper.unmount()
  })

  // ── Cleanup ──────────────────────────────────────────────────────────────

  it('calls the unlisten function when component unmounts', async () => {
    const setIdleVariant = vi.fn()
    const { wrapper } = mountComposable(setIdleVariant)
    await flushPromises()
    wrapper.unmount()
    expect(mockUnlisten).toHaveBeenCalledTimes(1)
  })

  it('registers exactly one pet-state-update listener', async () => {
    const setIdleVariant = vi.fn()
    const { wrapper } = mountComposable(setIdleVariant)
    await flushPromises()
    const petStateListeners = (mockListen as Mock).mock.calls.filter(
      (args: unknown[]) => args[0] === 'pet-state-update',
    )
    expect(petStateListeners).toHaveLength(1)
    wrapper.unmount()
  })
})
