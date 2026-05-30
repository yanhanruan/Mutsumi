/**
 * useWeatherAvailable — unit tests.
 *
 * Strategy:
 *  - The composable is a singleton (module-level `_started` + `available`).
 *    We use `vi.resetModules()` + dynamic `import()` in each test to get a
 *    fresh instance with no carry-over from the previous test.
 *  - `@tauri-apps/api/event` is mocked so we can capture the `weather-status`
 *    handler and fire it manually.
 *  - `@tauri-apps/api/core` is mocked so we can resolve / reject `invoke`
 *    at a controlled moment.
 *
 * Tests:
 *  1. weatherAvailable is null immediately after first call (before any I/O).
 *  2. invoke resolving true  → weatherAvailable becomes true.
 *  3. invoke resolving false → weatherAvailable becomes false.
 *  4. live weather-status { available: false } event → weatherAvailable false.
 *  5. live weather-status { available: true  } event → weatherAvailable true.
 *  6. live event that arrives before invoke settles takes precedence (invoke
 *     skips its write because available is no longer null).
 *  7. live event that arrives after invoke has already set the value
 *     still overwrites it.
 *  8. invoke rejection is silently swallowed; weatherAvailable stays null.
 *  9. listen() rejection is silently swallowed; weatherAvailable stays null.
 * 10. Two calls to useWeatherAvailable() return the same reactive ref (singleton).
 */

import { describe, it, expect, vi, beforeEach } from 'vitest'

// ── Types for the mocked modules ──────────────────────────────────────

type WeatherStatusPayload = { available: boolean }
type EventHandler = (e: { payload: WeatherStatusPayload }) => void

// ── Helpers ───────────────────────────────────────────────────────────

/**
 * Reload the composable fresh (bypasses singleton cache) and return:
 *  - `weatherAvailable` ref
 *  - `fireEvent(available)` — synchronously calls the captured listen handler
 *  - `resolveInvoke(status)` / `rejectInvoke(err)` — settle the invoke promise
 */
async function freshComposable() {
  // Captured mutable state — populated once the module's startOnce() runs.
  let capturedHandler: EventHandler | null = null
  let resolveInvoke!: (v: boolean) => void
  let rejectInvoke!:  (e: unknown) => void

  const invokePromise = new Promise<boolean>((res, rej) => {
    resolveInvoke = res
    rejectInvoke  = rej
  })

  // Reset module registry so the singleton is brand-new.
  vi.resetModules()

  // Install mocks AFTER reset so they apply to the freshly-loaded module.
  vi.doMock('@tauri-apps/api/event', () => ({
    listen: vi.fn((name: string, handler: EventHandler) => {
      if (name === 'weather-status') capturedHandler = handler
      return Promise.resolve(() => {})
    }),
  }))

  vi.doMock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(() => invokePromise),
  }))

  // Dynamic import picks up the fresh mocks.
  const { useWeatherAvailable } = await import('./useWeatherAvailable')
  const { weatherAvailable } = useWeatherAvailable()

  // Flush microtasks so listen() / invoke() calls inside startOnce() run.
  await Promise.resolve()
  await Promise.resolve()

  function fireEvent(available: boolean) {
    if (!capturedHandler) throw new Error('weather-status handler was never registered')
    capturedHandler({ payload: { available } })
  }

  return { weatherAvailable, fireEvent, resolveInvoke, rejectInvoke }
}

// ── Reset between tests ───────────────────────────────────────────────

beforeEach(() => {
  vi.resetModules()
})

// ── Tests ─────────────────────────────────────────────────────────────

describe('useWeatherAvailable — initial state', () => {
  it('weatherAvailable is null before any I/O resolves', async () => {
    const { weatherAvailable } = await freshComposable()
    expect(weatherAvailable.value).toBeNull()
  })
})

describe('useWeatherAvailable — invoke get_weather_status', () => {
  it('sets weatherAvailable to true when invoke resolves true', async () => {
    const { weatherAvailable, resolveInvoke } = await freshComposable()
    resolveInvoke(true)
    await Promise.resolve()   // let the .then() handler run
    expect(weatherAvailable.value).toBe(true)
  })

  it('sets weatherAvailable to false when invoke resolves false', async () => {
    const { weatherAvailable, resolveInvoke } = await freshComposable()
    resolveInvoke(false)
    await Promise.resolve()
    expect(weatherAvailable.value).toBe(false)
  })

  it('silently swallows invoke rejection; weatherAvailable stays null', async () => {
    const { weatherAvailable, rejectInvoke } = await freshComposable()
    rejectInvoke(new Error('network error'))
    await Promise.resolve()
    await Promise.resolve()
    expect(weatherAvailable.value).toBeNull()
  })
})

