#!/usr/bin/env node

/**
 * Convert tauri-action v1 API asset URLs in latest.json to stable, public
 * GitHub Release download URLs before the final release contract runs.
 *
 * Usage:
 *   node scripts/normalize-release-manifest.mjs <tag> --release-id <id> \
 *     --channel <production|staging>
 *
 * Env: GITHUB_TOKEN, GITHUB_REPOSITORY (defaults to yanhanruan/Mutsumi).
 */

import {
  normalizeUpdaterAssetUrls,
  ReleaseContractError,
  validateManifestNormalizationTarget,
} from './release-contract.mjs'

const args = process.argv.slice(2)
const tag = args[0]
const releaseIdIndex = args.indexOf('--release-id')
const releaseIdText = releaseIdIndex === -1 ? null : args[releaseIdIndex + 1]
const channelIndex = args.indexOf('--channel')
const channel = channelIndex === -1 ? null : args[channelIndex + 1]
const knownArguments = new Set([
  0,
  releaseIdIndex,
  releaseIdIndex + 1,
  channelIndex,
  channelIndex + 1,
])
if (
  !tag ||
  tag.startsWith('--') ||
  releaseIdIndex === -1 ||
  !releaseIdText ||
  !/^\d+$/.test(releaseIdText) ||
  channelIndex === -1 ||
  !['production', 'staging'].includes(channel) ||
  args.some((_, index) => !knownArguments.has(index))
) {
  console.error(
    'usage: node scripts/normalize-release-manifest.mjs <tag> ' +
    '--release-id <id> --channel <production|staging>',
  )
  process.exit(1)
}

const token = process.env.GITHUB_TOKEN
if (!token) {
  console.error('error: GITHUB_TOKEN is required to normalize a draft/prerelease manifest')
  process.exit(1)
}
const repository = process.env.GITHUB_REPOSITORY ?? 'yanhanruan/Mutsumi'
if (!/^[^/]+\/[^/]+$/.test(repository)) {
  console.error(`error: GITHUB_REPOSITORY must use owner/repo format, found "${repository}"`)
  process.exit(1)
}

const releaseId = Number(releaseIdText)
if (!Number.isSafeInteger(releaseId) || releaseId <= 0) {
  console.error(`error: release ID is outside the safe integer range: ${releaseIdText}`)
  process.exit(1)
}

const api = `https://api.github.com/repos/${repository}`
const apiHeaders = {
  authorization: `Bearer ${token}`,
  accept: 'application/vnd.github+json',
  'x-github-api-version': '2022-11-28',
}

function fail(message) {
  console.error(`✗ ${message}`)
  process.exit(1)
}

async function apiRequest(path, init = {}) {
  return fetch(`${api}${path}`, {
    ...init,
    headers: { ...apiHeaders, ...init.headers },
  })
}

async function downloadAssetText(asset) {
  const first = await fetch(asset.url, {
    headers: { ...apiHeaders, accept: 'application/octet-stream' },
    redirect: 'manual',
  })
  if (first.status >= 300 && first.status < 400) {
    const location = first.headers.get('location')
    if (!location) fail(`asset ${asset.name}: redirect without a Location header`)
    const second = await fetch(location)
    if (!second.ok) fail(`asset ${asset.name}: download failed with ${second.status}`)
    return second.text()
  }
  if (!first.ok) fail(`asset ${asset.name}: download failed with ${first.status}`)
  return first.text()
}

const releaseResponse = await apiRequest(`/releases/${releaseId}`)
if (!releaseResponse.ok) {
  fail(`could not fetch release ${releaseId}: ${releaseResponse.status} ${await releaseResponse.text()}`)
}
const release = await releaseResponse.json()
try {
  validateManifestNormalizationTarget({ release, releaseId, tag, channel })
} catch (error) {
  if (error instanceof ReleaseContractError) fail(error.message)
  throw error
}
const assets = release.assets ?? []
const manifestAssets = assets.filter((asset) => asset.name === 'latest.json')
if (manifestAssets.length !== 1) {
  fail(`expected exactly one latest.json asset, found ${manifestAssets.length}`)
}
const manifestAsset = manifestAssets[0]

let manifest
try {
  manifest = JSON.parse(await downloadAssetText(manifestAsset))
} catch (error) {
  fail(`latest.json is not valid JSON: ${error.message}`)
}

let normalized
try {
  normalized = normalizeUpdaterAssetUrls({
    manifest,
    assets,
    tag,
    expectedRepository: repository,
  })
} catch (error) {
  if (error instanceof ReleaseContractError) fail(error.message)
  throw error
}

if (normalized.rewrittenCount === 0) {
  console.log('✓ latest.json already uses canonical public GitHub Release URLs')
  process.exit(0)
}

const body = `${JSON.stringify(normalized.manifest, null, 2)}\n`
const deleteResponse = await apiRequest(`/releases/assets/${manifestAsset.id}`, { method: 'DELETE' })
if (deleteResponse.status !== 204) {
  fail(`could not remove the API-URL manifest: ${deleteResponse.status} ${await deleteResponse.text()}`)
}

const uploadUrl = new URL(`https://uploads.github.com/repos/${repository}/releases/${releaseId}/assets`)
uploadUrl.searchParams.set('name', 'latest.json')
const uploadResponse = await fetch(uploadUrl, {
  method: 'POST',
  headers: {
    ...apiHeaders,
    'content-type': 'application/json',
  },
  body,
})
if (uploadResponse.status !== 201) {
  fail(`could not upload the normalized manifest: ${uploadResponse.status} ${await uploadResponse.text()}`)
}

console.log(`✓ normalized ${normalized.rewrittenCount} updater platform URL(s) to public release downloads`)
