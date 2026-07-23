/**
 * Tests for the pure asset-inventory audit (src/config/assetInventory.ts).
 *
 * All inputs are hand-built directory listings — no filesystem — so the suite
 * is deterministic and reproducible (Phase 0.1 requirement). The audit of the
 * real committed assets lives separately in assetInventory.live.test.ts.
 */
import { describe, it, expect } from 'vitest'
import {
  analyzeAnimation,
  auditInventory,
  blockingIssueCount,
  expectedInventory,
  formatReport,
  parseFrameIndex,
  summarizeRanges,
  type ExpectedAnimation,
} from './assetInventory'

// ── helpers ────────────────────────────────────────────────────────

/** Contiguous frame filenames frame_001.webp … frame_NNN.webp. */
function frames(n: number): string[] {
  return Array.from({ length: n }, (_, i) => `frame_${String(i + 1).padStart(3, '0')}.webp`)
}

const anim = (dir: string, count: number, names = [dir]): ExpectedAnimation => ({ dir, count, names })

// ── parseFrameIndex ────────────────────────────────────────────────

describe('parseFrameIndex()', () => {
  it('parses a padded frame filename to its 1-based index', () => {
    expect(parseFrameIndex('frame_001.webp')).toBe(1)
    expect(parseFrameIndex('frame_042.webp')).toBe(42)
    expect(parseFrameIndex('frame_426.webp')).toBe(426)
  })

  it('parses an unpadded frame filename too (surfaces stray files)', () => {
    expect(parseFrameIndex('frame_7.webp')).toBe(7)
  })

  it('returns null for non-frame files', () => {
    expect(parseFrameIndex('.gitkeep')).toBeNull()
    expect(parseFrameIndex('poster.webp')).toBeNull()
    expect(parseFrameIndex('frame_001.png')).toBeNull()
    expect(parseFrameIndex('frame_.webp')).toBeNull()
    expect(parseFrameIndex('thumb.png')).toBeNull()
  })
})

// ── analyzeAnimation ───────────────────────────────────────────────

describe('analyzeAnimation()', () => {
  it('flags a complete, contiguous sequence as ok', () => {
    const a = analyzeAnimation(anim('idle', 426), frames(426))
    expect(a.ok).toBe(true)
    expect(a.actualCount).toBe(426)
    expect(a.missing).toEqual([])
    expect(a.extra).toEqual([])
    expect(a.duplicates).toEqual([])
  })

  it('detects a short tail (declared count larger than disk)', () => {
    const a = analyzeAnimation(anim('sleep', 192), frames(190))
    expect(a.ok).toBe(false)
    expect(a.actualCount).toBe(190)
    expect(a.missing).toEqual([191, 192])
  })

  it('detects an interior gap (missing frame)', () => {
    const files = frames(10).filter(f => f !== 'frame_005.webp')
    const a = analyzeAnimation(anim('click', 10), files)
    expect(a.ok).toBe(false)
    expect(a.missing).toEqual([5])
  })

  it('detects extra frames beyond the declared count', () => {
    const a = analyzeAnimation(anim('music1', 5), frames(7))
    expect(a.ok).toBe(false)
    expect(a.extra).toEqual([6, 7])
    expect(a.missing).toEqual([])
  })

  it('detects duplicate indices (padded + unpadded collision)', () => {
    const a = analyzeAnimation(anim('idle', 3), ['frame_001.webp', 'frame_1.webp', 'frame_002.webp', 'frame_003.webp'])
    expect(a.duplicates).toEqual([1])
    expect(a.ok).toBe(false)
  })

  it('ignores non-frame files without failing an otherwise-complete dir', () => {
    const a = analyzeAnimation(anim('idle', 3), [...frames(3), 'poster.webp', '.DS_Store'])
    expect(a.ok).toBe(true)
    expect(a.ignored).toEqual(['.DS_Store', 'poster.webp'])
  })

  it('reports an empty dir as all-missing', () => {
    const a = analyzeAnimation(anim('idle', 3), [])
    expect(a.ok).toBe(false)
    expect(a.actualCount).toBe(0)
    expect(a.missing).toEqual([1, 2, 3])
  })
})

// ── expectedInventory ──────────────────────────────────────────────

