import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const root = new URL('../', import.meta.url)
const production = readFileSync(new URL('.github/workflows/release.yml', root), 'utf8')
const staging = readFileSync(new URL('.github/workflows/staging-release.yml', root), 'utf8')
const desktop = readFileSync(new URL('.github/workflows/desktop-ci.yml', root), 'utf8')
const secretPreflight = readFileSync(new URL('scripts/verify-release-secrets.sh', root), 'utf8')
const setupSigning = readFileSync(new URL('scripts/setup-macos-signing.sh', root), 'utf8')
const cleanupSigning = readFileSync(new URL('scripts/cleanup-macos-signing.sh', root), 'utf8')

function occurrences(text, value) {
  return text.split(value).length - 1
}

function job(workflow, name, nextName) {
  const start = workflow.indexOf(`\n  ${name}:\n`)
  assert.notEqual(start, -1, `job ${name} must exist`)
  const end = nextName ? workflow.indexOf(`\n  ${nextName}:\n`, start + 1) : workflow.length
  assert.notEqual(end, -1, `job ${nextName} must exist after ${name}`)
  return workflow.slice(start, end)
}

test('uses tauri-action v1 and its current updater input in both release workflows', () => {
  for (const workflow of [production, staging]) {
    assert.equal(occurrences(workflow, 'tauri-apps/tauri-action@v1'), 2)
    assert.equal(occurrences(workflow, 'uploadUpdaterJson: true'), 2)
    assert.doesNotMatch(workflow, /tauri-action@v0|includeUpdaterJson/)
  }
})

test('serializes release creation, macOS upload and final cross-platform verification', () => {
  for (const workflow of [production, staging]) {
    const windows = job(workflow, 'windows', 'macos')
    const macos = job(workflow, 'macos', 'verify')
    const verify = job(workflow, 'verify')

    assert.match(windows, /needs: preflight/)
    assert.match(windows, /release_id: \$\{\{ steps\.tauri\.outputs\.releaseId \}\}/)
    assert.match(macos, /needs: windows/)
    assert.match(macos, /releaseId: \$\{\{ needs\.windows\.outputs\.release_id \}\}/)
    assert.match(macos, /releaseBody:/)
    assert.match(verify, /needs: \[windows, macos\]/)
    assert.match(verify, /--require-macos-universal/)
    const normalizeIndex = verify.indexOf('node scripts/normalize-release-manifest.mjs')
    const verifyIndex = verify.indexOf('node scripts/verify-release-assets.mjs')
    assert.ok(normalizeIndex !== -1 && verifyIndex > normalizeIndex)
    assert.match(verify, /--release-id "\$\{\{ needs\.windows\.outputs\.release_id \}\}"/)
    assert.match(verify, /--channel (?:production|staging)/)
  }
})

test('runs Rust tests on both platforms before a signed release can complete', () => {
  for (const workflow of [production, staging]) {
    const windows = job(workflow, 'windows', 'macos')
    const macos = job(workflow, 'macos', 'verify')

    assert.match(windows, /cargo test --manifest-path src-tauri\/Cargo\.toml/)
    assert.match(macos, /cargo test --manifest-path src-tauri\/Cargo\.toml/)
  }
})

test('runs a native Intel macOS build and lifecycle gate without release credentials', () => {
  const intel = job(desktop, 'intel-native')

  assert.match(intel, /runs-on: macos-15-intel/)
  assert.match(intel, /test "\$\(uname -m\)" = "x86_64"/)
  assert.match(intel, /host: x86_64-apple-darwin/)
  assert.match(intel, /cargo test --manifest-path src-tauri\/Cargo\.toml/)
  assert.match(intel, /--target x86_64-apple-darwin --bundles app --ci --no-sign/)
  assert.match(intel, /test:macos-lifecycle-smoke/)
  assert.match(intel, /--arch x86_64/)
  assert.doesNotMatch(intel, /secrets\.|APPLE_|TAURI_SIGNING_PRIVATE_KEY|upload-artifact/)
})

