#!/usr/bin/env node

// Tauri 2.11.2 notarizes the app, but only signs the outer DMG. Finish that
// container separately and replace the action's upload after the signed gate.
import { spawnSync } from 'node:child_process'
import { createReadStream, lstatSync, readdirSync } from 'node:fs'
import { basename, join, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'

import { validateManifestNormalizationTarget } from './release-contract.mjs'

export function parseDmgArgs(args) {
  const [phase, ...options] = args
  if (!['notarize', 'upload'].includes(phase)) throw new Error('phase must be notarize or upload')
  const allowed = phase === 'notarize'
    ? ['--bundle-dir']
    : ['--bundle-dir', '--release-id', '--tag', '--channel']
  const values = new Map()
  for (let index = 0; index < options.length; index += 2) {
    const option = options[index]
    const value = options[index + 1]
    if (!allowed.includes(option) || values.has(option) || !value || value.startsWith('--')) {
      throw new Error(`unknown, duplicate or incomplete argument: ${option}`)
    }
    values.set(option, value)
  }
  for (const option of allowed) {
    if (!values.has(option)) throw new Error(`missing required argument: ${option}`)
  }
  const parsed = { phase, bundleDir: values.get('--bundle-dir') }
  if (phase === 'upload') {
    const releaseId = Number(values.get('--release-id'))
    if (!/^\d+$/.test(values.get('--release-id')) || !Number.isSafeInteger(releaseId) || releaseId <= 0) {
      throw new Error('release ID must be a positive safe integer')
    }
    const tag = values.get('--tag')
    const channel = values.get('--channel')
    // Validate the tag/channel combination before any command or API request.
    validateManifestNormalizationTarget({
      release: { id: releaseId, tag_name: tag, draft: channel === 'production', prerelease: channel === 'staging' },
      releaseId, tag, channel,
    })
    Object.assign(parsed, { releaseId, tag, channel })
  }
  return parsed
}

function run(command, args) {
  const result = spawnSync(command, args, { encoding: 'utf8', maxBuffer: 10 * 1024 * 1024 })
  if (result.error || result.status !== 0) {
    // Tool diagnostics can contain credential identifiers. Do not echo them.
    throw new Error(`${command} ${args.slice(0, 2).join(' ')} failed (exit ${result.status ?? 'unavailable'})`)
  }
  return result.stdout
}

export function notarizeDmg(dmgPath, credentials, execute = run) {
  for (const name of ['APPLE_API_KEY_PATH', 'APPLE_API_KEY', 'APPLE_API_ISSUER']) {
    if (!credentials[name]) throw new Error(`missing notarization environment variable: ${name}`)
  }
  const output = execute('xcrun', [
    'notarytool', 'submit', dmgPath,
    '--key', credentials.APPLE_API_KEY_PATH,
    '--key-id', credentials.APPLE_API_KEY,
    '--issuer', credentials.APPLE_API_ISSUER,
    '--wait', '--timeout', '20m', '--output-format', 'json',
  ])
  let result
  try { result = JSON.parse(output) } catch { throw new Error('notarytool returned invalid JSON') }
  if (result?.status !== 'Accepted') throw new Error('DMG notarization was not accepted')
  execute('xcrun', ['stapler', 'staple', dmgPath])
  execute('xcrun', ['stapler', 'validate', dmgPath])
}

export async function uploadStapledDmg({
  dmgPath, size, repository, token, releaseId, tag, channel,
}, { execute = run, request = fetch, openBody = createReadStream } = {}) {
  if (!token) throw new Error('GITHUB_TOKEN is required')
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(repository ?? '')) {
    throw new Error('GITHUB_REPOSITORY must use owner/repo format')
  }
  if (!Number.isSafeInteger(size) || size <= 0) throw new Error('DMG must be a non-empty file')
  const name = basename(dmgPath)
  if (!/\.dmg$/i.test(name)) throw new Error('upload must be a DMG')
  execute('xcrun', ['stapler', 'validate', dmgPath])
  const headers = {
    authorization: `Bearer ${token}`,
    accept: 'application/vnd.github+json',
    'x-github-api-version': '2022-11-28',
  }
  const api = `https://api.github.com/repos/${repository}`
  const releaseResponse = await request(`${api}/releases/${releaseId}`, { headers })
  if (!releaseResponse.ok) throw new Error(`could not read release: HTTP ${releaseResponse.status}`)
  const release = await releaseResponse.json()
  validateManifestNormalizationTarget({ release, releaseId, tag, channel })
  const dmgs = (release.assets ?? []).filter((asset) => /\.dmg$/i.test(asset.name))
  if (dmgs.length !== 1 || dmgs[0].name !== name || !Number.isSafeInteger(dmgs[0].id) || dmgs[0].id <= 0) {
    throw new Error('release must contain exactly the expected DMG asset before replacement')
  }
  const removed = await request(`${api}/releases/assets/${dmgs[0].id}`, { method: 'DELETE', headers })
  if (removed.status !== 204) throw new Error(`could not replace DMG: delete HTTP ${removed.status}`)
  const uploadUrl = new URL(`https://uploads.github.com/repos/${repository}/releases/${releaseId}/assets`)
  uploadUrl.searchParams.set('name', name)
  const uploaded = await request(uploadUrl, {
    method: 'POST',
    headers: { ...headers, 'content-type': 'application/x-apple-diskimage', 'content-length': String(size) },
    body: openBody(dmgPath),
    duplex: 'half',
  })
  if (uploaded.status !== 201) throw new Error(`could not upload stapled DMG: HTTP ${uploaded.status}`)
  const asset = await uploaded.json()
  if (asset.name !== name || asset.size !== size || !Number.isSafeInteger(asset.id) || asset.id <= 0) {
    throw new Error('uploaded DMG response does not match the finalized file')
  }
}

async function main() {
  const args = parseDmgArgs(process.argv.slice(2))
  if (process.platform !== 'darwin') throw new Error('DMG finalization requires macOS')
  const dmgDir = resolve(args.bundleDir, 'dmg')
  const dmgs = readdirSync(dmgDir, { withFileTypes: true })
    .filter((entry) => entry.isFile() && /\.dmg$/i.test(entry.name))
  if (dmgs.length !== 1) throw new Error(`expected exactly one DMG, found ${dmgs.length}`)
  const dmgPath = join(dmgDir, dmgs[0].name)
  const size = lstatSync(dmgPath).size
  if (size <= 0) throw new Error('DMG is empty')
  if (args.phase === 'notarize') {
    notarizeDmg(dmgPath, process.env)
    console.log('DMG notarization accepted; stapled ticket validated.')
  } else {
    await uploadStapledDmg({
      ...args, dmgPath, size,
      repository: process.env.GITHUB_REPOSITORY,
      token: process.env.GITHUB_TOKEN,
    })
    console.log('Replaced the release DMG with the finalized, stapled file.')
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main().catch((error) => {
    console.error(`::error::${error.message}`)
    process.exitCode = 1
  })
}
