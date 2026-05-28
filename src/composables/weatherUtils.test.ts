/**
 * Tests for classify() in weatherUtils.ts.
 *
 * classify() is the sole pure function in the weather feature that the
 * frontend depends on — it drives which SVG icon is rendered. Every WMO
 * code group and every notable gap (codes that should return 'unknown')
 * are tested here.
 *
 * Reference: https://open-meteo.com/en/docs (WMO weather interpretation codes)
 */
import { describe, it, expect } from 'vitest'
import { classify } from './weatherUtils'

// ── Clear ──────────────────────────────────────────────────────────

describe("classify — clear sky", () => {
  it("code 0 → 'clear'", () => expect(classify(0)).toBe('clear'))
})

// ── Partly cloudy ──────────────────────────────────────────────────

describe("classify — partly cloudy", () => {
  it("code 1 → 'partly-cloudy'", () => expect(classify(1)).toBe('partly-cloudy'))
  it("code 2 → 'partly-cloudy'", () => expect(classify(2)).toBe('partly-cloudy'))
})

// ── Overcast ───────────────────────────────────────────────────────

describe("classify — overcast / cloudy", () => {
  it("code 3 → 'cloudy'", () => expect(classify(3)).toBe('cloudy'))
})

// ── Fog ────────────────────────────────────────────────────────────

describe("classify — fog", () => {
  it("code 45 → 'fog'",         () => expect(classify(45)).toBe('fog'))
  it("code 48 → 'fog' (rime)",  () => expect(classify(48)).toBe('fog'))
})

// ── Drizzle (51-57) ────────────────────────────────────────────────

describe("classify — drizzle", () => {
  it("code 51 → 'drizzle' (light)",            () => expect(classify(51)).toBe('drizzle'))
  it("code 53 → 'drizzle' (moderate)",         () => expect(classify(53)).toBe('drizzle'))
  it("code 55 → 'drizzle' (dense)",            () => expect(classify(55)).toBe('drizzle'))
  it("code 56 → 'drizzle' (freezing light)",   () => expect(classify(56)).toBe('drizzle'))
  it("code 57 → 'drizzle' (freezing dense)",   () => expect(classify(57)).toBe('drizzle'))
})

// ── Rain (61-67) ───────────────────────────────────────────────────

describe("classify — rain", () => {
  it("code 61 → 'rain' (slight)",          () => expect(classify(61)).toBe('rain'))
  it("code 63 → 'rain' (moderate)",        () => expect(classify(63)).toBe('rain'))
  it("code 65 → 'rain' (heavy)",           () => expect(classify(65)).toBe('rain'))
  it("code 66 → 'rain' (freezing light)",  () => expect(classify(66)).toBe('rain'))
  it("code 67 → 'rain' (freezing heavy)",  () => expect(classify(67)).toBe('rain'))
})

// ── Snow (71-77) ───────────────────────────────────────────────────

describe("classify — snow", () => {
  it("code 71 → 'snow' (slight)",   () => expect(classify(71)).toBe('snow'))
  it("code 73 → 'snow' (moderate)", () => expect(classify(73)).toBe('snow'))
  it("code 75 → 'snow' (heavy)",    () => expect(classify(75)).toBe('snow'))
  it("code 77 → 'snow' (grains)",   () => expect(classify(77)).toBe('snow'))
})

// ── Rain showers (80-82) ───────────────────────────────────────────

describe("classify — rain showers", () => {
  it("code 80 → 'rain' (slight shower)",   () => expect(classify(80)).toBe('rain'))
  it("code 81 → 'rain' (moderate shower)", () => expect(classify(81)).toBe('rain'))
  it("code 82 → 'rain' (violent shower)",  () => expect(classify(82)).toBe('rain'))
})

// ── Snow showers (85-86) ───────────────────────────────────────────

describe("classify — snow showers", () => {
  it("code 85 → 'snow' (slight shower)", () => expect(classify(85)).toBe('snow'))
  it("code 86 → 'snow' (heavy shower)",  () => expect(classify(86)).toBe('snow'))
})

// ── Thunderstorm (95, 96, 99) ──────────────────────────────────────

describe("classify — thunderstorm", () => {
  it("code 95 → 'storm'",                    () => expect(classify(95)).toBe('storm'))
  it("code 96 → 'storm' (with slight hail)", () => expect(classify(96)).toBe('storm'))
  it("code 99 → 'storm' (with heavy hail)",  () => expect(classify(99)).toBe('storm'))
})

// ── Unknown / gap codes ────────────────────────────────────────────

describe("classify — unknown / undefined WMO codes", () => {
  // Codes with no WMO definition that fall into gaps between groups.
  it("code 4   → 'unknown' (gap: cloudy..fog)",          () => expect(classify(4)).toBe('unknown'))
  it("code 49  → 'unknown' (gap: fog..drizzle)",         () => expect(classify(49)).toBe('unknown'))
  it("code 50  → 'unknown' (gap: fog..drizzle)",         () => expect(classify(50)).toBe('unknown'))
  it("code 58  → 'unknown' (gap: drizzle..rain)",        () => expect(classify(58)).toBe('unknown'))
  it("code 60  → 'unknown' (gap: drizzle..rain)",        () => expect(classify(60)).toBe('unknown'))
  it("code 68  → 'unknown' (gap: rain..snow)",           () => expect(classify(68)).toBe('unknown'))
  it("code 70  → 'unknown' (gap: rain..snow)",           () => expect(classify(70)).toBe('unknown'))
  it("code 78  → 'unknown' (gap: snow..showers)",        () => expect(classify(78)).toBe('unknown'))
  it("code 79  → 'unknown' (gap: snow..showers)",        () => expect(classify(79)).toBe('unknown'))
  it("code 83  → 'unknown' (gap: showers..snow showers)",() => expect(classify(83)).toBe('unknown'))
  it("code 84  → 'unknown' (gap: showers..snow showers)",() => expect(classify(84)).toBe('unknown'))
  it("code 87  → 'unknown' (gap: snow showers..storm)",  () => expect(classify(87)).toBe('unknown'))
  it("code 94  → 'unknown' (gap: snow showers..storm)",  () => expect(classify(94)).toBe('unknown'))
  it("code 97  → 'unknown' (gap inside storm codes)",    () => expect(classify(97)).toBe('unknown'))
  it("code 98  → 'unknown' (gap inside storm codes)",    () => expect(classify(98)).toBe('unknown'))
  it("code 100 → 'unknown' (above all defined codes)",   () => expect(classify(100)).toBe('unknown'))
  it("code 9999 → 'unknown'",                            () => expect(classify(9999)).toBe('unknown'))
})
