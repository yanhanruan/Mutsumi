#!/usr/bin/env node
/**
 * heal-published-release.mjs — post-publish gate + self-heal for the updater.
 *
 * Runs AFTER a release is published (the `release: published` event, i.e. the
 * moment a human clicks "Publish release") or on manual `workflow_dispatch`.
 * It guarantees the one thing the pre-publish gate cannot: that the installer
 * URL the in-app updater will actually fetch resolves for real end users.
 *
 * Why this exists
 * ---------------
 * `tauri-action` creates a DRAFT release parked on an `untagged-<hash>`
 * placeholder ref, and bakes latest.json's installer URL from the *tag*
 * (`/releases/download/vX.Y.Z/...`), assuming publish will move the assets onto
 * that tag path. But our release tag is pushed *first* — it's the CI trigger —
 * so publishing the draft can leave it orphaned on the `untagged-*` ref instead
 * of binding to `vX.Y.Z`. Result: `latest.json` version-check succeeds, but its
 * baked download URL 404s, and every client's update fails. (This bit v1.5.0
 * and v1.5.1.) See docs/RELEASING.md.
 *
 * The pre-publish gate (scripts/verify-release-assets.mjs) can't catch this: at
 * draft time the assets legitimately live under the `untagged-*` path while
 * latest.json already points at the tag path, and the URL only settles on
 * publish. So the check has to happen *after* publish — here.
 *
 * What it does
 * ------------
 *   1. Resolve the target release (by --release-id from the event, or --version).
 *   2. Read its latest.json → the exact installer URL clients will fetch.
 *   3. Enforce the invariant: a published updater release must be bound to its
 *      version tag `vX.Y.Z`. If it's bound to anything else (e.g. `untagged-*`),
 *      rebind it via PATCH — the same fix as editing the tag in the UI.
 *   4. Verify the live URL actually resolves (single-byte range GET). Fail the
 *      job loudly if it still doesn't, so a broken release can't go unnoticed.
 *
 * Env:  GITHUB_TOKEN (contents: write — needed to rebind), GITHUB_REPOSITORY.
 * Args: --release-id <id> | --version <X.Y.Z>
 */

const repo = process.env.GITHUB_REPOSITORY ?? 'yanhanruan/Mutsumi'
const token = process.env.GITHUB_TOKEN
if (!token) {
  console.error('error: GITHUB_TOKEN is not set (required to read + rebind the release)')
  process.exit(1)
}

const api = `https://api.github.com/repos/${repo}`
const headers = {
  authorization: `Bearer ${token}`,
  accept: 'application/vnd.github+json',
  'x-github-api-version': '2022-11-28',
}

const args = process.argv.slice(2)
const argVal = (name) => {
  const i = args.indexOf(name)
  return i !== -1 ? args[i + 1] : null
}
const releaseIdArg = argVal('--release-id')
const versionArg = argVal('--version')?.replace(/^v/, '') ?? null

const fail = (msg) => { console.error(`✗ ${msg}`); process.exit(1) }
const ok = (msg) => console.log(`✓ ${msg}`)

async function ghFetch(path, init) {
  return fetch(`${api}${path}`, { headers, ...init })
}

// ── Resolve the release ────────────────────────────────────────────
// From the event we get an unambiguous id. For manual dispatch we get a
// version; match it the robust way (tag, or release name / installer asset that
// carries the version), since the tag_name may be the broken `untagged-*`.
async function resolveRelease() {
  if (releaseIdArg) {
    const res = await ghFetch(`/releases/${releaseIdArg}`)
    if (!res.ok) fail(`could not fetch release id ${releaseIdArg}: ${res.status} ${await res.text()}`)
    return res.json()
  }
  if (!versionArg) fail('provide --release-id <id> or --version <X.Y.Z>')
  const res = await ghFetch(`/releases?per_page=100`)
  if (!res.ok) fail(`could not list releases: ${res.status} ${await res.text()}`)
  const releases = await res.json()
  const match = releases.find((r) =>
    r.tag_name === `v${versionArg}` ||
    (typeof r.name === 'string' && r.name.includes(versionArg)) ||
    (r.assets ?? []).some((a) => a.name.includes(`_${versionArg}_`)),
  )
  if (!match) fail(`no release found for version ${versionArg}`)
  return match
}

