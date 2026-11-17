#!/usr/bin/env node
/**
 * make-latest-json.mjs — build a `latest.json` updater manifest for the local
 * fake update server (scripts/fake-update-server.mjs) from a built staging
 * installer and its .sig file.
 *
 *   node scripts/make-latest-json.mjs \
 *     --installer src-tauri/target/release/bundle/nsis/mutsumi_1.5.0_x64-setup.exe \
 *     --version 1.5.0 \
 *     [--platform windows-x86_64|darwin-aarch64|darwin-x86_64|darwin-universal] \
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
const platform = arg('platform', 'windows-x86_64')
const targets = {
  'windows-x86_64': ['windows-x86_64'],
  'darwin-aarch64': ['darwin-aarch64'],
  'darwin-x86_64': ['darwin-x86_64'],
  'darwin-universal': ['darwin-aarch64', 'darwin-x86_64'],
}
if (!Object.hasOwn(targets, platform)) {
  console.error(`error: unsupported platform ${platform}`)
  process.exit(1)
}
if (!installer || !version) {
  console.error('usage: node scripts/make-latest-json.mjs --installer <setup.exe|app.tar.gz> --version <X.Y.Z> [--platform <target>] [--out <latest.json>] [--base-url <url>] [--notes <text>]')
  process.exit(1)
}

const installerPath = resolve(installer)
if (!installerPath.endsWith(platform.startsWith('darwin-') ? '.app.tar.gz' : '.exe')) {
  console.error('error: macOS updater requires .app.tar.gz (not DMG); Windows requires .exe')
  process.exit(1)
}
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
  platforms: Object.fromEntries(targets[platform].map(target => [target, {
      signature,
      url: `${baseUrl}/${encodeURIComponent(name)}`,
    }])),
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
console.log('Serve using the running client architecture (choose one):')
const quotedDir = "'" + outDir.replaceAll("'", "'\\''") + "'"
for (const target of targets[platform]) {
  console.log(`  node scripts/fake-update-server.mjs --dir ${quotedDir} --platform ${target} --scenario ok`)
}
