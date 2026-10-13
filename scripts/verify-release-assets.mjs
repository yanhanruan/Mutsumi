#!/usr/bin/env node
/**
 * verify-release-assets.mjs — release gate: verify that a GitHub Release
 * actually carries everything the auto-updater needs, and that the updater
 * manifest (latest.json) is internally consistent with the uploaded assets.
 *
 * This is the defense against the "partial success" failure mode: a release
 * that exists (so clients see an update) but whose installer / signature /
 * manifest is missing or mismatched (so every update attempt fails).
 *
 *   node scripts/verify-release-assets.mjs <tag> [--expect-version X.Y.Z]
 *     [--require-macos-universal]
 *
 * Env: GITHUB_TOKEN (required; drafts are invisible without it),
 *      GITHUB_REPOSITORY as owner/repo (defaults to yanhanruan/Mutsumi).
 *
 * Checks:
 *   1. the release for <tag> exists (drafts + prereleases included)
 *   2. Windows assets: exactly one NSIS `*-setup.exe`, its `*.sig` twin and
 *      `latest.json`, all non-empty
 *   3. latest.json: version/platform URLs match assets on THIS release, and
 *      every embedded signature exactly matches its uploaded `*.sig` file
 *   4. with --require-macos-universal: exactly one DMG plus one signed
 *      `.app.tar.gz` referenced by both darwin-aarch64 and darwin-x86_64
 *
 * Exits non-zero with a precise reason on the first failure.
 */

import {
  ReleaseContractError,
  validateReleaseContract,
} from './release-contract.mjs'

const tag = process.argv[2]
if (!tag || tag.startsWith('--')) {
  console.error('usage: node scripts/verify-release-assets.mjs <tag> [--expect-version X.Y.Z] [--require-macos-universal]')
  process.exit(1)
}
const evIdx = process.argv.indexOf('--expect-version')
if (evIdx !== -1 && (!process.argv[evIdx + 1] || process.argv[evIdx + 1].startsWith('--'))) {
  console.error('error: --expect-version requires an X.Y.Z value')
  process.exit(1)
}
const expectVersion =
  evIdx !== -1 ? process.argv[evIdx + 1]
  : /^v\d+\.\d+\.\d+$/.test(tag) ? tag.slice(1)
  : null
const requireMacosUniversal = process.argv.includes('--require-macos-universal')

const token = process.env.GITHUB_TOKEN
if (!token) {
  console.error('error: GITHUB_TOKEN is not set (required — draft releases are invisible without auth)')
  process.exit(1)
}
const repo = process.env.GITHUB_REPOSITORY ?? 'yanhanruan/Mutsumi'
const api = `https://api.github.com/repos/${repo}`
const headers = {
  authorization: `Bearer ${token}`,
  accept: 'application/vnd.github+json',
  'x-github-api-version': '2022-11-28',
}

function fail(msg) {
  console.error(`✗ ${msg}`)
  process.exit(1)
}
function ok(msg) {
  console.log(`✓ ${msg}`)
}

/** Download an asset's bytes. Drafts require the asset API + manual redirect
 *  handling: the Authorization header must NOT be forwarded to the storage
 *  backend the API redirects to, or the request is rejected. */
async function downloadAsset(asset) {
  const first = await fetch(asset.url, {
    headers: { ...headers, accept: 'application/octet-stream' },
    redirect: 'manual',
  })
  if (first.status >= 300 && first.status < 400) {
    const loc = first.headers.get('location')
    if (!loc) fail(`asset ${asset.name}: redirect without a Location header`)
    const second = await fetch(loc) // no auth header on the storage redirect
    if (!second.ok) fail(`asset ${asset.name}: download failed with ${second.status}`)
    return await second.text()
  }
  if (!first.ok) fail(`asset ${asset.name}: download failed with ${first.status}`)
  return await first.text()
}

// ── 1. Find the release (drafts included — listing sees them, /tags/ does not)
//
// A just-created draft is eventually consistent in the list endpoint: it shows
// up, but its `tag_name` field can briefly lag behind (the changelog step, run
// seconds earlier, can match by tag while this step still sees it blank). So we
// (a) retry the lookup, and (b) accept a draft whose release *name* carries the
// expected version as a fallback while tag_name settles. On final failure we
// dump every release we saw, so the cause is never a mystery again.

async function listReleases() {
  const res = await fetch(`${api}/releases?per_page=100`, { headers })
  if (!res.ok) fail(`could not list releases: ${res.status} ${await res.text()}`)
  return res.json()
}

let releases = []
let release
for (let attempt = 1; attempt <= 6; attempt++) {
  releases = await listReleases()
  release =
    // exact tag, then version tag with/without the `v`, then — since a draft's
    // tag_name can be blank in the list API — a draft whose release *name*
    // carries the version (releaseName is `Mutsumi-vX.Y.Z`).
    releases.find(r => r.tag_name === tag) ||
    (expectVersion && releases.find(r => r.tag_name === expectVersion || r.tag_name === `v${expectVersion}`)) ||
    (expectVersion && releases.find(r => r.draft && typeof r.name === 'string' && r.name.includes(expectVersion)))
  if (release) break
  if (attempt < 6) {
    console.log(`~ release "${tag}" not visible yet (attempt ${attempt}/6, saw ${releases.length}); retrying in 5s…`)
    await new Promise(r => setTimeout(r, 5000))
  }
}
if (!release) {
  console.error(`releases seen (${releases.length}):`)
  for (const r of releases) {
    console.error(`  - tag_name=${JSON.stringify(r.tag_name)} draft=${r.draft} prerelease=${r.prerelease} name=${JSON.stringify(r.name)}`)
  }
  fail(`no release found for tag "${tag}"${expectVersion ? ` (nor a draft named with "${expectVersion}")` : ''}`)
}
ok(`release found: "${release.name}" (draft=${release.draft}, prerelease=${release.prerelease}, tag=${JSON.stringify(release.tag_name)})`)

// ── 2. Download the manifest and signature assets ─────────────────

const assets = release.assets ?? []
const manifestAssets = assets.filter((asset) => asset.name === 'latest.json')
if (manifestAssets.length !== 1) {
  fail(`expected exactly one latest.json asset, found ${manifestAssets.length}`)
}
const manifestAsset = manifestAssets[0]

const manifestText = await downloadAsset(manifestAsset)
let manifest
try {
  manifest = JSON.parse(manifestText)
} catch (e) {
  fail(`latest.json is not valid JSON: ${e.message}`)
}

const signatureAssets = assets.filter((asset) => asset.name.endsWith('.sig'))
const signatureTexts = new Map(await Promise.all(
  signatureAssets.map(async (asset) => [asset.name, await downloadAsset(asset)]),
))

// ── 3. Pure release/updater contract ──────────────────────────────

try {
  const messages = validateReleaseContract({
    tag,
    expectVersion,
    expectedRepository: repo,
    assets,
    manifest,
    signatureTexts,
    requireMacosUniversal,
  })
  for (const message of messages) ok(message)
} catch (error) {
  if (error instanceof ReleaseContractError) fail(error.message)
  throw error
}

if (!expectVersion) {
  console.log(`~ skipping version match (tag "${tag}" is not vX.Y.Z and no --expect-version given); manifest says ${manifest.version}`)
}

console.log('')
console.log(`release "${tag}" passed ${requireMacosUniversal ? 'Windows + universal macOS' : 'Windows'} updater-contract checks`)