// Download an asset's text. Drafts/redirects need the asset API + manual
// redirect handling: the Authorization header must NOT follow to the storage
// backend, or it's rejected. Mirrors verify-release-assets.mjs.
async function downloadAssetText(asset) {
  const first = await fetch(asset.url, {
    headers: { ...headers, accept: 'application/octet-stream' },
    redirect: 'manual',
  })
  if (first.status >= 300 && first.status < 400) {
    const loc = first.headers.get('location')
    if (!loc) fail(`asset ${asset.name}: redirect without a Location header`)
    const second = await fetch(loc) // no auth header on the storage redirect
    if (!second.ok) fail(`asset ${asset.name}: download failed with ${second.status}`)
    return second.text()
  }
  if (!first.ok) fail(`asset ${asset.name}: download failed with ${first.status}`)
  return first.text()
}

// Does the public installer URL resolve for an unauthenticated client? Ask for
// a single byte so we never pull the whole installer; GitHub answers 206 (or
// 200), and a wrong /releases/download/<tag>/ path answers 404.
async function urlResolves(url) {
  try {
    const res = await fetch(url, { headers: { range: 'bytes=0-0' } })
    return res.status === 200 || res.status === 206
  } catch (e) {
    console.log(`~ request error probing ${url}: ${e.message}`)
    return false
  }
}

// ── Main ───────────────────────────────────────────────────────────
let release = await resolveRelease()
ok(`release: "${release.name}" (id=${release.id}, draft=${release.draft}, tag=${JSON.stringify(release.tag_name)})`)

if (release.draft) {
  // Nothing is public yet — the pre-publish gate owns drafts.
  console.log('~ release is still a draft; the post-publish gate has nothing to verify yet. Skipping.')
  process.exit(0)
}

const manifestAsset = (release.assets ?? []).find((a) => a.name === 'latest.json')
if (!manifestAsset) {
  console.log('~ no latest.json asset — not an updater release; nothing to gate.')
  process.exit(0)
}

let manifest
try {
  manifest = JSON.parse(await downloadAssetText(manifestAsset))
} catch (e) {
  fail(`latest.json is not valid JSON: ${e.message}`)
}
const version = manifest.version
const win = manifest.platforms?.['windows-x86_64']
if (!win?.url) fail('latest.json has no platforms."windows-x86_64".url')
ok(`manifest version ${version}; installer URL: ${win.url}`)

const expectedTag = `v${version}`

// Invariant: a published updater release must be bound to its version tag, so
// latest.json's baked /releases/download/vX.Y.Z/... URL resolves for clients.
if (release.tag_name !== expectedTag) {
  console.log(`! release is bound to "${release.tag_name}", not "${expectedTag}" — rebinding…`)
  const res = await ghFetch(`/releases/${release.id}`, {
    method: 'PATCH',
    body: JSON.stringify({ tag_name: expectedTag }),
  })
  if (!res.ok) fail(`rebind failed: ${res.status} ${await res.text()}`)
  release = await res.json()
  ok(`rebound release ${release.id} → tag ${JSON.stringify(release.tag_name)}`)
} else {
  ok(`release already bound to ${expectedTag}`)
}

// Final contract: the exact URL clients will hit must resolve. Asset re-hosting
// can lag a beat after a rebind, so retry a few times before giving up.
for (let attempt = 0; attempt <= 5; attempt++) {
  if (await urlResolves(win.url)) {
    ok(`updater download resolves: ${win.url}`)
    console.log('\npost-publish gate passed.')
    process.exit(0)
  }
  if (attempt < 5) {
    console.log(`~ not resolving yet (attempt ${attempt + 1}/6); retrying in 3s…`)
    await new Promise((r) => setTimeout(r, 3000))
  }
}

fail(
  `updater download URL still does not resolve: ${win.url}\n` +
    `  The release is bound to tag "${release.tag_name}". Clients on the previous\n` +
    `  version will get a 404 when updating. Inspect the release + tag binding.`,
)
