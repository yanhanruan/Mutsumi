/**
 * Live asset audit — the impure shell around the pure auditor.
 *
 * Reads the real, committed public/assets directory listing and runs the
 * registry (DEFAULT_ANIMATIONS) against it. This is the Phase 0.1 "scanner":
 * running `npm test` prints the inventory report and fails the suite if the
 * registry ever drifts from disk (a stale `count`, a missing frame, an orphan
 * that turns out to be referenced, …) — turning a silent-tail-drop bug into a
 * red test.
 *
 * Deterministic because the assets are checked into git. Reads only directory
 * listings (not frame bytes), so it stays fast despite ~2.5k files.
 */
/// <reference types="node" />
import { describe, it, expect } from 'vitest'
import { readdirSync, statSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'
import { DEFAULT_ANIMATIONS } from './animations'
import {
  auditInventory,
  blockingIssueCount,
  expectedInventory,
  formatReport,
  type DiskListing,
} from './assetInventory'

const assetsRoot = join(dirname(fileURLToPath(import.meta.url)), '..', '..', 'public', 'assets')

/** Build a { dir → filenames } listing of every immediate subdir of public/assets. */
function readDiskListing(): DiskListing {
  const listing: DiskListing = {}
  for (const entry of readdirSync(assetsRoot)) {
    const full = join(assetsRoot, entry)
    if (statSync(full).isDirectory()) listing[entry] = readdirSync(full)
  }
  return listing
}

describe('live asset inventory (public/assets vs DEFAULT_ANIMATIONS)', () => {
  const disk     = readDiskListing()
  const expected = expectedInventory(DEFAULT_ANIMATIONS)
  const report   = auditInventory(expected, disk, DEFAULT_ANIMATIONS)

  it('reports no blocking issues (registry matches disk)', () => {
    // Print the full report so `npm test` doubles as the audit scanner.
    // eslint-disable-next-line no-console
    console.log('\n' + formatReport(report) + '\n')
    expect(blockingIssueCount(report), formatReport(report)).toBe(0)
  })

  it('every registry animation directory exists on disk', () => {
    expect(report.missingDirs).toEqual([])
  })

  it('has no registry-internal count conflicts on shared directories', () => {
    expect(report.registryConflicts).toEqual([])
  })
})
