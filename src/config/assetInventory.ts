/**
 * assetInventory.ts — Phase 0 asset audit: verify the animation registry's
 * declared frame counts against the WebP files actually on disk.
 *
 * This is the first, deliberately non-invasive step of the renderer migration
 * (see docs/live2d-migration.md). It touches NO production rendering logic. It
 * exists to freeze and validate the current asset contract before anything is
 * abstracted or migrated: every `AnimationDef` in DEFAULT_ANIMATIONS points at
 * `/assets/<dir>/frame_001.webp … frame_<count>.webp`, and the playback engine
 * (useAnimator) trusts `count` to be exactly right — a stale `count` silently
 * drops the tail of an animation (missing frames just hold the last pose).
 *
 * Everything here is PURE: no fs, no DOM. The analysis takes a plain directory
 * listing (dir → filenames) plus the registry's expected {dir, count}, so it
 * runs deterministically in unit tests with hand-built inputs. The impure shell
 * that reads real files lives in the co-located *.live.test.ts, which feeds the
 * committed public/assets listing into auditInventory().
 */

// ── Frame filename parsing ──────────────────────────────────────────

/** Canonical frame filename, e.g. `frame_001.webp`. Capture group = index. */
const FRAME_RE = /^frame_(\d+)\.webp$/

/**
 * Parse a frame filename to its 1-based index, or null if it is not a frame
 * file (posters, .gitkeep, .DS_Store, …). Accepts any digit width — the loader
 * zero-pads to 3, but we parse leniently so an unpadded stray still surfaces as
 * a duplicate index rather than being silently ignored.
 */
export function parseFrameIndex(filename: string): number | null {
  const m = FRAME_RE.exec(filename)
  return m ? Number(m[1]) : null
}

// ── Types ───────────────────────────────────────────────────────────

/**
 * One expected animation directory derived from the registry. A single dir may
 * back several registry names (idle_low_* all reuse `idle`; fly_enter/fly_exit
 * share `fly_to_idle`), so `names` lists every registry key that resolves here.
 */
export interface ExpectedAnimation {
  dir:   string
  count: number
  names: string[]
}

/** Directory listing: dir → the filenames present in it (order irrelevant). */
export type DiskListing = Record<string, string[]>

/** Per-directory audit result. `ok` ⇔ disk matches the declared count exactly. */
export interface AnimationAudit {
  dir:            string
  names:          string[]
  declaredCount:  number
  /** Count of distinct frame indices found on disk. */
  actualCount:    number
  /** Sorted 1-based frame indices present on disk. */
  present:        number[]
  /** Indices in [1, declaredCount] with no matching file (gaps / short tail). */
  missing:        number[]
  /** Frame indices present on disk outside [1, declaredCount]. */
  extra:          number[]
  /** Indices that appear more than once (e.g. padded + unpadded collision). */
  duplicates:     number[]
  /** Non-frame files in the dir, ignored by the audit (informational). */
  ignored:        string[]
  ok:             boolean
}

/** Two registry entries share a dir but disagree on its frame count. */
export interface RegistryConflict {
  dir:    string
  names:  string[]
  counts: number[]
}

export interface InventoryReport {
  animations: AnimationAudit[]
  /** Dirs on disk referenced by no expected animation (informational). */
  orphanDirs: string[]
  /** Expected dirs entirely absent from the disk listing (blocking). */
  missingDirs: string[]
  /** Registry-internal disagreements about a shared dir's count (blocking). */
  registryConflicts: RegistryConflict[]
}

// ── Registry → expected inventory ───────────────────────────────────

/**
 * Collapse a registry (name → { dir, count }) into one ExpectedAnimation per
 * dir. Structurally typed on the minimum it needs, so tests can pass plain
 * objects and production can pass DEFAULT_ANIMATIONS directly. Distinct counts
 * for the same dir are surfaced by auditInventory as a RegistryConflict; here
 * we keep the smallest declared count so the "missing frames" check stays
 * conservative (never invents gaps the strictest entry wouldn't expect).
 */
export function expectedInventory(
  registry: Record<string, { dir: string; count: number }>,
): ExpectedAnimation[] {
  const byDir = new Map<string, ExpectedAnimation & { counts: Set<number> }>()
  for (const [name, def] of Object.entries(registry)) {
    const existing = byDir.get(def.dir)
    if (existing) {
      existing.names.push(name)
      existing.counts.add(def.count)
      existing.count = Math.min(existing.count, def.count)
    } else {
      byDir.set(def.dir, { dir: def.dir, count: def.count, names: [name], counts: new Set([def.count]) })
    }
  }
  return [...byDir.values()]
    .sort((a, b) => a.dir.localeCompare(b.dir))
    .map(({ dir, count, names }) => ({ dir, count, names: [...names].sort() }))
}

/** Recompute per-dir count disagreements from the raw registry (for reporting). */
function registryConflicts(
  registry: Record<string, { dir: string; count: number }>,
): RegistryConflict[] {
  const byDir = new Map<string, { names: string[]; counts: Set<number> }>()
  for (const [name, def] of Object.entries(registry)) {
    const e = byDir.get(def.dir) ?? { names: [], counts: new Set<number>() }
    e.names.push(name)
    e.counts.add(def.count)
    byDir.set(def.dir, e)
  }
  const conflicts: RegistryConflict[] = []
  for (const [dir, e] of byDir) {
    if (e.counts.size > 1) {
      conflicts.push({ dir, names: [...e.names].sort(), counts: [...e.counts].sort((a, b) => a - b) })
    }
  }
  return conflicts.sort((a, b) => a.dir.localeCompare(b.dir))
}

