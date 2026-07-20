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
 *
 * Env: GITHUB_TOKEN (required; drafts are invisible without it),
 *      GITHUB_REPOSITORY as owner/repo (defaults to yanhanruan/Mutsumi).
 *
 * Checks:
 *   1. the release for <tag> exists (drafts + prereleases included)
 *   2. assets: exactly one NSIS `*-setup.exe`, its `*.sig` twin, `latest.json`,
 *      all non-empty
 *   3. latest.json contract: version matches the tag / --expect-version,
 *      platforms."windows-x86_64" present, non-empty signature, and its url
 *      points at an asset that was actually uploaded to THIS release
 *
 * Exits non-zero with a precise reason on the first failure.
 */

const tag = process.argv[2]
if (!tag || tag.startsWith('--')) {
  console.error('usage: node scripts/verify-release-assets.mjs <tag> [--expect-version X.Y.Z]')
  process.exit(1)
}
const evIdx = process.argv.indexOf('--expect-version')
const expectVersion =
  evIdx !== -1 ? process.argv[evIdx + 1]
  : /^v\d+\.\d+\.\d+$/.test(tag) ? tag.slice(1)
  : null

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

// ── 2. Asset completeness ──────────────────────────────────────────

const assets = release.assets ?? []
const names = assets.map(a => a.name)

const installers = assets.filter(a => /-setup\.exe$/i.test(a.name))
if (installers.length !== 1) {
  fail(`expected exactly one *-setup.exe asset, found ${installers.length}: [${names.join(', ')}]`)
}
const installer = installers[0]
ok(`installer: ${installer.name} (${(installer.size / 1024 / 1024).toFixed(1)} MB)`)

const sig = assets.find(a => a.name === `${installer.name}.sig`)
if (!sig) fail(`missing signature asset "${installer.name}.sig"`)
ok(`signature: ${sig.name}`)

const manifestAsset = assets.find(a => a.name === 'latest.json')
if (!manifestAsset) fail('missing "latest.json" updater manifest asset')

for (const a of [installer, sig, manifestAsset]) {
  if (a.size === 0) fail(`asset ${a.name} is empty (0 bytes) — a partial upload`)
}

// ── 3. latest.json contract ────────────────────────────────────────

const manifestText = await downloadAsset(manifestAsset)
let manifest
try {
  manifest = JSON.parse(manifestText)
} catch (e) {
  fail(`latest.json is not valid JSON: ${e.message}`)
}

if (expectVersion) {
  if (manifest.version !== expectVersion) {
    fail(`latest.json version "${manifest.version}" != expected "${expectVersion}" (from ${evIdx !== -1 ? '--expect-version' : 'the tag'})`)
  }
  ok(`manifest version matches: ${manifest.version}`)
} else {
  console.log(`~ skipping version match (tag "${tag}" is not vX.Y.Z and no --expect-version given); manifest says ${manifest.version}`)
}

const win = manifest.platforms?.['windows-x86_64']
if (!win) fail(`latest.json has no platforms."windows-x86_64" entry (platforms: ${JSON.stringify(Object.keys(manifest.platforms ?? {}))})`)
ok('windows-x86_64 platform present')

if (typeof win.signature !== 'string' || win.signature.trim().length < 100) {
  fail('windows-x86_64.signature is missing or suspiciously short — was the build signed?')
}
ok('signature field is non-empty')

if (typeof win.url !== 'string' || !win.url.includes('/releases/download/')) {
  fail(`windows-x86_64.url does not look like a release download URL: ${win.url}`)
}
const urlName = decodeURIComponent(win.url.split('/').pop() ?? '')
if (!names.includes(urlName)) {
  fail(`latest.json points at "${urlName}" but that asset is NOT in this release: [${names.join(', ')}]`)
}
if (urlName !== installer.name) {
  fail(`latest.json points at "${urlName}" but the uploaded installer is "${installer.name}"`)
}
ok(`manifest url resolves to the uploaded installer (${urlName})`)

// The url's tag path segment must be the version tag, so that once the draft is
// published the baked /releases/download/<tag>/... URL resolves. NOTE: this only
// checks the *string* — at draft time the assets themselves still live under an
// `untagged-*` placeholder, so we can't verify the URL actually resolves here.
// That final check happens post-publish in scripts/heal-published-release.mjs.
const expectedTagSeg =
  /^v\d+\.\d+\.\d+$/.test(tag) ? tag : expectVersion ? `v${expectVersion}` : null
if (expectedTagSeg && !win.url.includes(`/releases/download/${expectedTagSeg}/`)) {
  fail(`latest.json url is not on the /releases/download/${expectedTagSeg}/ path: ${win.url}`)
}
if (expectedTagSeg) ok(`manifest url targets the ${expectedTagSeg} tag path`)

console.log('')
console.log(`release "${tag}" passed all updater-contract checks`)
