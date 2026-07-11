#!/usr/bin/env node
/**
 * make-latest-json.mjs — build a `latest.json` updater manifest for the local
 * fake update server (scripts/fake-update-server.mjs) from a built staging
 * installer and its .sig file.
 *
 *   node scripts/make-latest-json.mjs \
 *     --installer src-tauri/target/release/bundle/nsis/mutsumi_1.5.0_x64-setup.exe \
 *     --version 1.5.0 \
 *     [--out scripts/fake-update-dist/latest.json] \
 *     [--base-url http://localhost:9414/download] \
 *     [--notes "..."]
 *
 * The manifest matches what tauri-action generates for real releases:
 * { version, notes, pub_date, platforms: { "windows-x86_64": { signature, url } } }.
 * Copies the installer + .sig next to the manifest so the fake server can
 * serve everything from one directory.
 */
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { basename, dirname, join, resolve } from 'node:path'

function arg(name, fallback = null) {
  const i = process.argv.indexOf(`--${name}`)
  return i !== -1 && process.argv[i + 1] !== undefined ? process.argv[i + 1] : fallback
}

const installer = arg('installer')
const version = arg('version')
if (!installer || !version) {
  console.error('usage: node scripts/make-latest-json.mjs --installer <setup.exe> --version <X.Y.Z> [--out <latest.json>] [--base-url <url>] [--notes <text>]')
  process.exit(1)
}

const installerPath = resolve(installer)
const sigPath = `${installerPath}.sig`
const out = resolve(arg('out', join('scripts', 'fake-update-dist', 'latest.json')))
const baseUrl = arg('base-url', 'http://localhost:9414/download').replace(/\/$/, '')
const notes = arg('notes', `Staging test build v${version}`)

let signature
try {
  signature = readFileSync(sigPath, 'utf8').trim()
} catch {
  console.error(`error: missing signature file ${sigPath}`)
  console.error('build with the signing env vars set (see scripts/gen-test-keys.mjs output) so the .sig is produced')
  process.exit(1)
}

const name = basename(installerPath)
const manifest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    'windows-x86_64': {
      signature,
      url: `${baseUrl}/${name}`,
    },
  },
}

const outDir = dirname(out)
mkdirSync(outDir, { recursive: true })
writeFileSync(out, JSON.stringify(manifest, null, 2) + '\n')
copyFileSync(installerPath, join(outDir, name))
copyFileSync(sigPath, join(outDir, `${name}.sig`))

console.log(`wrote ${out}`)
console.log(`  version   ${version}`)
console.log(`  installer ${name} (copied with .sig)`)
console.log('')
console.log(`serve it:  node scripts/fake-update-server.mjs --dir ${outDir} --scenario ok`)