test('checks every required release secret before a workflow can mutate a release', () => {
  for (const workflow of [production, staging]) {
    const preflight = job(workflow, 'preflight', 'windows')
    const windows = job(workflow, 'windows', 'macos')

    assert.match(preflight, /bash scripts\/verify-release-secrets\.sh/)
    assert.match(windows, /needs: preflight/)
    assert.doesNotMatch(preflight, /tauri-action|gh release delete|git push/)
  }

  for (const secret of [
    'TAURI_SIGNING_PRIVATE_KEY',
    'APPLE_CERTIFICATE',
    'APPLE_CERTIFICATE_PASSWORD',
    'KEYCHAIN_PASSWORD',
    'APPLE_API_ISSUER',
    'APPLE_API_KEY',
    'APPLE_API_KEY_BASE64',
  ]) {
    assert.match(secretPreflight, new RegExp(`\\n  ${secret}\\n`))
  }
  assert.match(secretPreflight, /base64 --decode/)
  assert.match(secretPreflight, /BEGIN PRIVATE KEY/)
  assert.match(secretPreflight, /\[0-9A-Fa-f\]\{8\}.*\[0-9A-Fa-f\]\{12\}/)
  assert.doesNotMatch(secretPreflight, /echo "\$APPLE_|set -x/)
})

test('preserves updater notes when the macOS action rewrites the merged manifest', () => {
  const productionMacos = job(production, 'macos', 'verify')
  const stagingMacos = job(staging, 'macos', 'verify')

  assert.match(productionMacos, /fetch-depth: 0/)
  assert.match(productionMacos, /docs\/release-notes\/\$\{GITHUB_REF_NAME\}\.md/)
  assert.match(productionMacos, /releaseBody: \$\{\{ steps\.notes\.outputs\.notes \}\}/)
  assert.match(stagingMacos, /Version: v\$\{\{ needs\.windows\.outputs\.version \}\}/)
  assert.match(stagingMacos, /Source: `\$\{\{ github\.ref_name \}\}` @ \$\{\{ github\.sha \}\}/)
})

test('recomposes the production release body without duplicating changelog on reruns', () => {
  const verify = job(production, 'verify')

  assert.match(verify, /docs\/release-notes\/\$\{GITHUB_REF_NAME\}\.md/)
  assert.match(verify, /cat "\$notes_file" > body\.md/)
  assert.match(verify, /cat generated-notes\.md >> body\.md/)
  assert.doesNotMatch(verify, /--jq \.body > body\.md/)
})

test('builds one universal signed and notarized macOS bundle behind the exact bundle gate', () => {
  for (const workflow of [production, staging]) {
    const macos = job(workflow, 'macos', 'verify')

    assert.match(macos, /targets: aarch64-apple-darwin,x86_64-apple-darwin/)
    assert.match(macos, /args: --target universal-apple-darwin/)
    assert.match(macos, /bash scripts\/setup-macos-signing\.sh/)
    assert.match(macos, /--mode signed/)
    assert.match(macos, /--expected-identifier io\.github\.yanhanruan\.mutsumi/)
    assert.match(macos, /--expected-minimum-system-version 13\.0/)
    assert.match(macos, /if: always\(\)[\s\S]*bash scripts\/cleanup-macos-signing\.sh/)
  }
})

test('keeps Apple credentials out of Windows jobs and preserves channel safety', () => {
  const productionWindows = job(production, 'windows', 'macos')
  const productionMacos = job(production, 'macos', 'verify')
  const stagingWindows = job(staging, 'windows', 'macos')
  const stagingMacos = job(staging, 'macos', 'verify')

  for (const windows of [productionWindows, stagingWindows]) {
    assert.doesNotMatch(windows, /APPLE_CERTIFICATE|APPLE_API_ISSUER|KEYCHAIN_PASSWORD/)
    assert.match(windows, /updaterJsonPreferNsis: true/)
  }
  for (const macos of [productionMacos, stagingMacos]) {
    for (const secret of [
      'APPLE_CERTIFICATE',
      'APPLE_CERTIFICATE_PASSWORD',
      'KEYCHAIN_PASSWORD',
      'APPLE_API_ISSUER',
      'APPLE_API_KEY',
      'APPLE_API_KEY_BASE64',
    ]) {
      assert.match(macos, new RegExp(`secrets\\.${secret}`))
    }
  }

  assert.equal(occurrences(production, 'releaseDraft: true'), 2)
  assert.equal(occurrences(production, 'prerelease: false'), 2)
  assert.equal(occurrences(staging, 'releaseDraft: false'), 2)
  assert.equal(occurrences(staging, 'prerelease: true'), 2)
})

test('resets staging without swallowing deletion failures and verifies tag provenance', () => {
  const stagingWindows = job(staging, 'windows', 'macos')
  const stagingVerify = job(staging, 'verify')

  assert.doesNotMatch(stagingWindows, /gh release delete[^\n]*\|\| true|git push[^\n]*\|\| true/)
  assert.match(stagingWindows, /gh api -X DELETE/)
  assert.match(stagingWindows, /remaining_release=/)
  assert.match(stagingWindows, /remaining_tag=/)
  assert.match(stagingVerify, /git\/ref\/tags\/staging/)
  assert.match(stagingVerify, /tag_sha.*GITHUB_SHA/s)
  assert.ok(
    stagingVerify.indexOf('git/ref/tags/staging') <
      stagingVerify.indexOf('node scripts/normalize-release-manifest.mjs'),
  )
})

test('materializes secrets privately, selects one Developer ID identity and constrains cleanup', () => {
  assert.match(setupSigning, /set -euo pipefail/)
  assert.match(setupSigning, /umask 077/)
  assert.match(setupSigning, /\^\[A-Z0-9\]\{10\}\$/)
  assert.match(setupSigning, /\[0-9A-Fa-f\]\{8\}.*\[0-9A-Fa-f\]\{12\}/)
  assert.match(setupSigning, /security import/)
  assert.match(setupSigning, /security set-key-partition-list/)
  assert.match(setupSigning, /\^Developer ID Application:/)
  assert.match(setupSigning, /identity_count" -ne 1/)
  assert.match(setupSigning, /::add-mask::\$identity/)
  assert.doesNotMatch(setupSigning, /echo "\$APPLE_|set -x/)

  assert.match(cleanupSigning, /"\$runner_temp"\/\*/)
  assert.match(cleanupSigning, /security delete-keychain/)
  assert.match(cleanupSigning, /\/bin\/rm -f -- "\$path"/)
})
