#!/usr/bin/env node
/**
 * gen-test-keys.mjs — generate a TEST-ONLY updater signing keypair for the
 * local fake-update-server integration tests (see docs/TESTING-UPDATES.md).
 *
 *   node scripts/gen-test-keys.mjs
 *
 * Writes scripts/test-keys/mutsumi-test.key (+ .pub). The directory is
 * gitignored: these keys sign only local test artifacts and must never be
 * committed or confused with the production key (which lives in your GitHub
 * secrets / ~/.tauri). The keypair has an EMPTY password so the staging build
 * command stays non-interactive.
 *
 * After generating, paste the printed public key into
 * src-tauri/tauri.staging.conf.json → plugins.updater.pubkey.
 */
import { spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const here = dirname(fileURLToPath(import.meta.url))
const outDir = join(here, 'test-keys')
const keyPath = join(outDir, 'mutsumi-test.key')

mkdirSync(outDir, { recursive: true })

if (existsSync(keyPath) && !process.argv.includes('--force')) {
  console.log(`test keypair already exists at ${keyPath} (use --force to regenerate)`)
} else {
  // `--password=` (equals form) — a bare '' arg would be eaten by the shell
  // layer that npx needs on Windows.
  const result = spawnSync(
    'npx',
    ['tauri', 'signer', 'generate', '-w', keyPath, '--password=', '--force'],
    { stdio: 'inherit', shell: true },
  )
  if (result.status !== 0) {
    console.error('error: tauri signer generate failed')
    process.exit(result.status ?? 1)
  }
}

const pubkey = readFileSync(`${keyPath}.pub`, 'utf8').trim()
console.log('')
console.log('── TEST public key (paste into src-tauri/tauri.staging.conf.json → plugins.updater.pubkey) ──')
console.log(pubkey)
console.log('')
console.log('Staging builds then sign with (PowerShell):')
console.log(`  $env:TAURI_SIGNING_PRIVATE_KEY = "${keyPath.replace(/\\/g, '\\\\')}"`)
console.log('  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""')
console.log('  npm run tauri build -- --config src-tauri/tauri.staging.conf.json')
