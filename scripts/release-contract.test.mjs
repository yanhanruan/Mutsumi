import assert from 'node:assert/strict'
import test from 'node:test'

import {
  collectUpdaterDownloads,
  postPublishSkipReason,
  productionTagForVersion,
  ReleaseContractError,
  validateReleaseContract,
} from './release-contract.mjs'

const WINDOWS_SIGNATURE = `windows-${'w'.repeat(128)}`
const MACOS_SIGNATURE = `macos-${'m'.repeat(128)}`

function asset(name, size = 1024) {
  return { name, size }
}

function releaseFixture({ tag = 'v1.5.3', macos = false } = {}) {
  const windowsInstaller = 'mutsumi_1.5.3_x64-setup.exe'
  const assets = [
    asset(windowsInstaller, 12_000_000),
    asset(`${windowsInstaller}.sig`, 300),
    asset('latest.json', 800),
  ]
  const signatureTexts = new Map([
    [`${windowsInstaller}.sig`, WINDOWS_SIGNATURE],
  ])
  const platforms = {
    'windows-x86_64': {
      signature: WINDOWS_SIGNATURE,
      url: `https://github.com/yanhanruan/Mutsumi/releases/download/${tag}/${windowsInstaller}`,
    },
    'windows-x86_64-nsis': {
      signature: WINDOWS_SIGNATURE,
      url: `https://github.com/yanhanruan/Mutsumi/releases/download/${tag}/${windowsInstaller}`,
    },
  }

  if (macos) {
    const updater = 'mutsumi_1.5.3_universal.app.tar.gz'
    assets.push(
      asset(updater, 14_000_000),
      asset(`${updater}.sig`, 300),
      asset('mutsumi_1.5.3_universal.dmg', 15_000_000),
    )
    signatureTexts.set(`${updater}.sig`, MACOS_SIGNATURE)
    platforms['darwin-aarch64'] = {
      signature: MACOS_SIGNATURE,
      url: `https://github.com/yanhanruan/Mutsumi/releases/download/${tag}/${updater}`,
    }
    platforms['darwin-x86_64'] = {
      signature: MACOS_SIGNATURE,
      url: `https://github.com/yanhanruan/Mutsumi/releases/download/${tag}/${updater}`,
    }
  }

  return {
    tag,
    expectVersion: '1.5.3',
    expectedRepository: 'yanhanruan/Mutsumi',
    assets,
    signatureTexts,
    manifest: { version: '1.5.3', platforms },
  }
}

test('accepts the existing signed Windows release contract', () => {
  const result = validateReleaseContract(releaseFixture())
  assert.ok(result.some((message) => message.includes('windows-x86_64')))
})

test('deduplicates updater URLs shared by legacy and installer-specific platform keys', () => {
  const fixture = releaseFixture({ macos: true })
  const downloads = collectUpdaterDownloads(fixture.manifest)

  assert.equal(downloads.length, 2)
  assert.deepEqual(downloads[0].platformKeys, ['windows-x86_64', 'windows-x86_64-nsis'])
  assert.deepEqual(downloads[1].platformKeys, ['darwin-aarch64', 'darwin-x86_64'])
})

test('rejects malformed extra platform URLs in the post-publish contract', () => {
  const fixture = releaseFixture()
  fixture.manifest.platforms['windows-x86_64-nsis'].url = ''

  assert.throws(
    () => collectUpdaterDownloads(fixture.manifest),
    /windows-x86_64-nsis\.url is missing/,
  )
})

test('keeps the production healer away from drafts and rolling prereleases', () => {
  assert.equal(postPublishSkipReason({ draft: true, prerelease: false }), 'release is still a draft')
  assert.equal(postPublishSkipReason({ draft: false, prerelease: true }), 'release is a prerelease')
  assert.equal(postPublishSkipReason({ draft: false, prerelease: false }), null)
})

test('rejects a malformed version before the production healer can derive a tag', () => {
  assert.equal(productionTagForVersion('1.5.3'), 'v1.5.3')
  assert.throws(() => productionTagForVersion('../staging'), /must be an X\.Y\.Z production version/)
})

test('uses the requested staging tag path instead of inventing a version tag', () => {
  const result = validateReleaseContract(releaseFixture({ tag: 'staging' }))
  assert.ok(result.some((message) => message.includes('windows-x86_64')))
})

test('accepts one universal updater archive for both macOS architectures plus a DMG', () => {
  const fixture = releaseFixture({ macos: true })
  const result = validateReleaseContract({ ...fixture, requireMacosUniversal: true })
  assert.ok(result.some((message) => message.includes('universal updater contract')))
})

test('rejects a manifest signature that differs from the uploaded .sig file', () => {
  const fixture = releaseFixture()
  fixture.manifest.platforms['windows-x86_64'].signature = `tampered-${'x'.repeat(128)}`

  assert.throws(
    () => validateReleaseContract(fixture),
    (error) => error instanceof ReleaseContractError && /does not match uploaded asset/.test(error.message),
  )
})

test('compares signature asset text exactly, including trailing whitespace', () => {
  const fixture = releaseFixture()
  const signatureAsset = 'mutsumi_1.5.3_x64-setup.exe.sig'
  fixture.signatureTexts.set(signatureAsset, `${WINDOWS_SIGNATURE}\n`)

  assert.throws(
    () => validateReleaseContract(fixture),
    /signature does not match uploaded asset/,
  )
})