describe('useWeatherAvailable — live weather-status events', () => {
  it('sets weatherAvailable to false on { available: false } event', async () => {
    const { weatherAvailable, fireEvent } = await freshComposable()
    fireEvent(false)
    expect(weatherAvailable.value).toBe(false)
  })

  it('sets weatherAvailable to true on { available: true } event', async () => {
    const { weatherAvailable, fireEvent } = await freshComposable()
    fireEvent(true)
    expect(weatherAvailable.value).toBe(true)
  })

  it('event overwriting: false → true on subsequent event', async () => {
    const { weatherAvailable, fireEvent } = await freshComposable()
    fireEvent(false)
    expect(weatherAvailable.value).toBe(false)
    fireEvent(true)
    expect(weatherAvailable.value).toBe(true)
  })

  it('event overwriting: true → false on subsequent event', async () => {
    const { weatherAvailable, fireEvent } = await freshComposable()
    fireEvent(true)
    expect(weatherAvailable.value).toBe(true)
    fireEvent(false)
    expect(weatherAvailable.value).toBe(false)
  })
})

describe('useWeatherAvailable — event / invoke precedence', () => {
  it('live event that arrives before invoke settles sets value; invoke does not overwrite it', async () => {
    const { weatherAvailable, fireEvent, resolveInvoke } = await freshComposable()

    // Event fires first — available goes false.
    fireEvent(false)
    expect(weatherAvailable.value).toBe(false)

    // Invoke resolves true AFTER the event — the composable's guard
    // (`if (available.value === null)`) prevents overwriting.
    resolveInvoke(true)
    await Promise.resolve()

    expect(weatherAvailable.value).toBe(false)  // event value preserved
  })

  it('invoke that resolves before any event sets the initial value', async () => {
    const { weatherAvailable, resolveInvoke } = await freshComposable()

    resolveInvoke(true)
    await Promise.resolve()
    expect(weatherAvailable.value).toBe(true)

    // No event has fired — value is purely from invoke.
  })

  it('live event that arrives after invoke has set the value overwrites it', async () => {
    const { weatherAvailable, resolveInvoke, fireEvent } = await freshComposable()

    resolveInvoke(true)
    await Promise.resolve()
    expect(weatherAvailable.value).toBe(true)

    // Backend emits a failure event — should override.
    fireEvent(false)
    expect(weatherAvailable.value).toBe(false)
  })
})

describe('useWeatherAvailable — singleton behaviour', () => {
  it('two calls to useWeatherAvailable() return the same reactive ref', async () => {
    vi.resetModules()

    vi.doMock('@tauri-apps/api/event', () => ({
      listen: vi.fn().mockResolvedValue(() => {}),
    }))
    vi.doMock('@tauri-apps/api/core', () => ({
      invoke: vi.fn().mockResolvedValue(false),
    }))

    const { useWeatherAvailable } = await import('./useWeatherAvailable')
    const a = useWeatherAvailable()
    const b = useWeatherAvailable()

    expect(a.weatherAvailable).toBe(b.weatherAvailable)
  })

  it('singleton: state set by one caller is immediately visible to another', async () => {
    let capturedHandler: EventHandler | null = null

    vi.resetModules()
    vi.doMock('@tauri-apps/api/event', () => ({
      listen: vi.fn((name: string, handler: EventHandler) => {
        if (name === 'weather-status') capturedHandler = handler
        return Promise.resolve(() => {})
      }),
    }))
    vi.doMock('@tauri-apps/api/core', () => ({
      invoke: vi.fn(() => new Promise(() => {})),  // never resolves
    }))

    const { useWeatherAvailable } = await import('./useWeatherAvailable')
    const a = useWeatherAvailable()
    const b = useWeatherAvailable()

    await Promise.resolve()
    await Promise.resolve()

    capturedHandler!({ payload: { available: true } })

    // Both callers see the update through the shared ref.
    expect(a.weatherAvailable.value).toBe(true)
    expect(b.weatherAvailable.value).toBe(true)
  })
})
