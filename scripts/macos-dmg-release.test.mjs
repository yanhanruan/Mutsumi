import assert from 'node:assert/strict'
import test from 'node:test'

import { notarizeDmg, parseDmgArgs, uploadStapledDmg } from './finalize-macos-dmg.mjs'

const credentials = {
  APPLE_API_KEY_PATH: '/runner/temp/AuthKey_TESTKEY123.p8',
  APPLE_API_KEY: 'TESTKEY123',
  APPLE_API_ISSUER: 'test-issuer',
}

test('DMG CLI requires explicit, unambiguous arguments and a safe release channel', () => {
  assert.deepEqual(parseDmgArgs(['notarize', '--bundle-dir', 'bundle']), {
    phase: 'notarize', bundleDir: 'bundle',
  })
  assert.equal(parseDmgArgs([
    'upload', '--bundle-dir', 'bundle', '--release-id', '42', '--tag', 'staging', '--channel', 'staging',
  ]).releaseId, 42)
  for (const args of [
    [], ['notarize'], ['notarize', '--bundle-dir'],
    ['notarize', '--bundel-dir', 'bundle'],
    ['notarize', '--bundle-dir', 'a', '--bundle-dir', 'b'],
    ['upload', '--bundle-dir', 'bundle', '--release-id', '0', '--tag', 'staging', '--channel', 'staging'],
    ['upload', '--bundle-dir', 'bundle', '--release-id', '42', '--tag', 'staging', '--channel', 'production'],
  ]) assert.throws(() => parseDmgArgs(args))
})

test('waits for DMG acceptance before stapling and validating its ticket', () => {
  const calls = []
  notarizeDmg('/bundle/app.dmg', credentials, (command, args) => {
    calls.push([command, args])
    return args[0] === 'notarytool' ? '{"status":"Accepted"}' : ''
  })
  assert.deepEqual(calls.map(([, args]) => args.slice(0, 2)), [
    ['notarytool', 'submit'], ['stapler', 'staple'], ['stapler', 'validate'],
  ])
  assert.deepEqual(calls[0][1], [
    'notarytool', 'submit', '/bundle/app.dmg',
    '--key', credentials.APPLE_API_KEY_PATH, '--key-id', credentials.APPLE_API_KEY,
    '--issuer', credentials.APPLE_API_ISSUER,
    '--wait', '--timeout', '20m', '--output-format', 'json',
  ])
})

test('never staples rejected, pending, malformed or failed notarization results', () => {
  for (const output of ['{"status":"Invalid"}', '{"status":"In Progress"}', '{}', 'not JSON']) {
    const calls = []
    assert.throws(() => notarizeDmg('/bundle/app.dmg', credentials, (...args) => {
      calls.push(args)
      return output
    }))
    assert.equal(calls.length, 1)
  }
  assert.throws(() => notarizeDmg('/bundle/app.dmg', {}, () => assert.fail('must not invoke xcrun')))
  assert.throws(() => notarizeDmg('/bundle/app.dmg', credentials, () => {
    throw new Error('notary service failed')
  }), /notary service failed/)
})

test('stapling failure stops before ticket validation', () => {
  const calls = []
  assert.throws(() => notarizeDmg('/bundle/app.dmg', credentials, (_command, args) => {
    calls.push(args)
    if (args[1] === 'staple') throw new Error('stapling failed')
    return '{"status":"Accepted"}'
  }), /stapling failed/)
  assert.equal(calls.length, 2)
})

function fixture(overrides = {}) {
  const content = Buffer.from('final stapled DMG bytes')
  const release = {
    id: 42, tag_name: 'staging', draft: false, prerelease: true,
    assets: [{ id: 101, name: 'app.dmg', size: 1 }, { id: 102, name: 'app.app.tar.gz', size: 100 }],
    ...overrides,
  }
  const calls = []
  const commands = []
  const options = {
    dmgPath: '/bundle/app.dmg', size: content.length, repository: 'yanhanruan/Mutsumi',
    token: 'test-token', releaseId: 42, tag: 'staging', channel: 'staging',
  }
  const dependencies = {
    execute: (command, args) => { commands.push([command, args]) },
    openBody: () => content,
    request: async (url, init) => {
      calls.push({ url: String(url), ...init })
      if (!init.method) return { ok: true, json: async () => release }
      if (init.method === 'DELETE') return { status: 204 }
      return { status: 201, json: async () => ({ id: 103, name: 'app.dmg', size: content.length }) }
    },
  }
  return { options, dependencies, calls, commands, content }
}

test('uploads the stapled bytes only to the guarded release and replaces only its DMG', async () => {
  const f = fixture()
  await uploadStapledDmg(f.options, f.dependencies)
  assert.deepEqual(f.commands, [['xcrun', ['stapler', 'validate', '/bundle/app.dmg']]])
  assert.deepEqual(f.calls.map((call) => call.method ?? 'GET'), ['GET', 'DELETE', 'POST'])
  assert.equal(f.calls[1].url, 'https://api.github.com/repos/yanhanruan/Mutsumi/releases/assets/101')
  assert.equal(f.calls[2].url, 'https://uploads.github.com/repos/yanhanruan/Mutsumi/releases/42/assets?name=app.dmg')
  assert.equal(f.calls[2].body, f.content)
  assert.equal(f.calls[2].headers['content-length'], String(f.content.length))
})

test('production replacement accepts only the intended unpublished draft', async () => {
  const f = fixture({ tag_name: 'v1.5.3', draft: true, prerelease: false })
  await uploadStapledDmg({ ...f.options, tag: 'v1.5.3', channel: 'production' }, f.dependencies)
  assert.equal(f.calls.length, 3)
})

test('wrong release id, tag, channel or DMG never leads to a delete or upload', async () => {
  for (const release of [
    { id: 43 }, { tag_name: 'v1.5.3' }, { draft: true }, { prerelease: false },
    { assets: [] }, { assets: [{ id: 101, name: 'other.dmg' }] },
    { assets: [{ id: 101, name: 'app.dmg' }, { id: 102, name: 'extra.dmg' }] },
    { assets: [{ id: -1, name: 'app.dmg' }] },
  ]) {
    const f = fixture(release)
    await assert.rejects(uploadStapledDmg(f.options, f.dependencies))
    assert.deepEqual(f.calls.map((call) => call.method ?? 'GET'), ['GET'])
  }
})

test('unreadable ticket prevents all GitHub requests', async () => {
  const f = fixture()
  f.dependencies.execute = () => { throw new Error('ticket validation failed') }
  await assert.rejects(uploadStapledDmg(f.options, f.dependencies), /ticket validation failed/)
  assert.equal(f.calls.length, 0)
})

test('delete failure prevents upload and upload failure never reports success', async () => {
  for (const failedMethod of ['DELETE', 'POST']) {
    const f = fixture()
    const request = f.dependencies.request
    f.dependencies.request = async (url, init) => {
      if (init.method === failedMethod) {
        f.calls.push({ method: init.method })
        return { status: 503 }
      }
      return request(url, init)
    }
    await assert.rejects(uploadStapledDmg(f.options, f.dependencies), /HTTP 503/)
    assert.equal(f.calls.length, failedMethod === 'DELETE' ? 2 : 3)
  }
  const f = fixture()
  const request = f.dependencies.request
  f.dependencies.request = (url, init) => init.method === 'POST'
    ? { status: 201, json: async () => ({ id: 103, name: 'wrong.dmg', size: 1 }) }
    : request(url, init)
  await assert.rejects(uploadStapledDmg(f.options, f.dependencies), /does not match/)
})