// ── Analysis ────────────────────────────────────────────────────────

/** Audit one directory's files against its declared frame count. Pure. */
export function analyzeAnimation(expected: ExpectedAnimation, files: string[]): AnimationAudit {
  const occurrences = new Map<number, number>()
  const ignored: string[] = []
  for (const f of files) {
    const idx = parseFrameIndex(f)
    if (idx === null) { ignored.push(f); continue }
    occurrences.set(idx, (occurrences.get(idx) ?? 0) + 1)
  }

  const present    = [...occurrences.keys()].sort((a, b) => a - b)
  const duplicates = present.filter(i => (occurrences.get(i) ?? 0) > 1)
  const extra      = present.filter(i => i < 1 || i > expected.count)
  const missing: number[] = []
  for (let i = 1; i <= expected.count; i++) if (!occurrences.has(i)) missing.push(i)

  const ok = missing.length === 0 && extra.length === 0 && duplicates.length === 0
    && present.length === expected.count

  return {
    dir:           expected.dir,
    names:         expected.names,
    declaredCount: expected.count,
    actualCount:   present.length,
    present,
    missing,
    extra,
    duplicates,
    ignored:       ignored.sort(),
    ok,
  }
}

/**
 * Full audit: every expected animation against the disk listing, plus
 * cross-checks for orphan dirs (present on disk, referenced by nobody),
 * missing dirs (expected, absent from disk), and registry-internal count
 * conflicts. Output ordering is deterministic (dirs sorted) so reports diff
 * cleanly. Pass `registry` to include RegistryConflict detection; omit it when
 * `expected` was already built by the caller.
 */
export function auditInventory(
  expected: ExpectedAnimation[],
  disk: DiskListing,
  registry?: Record<string, { dir: string; count: number }>,
): InventoryReport {
  const expectedByDir = new Map(expected.map(e => [e.dir, e]))

  const animations = [...expected]
    .sort((a, b) => a.dir.localeCompare(b.dir))
    .map(e => analyzeAnimation(e, disk[e.dir] ?? []))

  const missingDirs = expected
    .filter(e => disk[e.dir] === undefined)
    .map(e => e.dir)
    .sort()

  const orphanDirs = Object.keys(disk)
    .filter(dir => !expectedByDir.has(dir))
    .sort()

  return {
    animations,
    orphanDirs,
    missingDirs,
    registryConflicts: registry ? registryConflicts(registry) : [],
  }
}

// ── Reporting ───────────────────────────────────────────────────────

/**
 * Count issues that should block the migration from trusting the registry.
 * Orphan dirs are informational (e.g. `tarot/` holds card art, not frames) and
 * do NOT count. A dir with zero frame files but which IS expected counts as
 * blocking (all its declared frames are missing).
 */
export function blockingIssueCount(report: InventoryReport): number {
  return report.animations.filter(a => !a.ok).length
    + report.missingDirs.length
    + report.registryConflicts.length
}

/** Deterministic, human-readable summary of an InventoryReport. */
export function formatReport(report: InventoryReport): string {
  const lines: string[] = []
  const okCount = report.animations.filter(a => a.ok).length
  lines.push(`Asset inventory — ${okCount}/${report.animations.length} animations OK`)

  for (const a of report.animations) {
    if (a.ok) {
      lines.push(`  ✓ ${a.dir} (${a.declaredCount} frames)`)
      continue
    }
    const problems: string[] = []
    if (a.present.length === 0) problems.push('no frame files on disk')
    else if (a.actualCount !== a.declaredCount) problems.push(`count ${a.actualCount} ≠ declared ${a.declaredCount}`)
    if (a.missing.length)    problems.push(`missing [${summarizeRanges(a.missing)}]`)
    if (a.extra.length)      problems.push(`extra [${summarizeRanges(a.extra)}]`)
    if (a.duplicates.length) problems.push(`duplicates [${summarizeRanges(a.duplicates)}]`)
    lines.push(`  ✗ ${a.dir}: ${problems.join('; ')}`)
  }

  if (report.missingDirs.length) lines.push(`  ✗ missing dirs: ${report.missingDirs.join(', ')}`)
  if (report.registryConflicts.length) {
    for (const c of report.registryConflicts) {
      lines.push(`  ✗ registry conflict on ${c.dir}: ${c.names.join('/')} declare counts ${c.counts.join('/')}`)
    }
  }
  if (report.orphanDirs.length) lines.push(`  · orphan dirs (no registry entry): ${report.orphanDirs.join(', ')}`)

  return lines.join('\n')
}

/** Compress a sorted index list into `1-3,7,10-12` form for readable reports. */
export function summarizeRanges(indices: number[]): string {
  if (indices.length === 0) return ''
  const sorted = [...indices].sort((a, b) => a - b)
  const parts: string[] = []
  let start = sorted[0]
  let prev  = sorted[0]
  for (let i = 1; i <= sorted.length; i++) {
    const cur = sorted[i]
    if (cur === prev + 1) { prev = cur; continue }
    parts.push(start === prev ? `${start}` : `${start}-${prev}`)
    start = cur
    prev  = cur
  }
  return parts.join(',')
}
