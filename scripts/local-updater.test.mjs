import test from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, writeFileSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { spawn, spawnSync } from 'node:child_process'
import { once } from 'node:events'

const signature = Buffer.from('untrusted comment: test\n' + 'A'.repeat(100) + '\ntrusted comment: test\nAAAA\n').toString('base64')

function fixture(t, platform = 'darwin-universal', omitPlatform = false, size = 1024 * 128) {
  const dir = mkdtempSync(join(tmpdir(), 'mutsumi-updater-'))
  t.after(() => rmSync(dir, { recursive: true, force: true }))
  const installer = join(dir, platform.startsWith('darwin') ? 'test app.app.tar.gz' : 'setup.exe')
  writeFileSync(installer, Buffer.alloc(size, 42))
  writeFileSync(`${installer}.sig`, signature)
  const out = join(dir, 'serve', 'latest.json')
  const result = spawnSync(process.execPath, ['scripts/make-latest-json.mjs', '--installer', installer,
    '--version', '9.9.9', ...(omitPlatform ? [] : ['--platform', platform]), '--out', out], { encoding: 'utf8' })
  assert.equal(result.status, 0, result.stderr)
  return { dir: join(dir, 'serve'), manifest: JSON.parse(readFileSync(out, 'utf8')), installer, output: result.stdout }
}

async function server(t, dir, scenario, platform, logs = []) {
  const child = spawn(process.execPath, ['scripts/fake-update-server.mjs', '--dir', dir,
    '--scenario', scenario, ...(platform ? ['--platform', platform] : []), '--port', '0'], { stdio: ['ignore', 'pipe', 'pipe'] })
  t.after(async () => {
    if (child.exitCode === null && child.signalCode === null) {
      const exited = once(child, 'exit')
      child.kill()
      await exited
    }
  })
  return await new Promise((resolve, reject) => {
    let output = ''
    let errors = ''
    const timer = setTimeout(() => reject(new Error('server startup timed out')), 5000)
    child.stderr.on('data', data => { errors += data })
    child.once('error', error => { clearTimeout(timer); reject(error) })
    child.once('exit', () => { clearTimeout(timer); reject(new Error(errors || 'server exited')) })
    child.stdout.on('data', data => {
      output += data
      logs.push(String(data))
      const match = output.match(/fake update server on (http:\/\/localhost:\d+)/)
      if (match) { clearTimeout(timer); resolve(match[1]) }
    })
  })
}

async function waitFor(condition, message, timeoutMs = 2000) {
  const deadline = Date.now() + timeoutMs
  while (!condition()) {
    if (Date.now() > deadline) throw new Error(`timed out: ${message}`)
    await new Promise(resolve => setTimeout(resolve, 20))
  }
}

test('universal fixture maps both macOS architectures to one archive and signature', t => {
  const { manifest, output } = fixture(t)
  assert.deepEqual(Object.keys(manifest.platforms), ['darwin-aarch64', 'darwin-x86_64'])
  assert.deepEqual(manifest.platforms['darwin-aarch64'], manifest.platforms['darwin-x86_64'])
  assert.equal(manifest.platforms['darwin-aarch64'].signature, signature)
  assert.match(manifest.platforms['darwin-aarch64'].url, /test%20app.app.tar.gz$/)
  assert.match(output, /--platform darwin-aarch64 --scenario ok/)
  assert.match(output, /--platform darwin-x86_64 --scenario ok/)
})

test('legacy omitted platform remains Windows and missing-windows still works', async t => {
  const { dir, manifest, output } = fixture(t, 'windows-x86_64', true)
  assert.deepEqual(Object.keys(manifest.platforms), ['windows-x86_64'])
  assert.match(output, /--platform windows-x86_64 --scenario ok/)
  const base = await server(t, dir, 'missing-windows')
  assert.deepEqual((await (await fetch(`${base}/latest.json`)).json()).platforms, {})
})

