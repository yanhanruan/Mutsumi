#!/usr/bin/env node
/**
 * fake-update-server.mjs — local fake update source for Layer-2 integration
 * tests (docs/TESTING-UPDATES.md). Serves an updater manifest + installer to a
 * STAGING build of the app (built with src-tauri/tauri.staging.conf.json,
 * whose endpoint points at http://localhost:9414/latest.json), driving the
 * real tauri-plugin-updater protocol end to end — including signature
 * verification against the test keypair.
 *
 *   node scripts/fake-update-server.mjs --dir scripts/fake-update-dist --scenario ok
 *
 * --dir must contain latest.json + the installer + its .sig
 * (produced by scripts/make-latest-json.mjs).
 *
 * Scenarios (what the client MUST do in each case):
 *   ok             valid manifest + installer + signature → client updates
 *   lower-version  manifest advertises 0.0.1              → "up to date", no prompt
 *   missing-windows manifest has no windows platform      → failed view (plugin
 *                  errors: no matching platform — such a manifest is broken,
 *                  and verify-release-assets.mjs keeps it from ever shipping)
 *   missing-platform removes only --platform's entry (defaults to windows-x86_64)
 *   installer-404  download URL returns 404               → failed view, retry allowed
 *   bad-signature  signature corrupted                    → verification failure, NEVER installs
 *   malformed-json latest.json is not valid JSON          → failed view, no crash
 *   interrupt      download drops mid-stream              → failed view, NEVER "installed"
 */
import { createServer } from 'node:http'
import { readFileSync, existsSync, statSync, createReadStream } from 'node:fs'
import { join, resolve, basename } from 'node:path'

const SCENARIOS = [
  'ok', 'lower-version', 'missing-windows', 'missing-platform', 'installer-404',
  'bad-signature', 'malformed-json', 'interrupt',
]

function arg(name, fallback = null) {
  const i = process.argv.indexOf(`--${name}`)
  return i !== -1 && process.argv[i + 1] !== undefined ? process.argv[i + 1] : fallback
}

const dir = resolve(arg('dir', join('scripts', 'fake-update-dist')))
const scenario = arg('scenario', 'ok')
const port = Number(arg('port', '9414'))
const platform = arg('platform', 'windows-x86_64')
if (!['windows-x86_64', 'darwin-aarch64', 'darwin-x86_64'].includes(platform)) {
  console.error(`error: unsupported platform ${platform}`)
  process.exit(1)
}

if (!SCENARIOS.includes(scenario)) {
  console.error(`error: unknown scenario "${scenario}"\nvalid: ${SCENARIOS.join(', ')}`)
  process.exit(1)
}
const manifestPath = join(dir, 'latest.json')
if (!existsSync(manifestPath)) {
  console.error(`error: ${manifestPath} not found — run scripts/make-latest-json.mjs first`)
  process.exit(1)
}

/** Apply the scenario's corruption to the pristine manifest. */
function manifestBody() {
  const raw = readFileSync(manifestPath, 'utf8')
  if (scenario === 'malformed-json') return '{ "version": "9.9.9", this is not json'
  const m = JSON.parse(raw)
  if (scenario === 'lower-version') m.version = '0.0.1'
  if (scenario === 'missing-windows') delete m.platforms['windows-x86_64']
  if (scenario === 'missing-platform') delete m.platforms[platform]
  if (scenario === 'bad-signature') {
    const entry = m.platforms[platform]
    // The manifest signature is base64 of the WHOLE minisign .sig file, whose
    // first line is an "untrusted comment" that verification ignores by
    // design. Tampering must therefore hit line 2 — the actual ed25519
    // signature. (v1 of this script corrupted bytes 30–36, i.e. the comment,
    // and the update sailed through: a false PASS of the whole scenario.)
    const lines = Buffer.from(entry.signature, 'base64').toString('utf8').split('\n')
    if (!lines[1]?.length) throw new Error('signature must contain a minisign signature line')
    const mid = Math.floor(lines[1].length / 2)
    const flipped = lines[1][mid] === 'A' ? 'B' : 'A'
    lines[1] = lines[1].slice(0, mid) + flipped + lines[1].slice(mid + 1)
    entry.signature = Buffer.from(lines.join('\n'), 'utf8').toString('base64')
  }
  return JSON.stringify(m, null, 2)
}

// Reject unusable fixtures before accepting client requests.
try {
  const pristine = JSON.parse(readFileSync(manifestPath, 'utf8'))
  if (!pristine.platforms?.[platform]) throw new Error(`manifest has no ${platform} entry`)
  manifestBody()
} catch (error) {
  console.error(`error: invalid fixture: ${error.message}`)
  process.exit(1)
}

const server = createServer((req, res) => {
  const url = new URL(req.url, `http://localhost:${port}`)
  console.log(`[fake-update-server] ${req.method} ${url.pathname}  (scenario: ${scenario})`)

  if (url.pathname === '/latest.json') {
    const body = manifestBody()
    res.writeHead(200, { 'content-type': 'application/json' })
    res.end(body)
    return
  }

  if (url.pathname.startsWith('/download/')) {
    const name = basename(decodeURIComponent(url.pathname.slice('/download/'.length)))
    const file = join(dir, name)
    if (scenario === 'installer-404' || !existsSync(file)) {
      res.writeHead(404, { 'content-type': 'text/plain' })
      res.end('not found')
      return
    }

    const size = statSync(file).size
    res.writeHead(200, { 'content-type': 'application/octet-stream', 'content-length': size })

    if (scenario === 'interrupt') {
      // Stream roughly half the installer, then destroy the socket so the
      // client sees a mid-download connection failure.
      const stream = createReadStream(file, { end: Math.max(0, Math.floor(size / 2) - 1) })
      let sent = 0
      stream.on('data', chunk => {
        sent += chunk.length
        res.write(chunk)
        if (sent >= size / 2) {
          console.log(`[fake-update-server] interrupting after ${sent}/${size} bytes`)
          stream.destroy()
          res.destroy()
        }
      })
      stream.on('end', () => res.destroy())
      return
    }

    createReadStream(file).pipe(res)
    return
  }

  res.writeHead(404, { 'content-type': 'text/plain' })
  res.end('not found')
})

server.listen(port, 'localhost', () => {
  console.log(`fake update server on http://localhost:${server.address().port}`)
  console.log(`  dir       ${dir}`)
  console.log(`  scenario  ${scenario}`)
  console.log('')
  console.log('point a STAGING build at it (tauri.staging.conf.json) and open')
  console.log('About → "Check for updates". Ctrl-C to stop.')
})