test('rejects a whitespace-only signature even when the uploaded text matches', () => {
  const fixture = releaseFixture()
  const whitespace = ' '.repeat(128)
  const signatureAsset = 'mutsumi_1.5.3_x64-setup.exe.sig'
  fixture.manifest.platforms['windows-x86_64'].signature = whitespace
  fixture.manifest.platforms['windows-x86_64-nsis'].signature = whitespace
  fixture.signatureTexts.set(signatureAsset, whitespace)

  assert.throws(
    () => validateReleaseContract(fixture),
    /signature is missing or suspiciously short/,
  )
})

test('rejects release URLs on a foreign host', () => {
  const fixture = releaseFixture()
  fixture.manifest.platforms['windows-x86_64'].url = fixture.manifest.platforms['windows-x86_64'].url
    .replace('https://github.com', 'https://example.invalid')

  assert.throws(() => validateReleaseContract(fixture), /must use https:\/\/github\.com/)
})

test('rejects release URLs for a different repository', () => {
  const fixture = releaseFixture()
  fixture.manifest.platforms['windows-x86_64'].url = fixture.manifest.platforms['windows-x86_64'].url
    .replace('/yanhanruan/Mutsumi/', '/someone/Else/')

  assert.throws(() => validateReleaseContract(fixture), /is not a yanhanruan\/Mutsumi release asset/)
})

test('rejects non-canonical release URLs with a query or fragment', () => {
  const fixture = releaseFixture()
  fixture.manifest.platforms['windows-x86_64'].url += '?download=1'

  assert.throws(() => validateReleaseContract(fixture), /without credentials, query or fragment/)
})

test('rejects release URL paths with duplicate or trailing slashes', () => {
  const duplicate = releaseFixture()
  duplicate.manifest.platforms['windows-x86_64'].url = duplicate.manifest.platforms['windows-x86_64'].url
    .replace('/releases/', '//releases/')
  assert.throws(() => validateReleaseContract(duplicate), /url path is not canonical/)

  const trailing = releaseFixture()
  trailing.manifest.platforms['windows-x86_64'].url += '/'
  assert.throws(() => validateReleaseContract(trailing), /url path is not canonical/)
})

test('rejects a release URL whose tag path does not match the requested release', () => {
  const fixture = releaseFixture({ tag: 'staging' })
  fixture.manifest.platforms['windows-x86_64'].url = fixture.manifest.platforms['windows-x86_64'].url
    .replace('/staging/', '/v1.5.3/')

  assert.throws(
    () => validateReleaseContract(fixture),
    /is not a yanhanruan\/Mutsumi release asset on tag staging/,
  )
})

test('rejects an incomplete universal macOS platform pair', () => {
  const fixture = releaseFixture({ macos: true })
  delete fixture.manifest.platforms['darwin-x86_64']

  assert.throws(
    () => validateReleaseContract({ ...fixture, requireMacosUniversal: true }),
    /has no platforms\."darwin-x86_64" entry/,
  )
})

test('rejects universal macOS entries that point at different archives', () => {
  const fixture = releaseFixture({ macos: true })
  const secondUpdater = 'mutsumi_1.5.3_x64.app.tar.gz'
  fixture.assets.push(asset(secondUpdater), asset(`${secondUpdater}.sig`, 300))
  fixture.signatureTexts.set(`${secondUpdater}.sig`, MACOS_SIGNATURE)
  fixture.manifest.platforms['darwin-x86_64'].url =
    `https://github.com/yanhanruan/Mutsumi/releases/download/v1.5.3/${secondUpdater}`

  assert.throws(
    () => validateReleaseContract({ ...fixture, requireMacosUniversal: true }),
    /must reference the same updater asset/,
  )
})

test('rejects an unreferenced second macOS updater archive', () => {
  const fixture = releaseFixture({ macos: true })
  fixture.assets.push(
    asset('stale.app.tar.gz'),
    asset('stale.app.tar.gz.sig', 300),
  )
  fixture.signatureTexts.set('stale.app.tar.gz.sig', MACOS_SIGNATURE)

  assert.throws(
    () => validateReleaseContract({ ...fixture, requireMacosUniversal: true }),
    /expected exactly one \*\.app\.tar\.gz asset, found 2/,
  )
})

test('rejects a macOS updater release without its direct-install DMG', () => {
  const fixture = releaseFixture({ macos: true })
  fixture.assets = fixture.assets.filter((entry) => !entry.name.endsWith('.dmg'))

  assert.throws(
    () => validateReleaseContract({ ...fixture, requireMacosUniversal: true }),
    /expected exactly one \*\.dmg asset, found 0/,
  )
})

test('validates every extra manifest platform instead of ignoring stale entries', () => {
  const fixture = releaseFixture()
  fixture.manifest.platforms['darwin-aarch64'] = {
    signature: MACOS_SIGNATURE,
    url: 'https://github.com/yanhanruan/Mutsumi/releases/download/v1.5.3/stale.app.tar.gz',
  }

  assert.throws(
    () => validateReleaseContract(fixture),
    /stale\.app\.tar\.gz.*not in this release/,
  )
})