for (const platform of ['windows-x86_64', 'darwin-aarch64', 'darwin-x86_64']) {
  test(`${platform}: missing entry and corrupt signature target the requested client`, async t => {
    const { dir, manifest } = fixture(t, platform.startsWith('darwin') ? 'darwin-universal' : platform)
    const missing = await server(t, dir, 'missing-platform', platform)
    const body = await (await fetch(`${missing}/latest.json`)).json()
    assert.equal(body.platforms[platform], undefined)
    for (const key of Object.keys(manifest.platforms).filter(key => key !== platform)) {
      assert.deepEqual(body.platforms[key], manifest.platforms[key])
    }
    const bad = await server(t, dir, 'bad-signature', platform)
    const corrupted = await (await fetch(`${bad}/latest.json`)).json()
    const lines = Buffer.from(corrupted.platforms[platform].signature, 'base64').toString().split('\n')
    const original = Buffer.from(signature, 'base64').toString().split('\n')
    assert.equal(lines[0], original[0])
    assert.notEqual(lines[1], original[1])
    for (const key of Object.keys(manifest.platforms).filter(key => key !== platform)) {
      assert.deepEqual(corrupted.platforms[key], manifest.platforms[key])
    }
  })
}

test('HTTP fixture serves bytes, 404, malformed JSON, lower version, and interrupted download', async t => {
  const { dir, manifest } = fixture(t)
  const download = new URL(manifest.platforms['darwin-aarch64'].url).pathname
  for (const scenario of ['ok', 'installer-404', 'malformed-json', 'lower-version', 'interrupt']) {
    const base = await server(t, dir, scenario, 'darwin-aarch64')
    if (scenario === 'malformed-json') {
      await assert.rejects(async () => (await fetch(`${base}/latest.json`)).json())
    } else if (scenario === 'lower-version') {
      assert.equal((await (await fetch(`${base}/latest.json`)).json()).version, '0.0.1')
    } else if (scenario === 'interrupt') {
      await assert.rejects(async () => (await fetch(`${base}${download}`)).arrayBuffer())
    } else {
      const response = await fetch(`${base}${download}`)
      assert.equal(response.status, scenario === 'ok' ? 200 : 404)
      if (scenario === 'ok') assert.equal((await response.arrayBuffer()).byteLength, 128 * 1024)
    }
  }
})

test('interrupt logs the cut-off point for odd-sized archives', async t => {
  const { dir, manifest } = fixture(t, 'darwin-universal', false, 1024 * 128 + 1)
  const logs = []
  const base = await server(t, dir, 'interrupt', 'darwin-aarch64', logs)
  const download = new URL(manifest.platforms['darwin-aarch64'].url).pathname
  await assert.rejects(async () => (await fetch(`${base}${download}`)).arrayBuffer())
  await waitFor(() => logs.join('').includes('interrupting after 65536/131073 bytes'), 'interrupt log line')
})

test('missing-windows is refused for a macOS client instead of acting like ok', async t => {
  const { dir } = fixture(t)
  await assert.rejects(server(t, dir, 'missing-windows', 'darwin-aarch64'),
    /missing-windows only applies to windows-x86_64/)
})

test('platform faults also hit installer-specific keys the client resolves first', async t => {
  const { dir, manifest } = fixture(t)
  const withInstallerKeys = structuredClone(manifest)
  withInstallerKeys.platforms['darwin-aarch64-app'] = manifest.platforms['darwin-aarch64']
  withInstallerKeys.platforms['darwin-x86_64-app'] = manifest.platforms['darwin-x86_64']
  writeFileSync(join(dir, 'latest.json'), JSON.stringify(withInstallerKeys))

  const missing = await server(t, dir, 'missing-platform', 'darwin-aarch64')
  const served = await (await fetch(`${missing}/latest.json`)).json()
  assert.deepEqual(Object.keys(served.platforms).sort(), ['darwin-x86_64', 'darwin-x86_64-app'])

  const bad = await server(t, dir, 'bad-signature', 'darwin-aarch64')
  const corrupted = await (await fetch(`${bad}/latest.json`)).json()
  for (const key of ['darwin-aarch64', 'darwin-aarch64-app']) {
    assert.notEqual(corrupted.platforms[key].signature, signature, key)
  }
  for (const key of ['darwin-x86_64', 'darwin-x86_64-app']) {
    assert.equal(corrupted.platforms[key].signature, signature, key)
  }
})

test('DMG cannot be mistaken for a macOS updater archive', () => {
  const result = spawnSync(process.execPath, ['scripts/make-latest-json.mjs', '--installer', 'test.dmg',
    '--version', '9.9.9', '--platform', 'darwin-universal'], { encoding: 'utf8' })
  assert.equal(result.status, 1)
  assert.match(result.stderr, /not DMG/)
})