describe('expectedInventory()', () => {
  it('collapses registry names that share a directory into one entry', () => {
    const registry = {
      idle:               { dir: 'idle', count: 426 },
      idle_low_energy:    { dir: 'idle', count: 426 },
      idle_low_affection: { dir: 'idle', count: 426 },
      click:              { dir: 'click_matched', count: 156 },
    }
    const inv = expectedInventory(registry)
    expect(inv).toHaveLength(2)
    const idle = inv.find(e => e.dir === 'idle')!
    expect(idle.names).toEqual(['idle', 'idle_low_affection', 'idle_low_energy'])
    expect(idle.count).toBe(426)
  })

  it('is sorted by dir for deterministic output', () => {
    const registry = {
      zed: { dir: 'zed', count: 1 },
      abe: { dir: 'abe', count: 1 },
    }
    expect(expectedInventory(registry).map(e => e.dir)).toEqual(['abe', 'zed'])
  })

  it('keeps the smallest count when a shared dir declares conflicting counts', () => {
    const registry = {
      a: { dir: 'shared', count: 185 },
      b: { dir: 'shared', count: 180 },
    }
    expect(expectedInventory(registry)[0].count).toBe(180)
  })
})

// ── auditInventory ─────────────────────────────────────────────────

describe('auditInventory()', () => {
  it('audits every expected dir against the disk listing', () => {
    const expected = [anim('idle', 3), anim('click', 2)]
    const disk = { idle: frames(3), click: frames(2) }
    const report = auditInventory(expected, disk)
    expect(report.animations.every(a => a.ok)).toBe(true)
    expect(blockingIssueCount(report)).toBe(0)
  })

  it('flags a dir present on disk but referenced by no animation as an orphan', () => {
    const expected = [anim('idle', 3)]
    const disk = { idle: frames(3), tarot: ['card_00.webp'] }
    const report = auditInventory(expected, disk)
    expect(report.orphanDirs).toEqual(['tarot'])
    // Orphans are informational — not blocking.
    expect(blockingIssueCount(report)).toBe(0)
  })

  it('flags an expected dir absent from disk as a missing (blocking) dir', () => {
    const expected = [anim('idle', 3), anim('sleep', 5)]
    const disk = { idle: frames(3) }
    const report = auditInventory(expected, disk)
    expect(report.missingDirs).toEqual(['sleep'])
    expect(blockingIssueCount(report)).toBeGreaterThan(0)
  })

  it('detects a registry conflict when a shared dir declares two counts', () => {
    const registry = {
      fly_enter: { dir: 'fly_to_idle', count: 185 },
      fly_exit:  { dir: 'fly_to_idle', count: 180 },
    }
    const report = auditInventory(expectedInventory(registry), { fly_to_idle: frames(185) }, registry)
    expect(report.registryConflicts).toEqual([
      { dir: 'fly_to_idle', names: ['fly_enter', 'fly_exit'], counts: [180, 185] },
    ])
    expect(blockingIssueCount(report)).toBeGreaterThan(0)
  })

  it('produces deterministic ordering (dirs sorted)', () => {
    const expected = [anim('zed', 1), anim('abe', 1)]
    const disk = { zed: frames(1), abe: frames(1) }
    const report = auditInventory(expected, disk)
    expect(report.animations.map(a => a.dir)).toEqual(['abe', 'zed'])
  })
})

// ── summarizeRanges ────────────────────────────────────────────────

describe('summarizeRanges()', () => {
  it('collapses consecutive runs into ranges', () => {
    expect(summarizeRanges([1, 2, 3, 7, 10, 11, 12])).toBe('1-3,7,10-12')
  })
  it('handles singletons and empty input', () => {
    expect(summarizeRanges([5])).toBe('5')
    expect(summarizeRanges([])).toBe('')
  })
  it('sorts before summarizing', () => {
    expect(summarizeRanges([3, 1, 2])).toBe('1-3')
  })
})

// ── formatReport ───────────────────────────────────────────────────

describe('formatReport()', () => {
  it('renders an all-OK report with a header and ✓ lines', () => {
    const report = auditInventory([anim('idle', 3)], { idle: frames(3) })
    const text = formatReport(report)
    expect(text).toContain('1/1 animations OK')
    expect(text).toContain('✓ idle (3 frames)')
  })

  it('renders problems for a failing dir', () => {
    const report = auditInventory([anim('sleep', 5)], { sleep: frames(3) })
    const text = formatReport(report)
    expect(text).toContain('✗ sleep')
    expect(text).toContain('missing [4-5]')
  })
})
