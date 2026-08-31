/**
 * Pure release/updater contract validation shared by CI and node:test.
 *
 * GitHub API access deliberately stays in verify-release-assets.mjs. Keeping
 * this module side-effect free lets us exercise every failure mode without
 * creating or mutating a real release.
 */

const MIN_SIGNATURE_LENGTH = 100

export class ReleaseContractError extends Error {
  constructor(message) {
    super(message)
    this.name = 'ReleaseContractError'
  }
}

/** Return unique updater URLs together with every platform key using them. */
export function collectUpdaterDownloads(manifest, releaseContext) {
  const platforms = manifest?.platforms
  if (!platforms || typeof platforms !== 'object' || Array.isArray(platforms)) {
    fail('latest.json.platforms must be an object')
  }

  const byUrl = new Map()
  for (const [platform, entry] of Object.entries(platforms)) {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
      fail(`${platform} entry must be an object`)
    }
    if (typeof entry.url !== 'string' || entry.url.length === 0) {
      fail(`${platform}.url is missing`)
    }
    try {
      new URL(entry.url)
    } catch {
      fail(`${platform}.url is not a valid URL: ${entry.url}`)
    }
    if (releaseContext) {
      releaseAssetName(platform, entry.url, releaseContext.tag, releaseContext.expectedRepository)
    }

    const platformsForUrl = byUrl.get(entry.url) ?? []
    platformsForUrl.push(platform)
    byUrl.set(entry.url, platformsForUrl)
  }

  if (byUrl.size === 0) fail('latest.json.platforms has no updater entries')
  return [...byUrl].map(([url, platformKeys]) => ({ url, platformKeys }))
}

/** Keep the production healer away from drafts and rolling prereleases. */
export function postPublishSkipReason(release) {
  if (release?.draft) return 'release is still a draft'
  if (release?.prerelease) return 'release is a prerelease'
  return null
}

export function productionTagForVersion(version) {
  if (typeof version !== 'string' || !/^\d+\.\d+\.\d+$/.test(version)) {
    fail(`latest.json version must be an X.Y.Z production version, found "${version}"`)
  }
  return `v${version}`
}

function fail(message) {
  throw new ReleaseContractError(message)
}

function exactlyOne(assets, predicate, description) {
  const matches = assets.filter(predicate)
  if (matches.length !== 1) {
    fail(`expected exactly one ${description} asset, found ${matches.length}`)
  }
  return matches[0]
}

function assertNonEmpty(asset) {
  if (!Number.isFinite(asset.size) || asset.size <= 0) {
    fail(`asset ${asset.name} is empty (0 bytes) — a partial upload`)
  }
}

function releaseAssetName(platform, url, tag, expectedRepository) {
  if (typeof url !== 'string') {
    fail(`${platform}.url is missing`)
  }

  let parsed
  try {
    parsed = new URL(url)
  } catch {
    fail(`${platform}.url is not a valid URL: ${url}`)
  }

  if (parsed.protocol !== 'https:' || parsed.hostname !== 'github.com' || parsed.port) {
    fail(`${platform}.url must use https://github.com: ${url}`)
  }
  if (parsed.username || parsed.password || parsed.search || parsed.hash) {
    fail(`${platform}.url must be a canonical GitHub release asset URL without credentials, query or fragment: ${url}`)
  }

  const repositoryParts = expectedRepository?.split('/') ?? []
  if (repositoryParts.length !== 2 || repositoryParts.some((part) => !part)) {
    fail(`expected repository must use owner/repo format, found "${expectedRepository}"`)
  }

  const encodedParts = parsed.pathname.split('/')
  if (
    encodedParts.length !== 7 ||
    encodedParts[0] !== '' ||
    encodedParts.slice(1).some((part) => part.length === 0)
  ) {
    fail(`${platform}.url path is not canonical: ${url}`)
  }

  let parts
  try {
    parts = encodedParts.slice(1).map(decodeURIComponent)
  } catch {
    fail(`${platform}.url has an invalid encoded path: ${url}`)
  }
  if (
    parts.length !== 6 ||
    parts[0].toLowerCase() !== repositoryParts[0].toLowerCase() ||
    parts[1].toLowerCase() !== repositoryParts[1].toLowerCase() ||
    parts[2] !== 'releases' ||
    parts[3] !== 'download' ||
    parts[4] !== tag
  ) {
    fail(`${platform}.url is not a ${expectedRepository} release asset on tag ${tag}: ${url}`)
  }
  if (!parts[5] || parts[5].includes('/')) fail(`${platform}.url has no valid release asset name: ${url}`)
  return parts[5]
}

/**
 * Rewrite tauri-action v1 GitHub API asset URLs to stable public release URLs.
 *
 * v1 identifies an uploaded asset by its API ID. Resolve that ID only against
 * the assets on the release being normalized, then construct the public URL
 * from the workflow-controlled tag and the authoritative asset name. Do not
 * trust browser_download_url here: draft releases can expose an untagged
 * placeholder until publication.
 */
export function normalizeUpdaterAssetUrls({
  manifest,
  assets,
  tag,
  expectedRepository,
}) {
  if (!manifest || typeof manifest !== 'object' || Array.isArray(manifest)) {
    fail('latest.json must be an object')
  }
  const platforms = manifest.platforms
  if (!platforms || typeof platforms !== 'object' || Array.isArray(platforms)) {
    fail('latest.json.platforms must be an object')
  }
  if (!Array.isArray(assets)) fail('release assets must be an array')
  if (typeof tag !== 'string' || !tag || tag.includes('/') || /[\x00-\x1f\x7f]/.test(tag)) {
    fail(`release tag is not safe for a public download URL: "${tag}"`)
  }

  const repositoryParts = expectedRepository?.split('/') ?? []
  if (repositoryParts.length !== 2 || repositoryParts.some((part) => !part)) {
    fail(`expected repository must use owner/repo format, found "${expectedRepository}"`)
  }
  const [owner, repository] = repositoryParts

  const assetsById = new Map()
  for (const asset of assets) {
    if (!Number.isSafeInteger(asset?.id) || asset.id <= 0) {
      fail(`release asset has an invalid API id: ${asset?.id}`)
    }
    if (assetsById.has(asset.id)) fail(`release contains duplicate asset API id ${asset.id}`)
    if (typeof asset.name !== 'string' || !asset.name || asset.name.includes('/')) {
      fail(`release asset ${asset.id} has an invalid name`)
    }
    assetsById.set(asset.id, asset)
  }

  let rewrittenCount = 0
  const normalizedPlatforms = {}
  for (const [platform, entry] of Object.entries(platforms)) {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
      fail(`${platform} entry must be an object`)
    }
    if (typeof entry.url !== 'string' || !entry.url) fail(`${platform}.url is missing`)

    let parsed
    try {
      parsed = new URL(entry.url)
    } catch {
      fail(`${platform}.url is not a valid URL: ${entry.url}`)
    }

    let url = entry.url
    if (parsed.hostname === 'github.com') {
      releaseAssetName(platform, url, tag, expectedRepository)
    } else {
      if (
        parsed.protocol !== 'https:' ||
        parsed.hostname !== 'api.github.com' ||
        parsed.port ||
        parsed.username ||
        parsed.password ||
        parsed.search ||
        parsed.hash
      ) {
        fail(`${platform}.url is neither a canonical release URL nor a GitHub asset API URL: ${entry.url}`)
      }

      const encodedParts = parsed.pathname.split('/')
      if (
        encodedParts.length !== 7 ||
        encodedParts[0] !== '' ||
        encodedParts.slice(1).some((part) => part.length === 0)
      ) {
        fail(`${platform}.url is not a canonical GitHub asset API path: ${entry.url}`)
      }

      let parts
      try {
        parts = encodedParts.slice(1).map(decodeURIComponent)
      } catch {
        fail(`${platform}.url has an invalid encoded API path: ${entry.url}`)
      }
      if (
        parts[0] !== 'repos' ||
        parts[1].toLowerCase() !== owner.toLowerCase() ||
        parts[2].toLowerCase() !== repository.toLowerCase() ||
        parts[3] !== 'releases' ||
        parts[4] !== 'assets' ||
        !/^\d+$/.test(parts[5]) ||
        parts.length !== 6
      ) {
        fail(`${platform}.url is not a ${expectedRepository} GitHub asset API URL: ${entry.url}`)
      }

      const assetId = Number(parts[5])
      if (!Number.isSafeInteger(assetId) || assetId <= 0) {
        fail(`${platform}.url contains an invalid release asset API id: ${parts[5]}`)
      }
      const asset = assetsById.get(assetId)
      if (!asset) fail(`${platform}.url references unknown release asset API id ${assetId}`)
      url = `https://github.com/${encodeURIComponent(owner)}/${encodeURIComponent(repository)}` +
        `/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(asset.name)}`
      rewrittenCount += 1
    }

    normalizedPlatforms[platform] = { ...entry, url }
  }

  if (Object.keys(normalizedPlatforms).length === 0) {
    fail('latest.json.platforms has no updater entries')
  }

  return {
    manifest: { ...manifest, platforms: normalizedPlatforms },
    rewrittenCount,
  }
}

/** Guard destructive latest.json replacement against a stale/wrong release. */
export function validateManifestNormalizationTarget({
  release,
  releaseId,
  tag,
  channel,
}) {
  if (!release || typeof release !== 'object' || Array.isArray(release)) {
    fail('GitHub release response must be an object')
  }
  if (!Number.isSafeInteger(releaseId) || releaseId <= 0 || release.id !== releaseId) {
    fail(`GitHub release id ${release.id} does not match requested release ${releaseId}`)
  }

  let expectedDraft
  let expectedPrerelease
  if (channel === 'production') {
    if (!/^v\d+\.\d+\.\d+$/.test(tag)) {
      fail(`production manifest normalization requires a vX.Y.Z tag, found "${tag}"`)
    }
    expectedDraft = true
    expectedPrerelease = false
  } else if (channel === 'staging') {
    if (tag !== 'staging') {
      fail(`staging manifest normalization requires the staging tag, found "${tag}"`)
    }
    expectedDraft = false
    expectedPrerelease = true
  } else {
    fail(`manifest normalization channel must be production or staging, found "${channel}"`)
  }

  if (release.tag_name !== tag) {
    fail(`release ${releaseId} belongs to tag "${release.tag_name}", not "${tag}"`)
  }
  if (release.draft !== expectedDraft || release.prerelease !== expectedPrerelease) {
    fail(
      `release ${releaseId} has unexpected channel state ` +
      `(draft=${release.draft}, prerelease=${release.prerelease})`,
    )
  }
}

function signatureText(signatureTexts, name) {
  const value = signatureTexts instanceof Map
    ? signatureTexts.get(name)
    : signatureTexts?.[name]
  if (typeof value !== 'string') {
    fail(`signature asset ${name} was not downloaded for content verification`)
  }
  return value
}

/**
 * Validate one GitHub Release payload and its downloaded signature contents.
 *
 * The default remains the legacy Windows contract for callers that explicitly
 * validate old releases. Current staging and production workflows pass
 * requireMacosUniversal to require one DMG for direct installation plus one
 * signed universal .app.tar.gz referenced by both default macOS updater
 * architecture keys.
 */
export function validateReleaseContract({
  tag,
  expectVersion,
  expectedRepository,
  assets,
  manifest,
  signatureTexts,
  requireMacosUniversal = false,
}) {
  if (!tag) fail('release tag is required')
  if (!Array.isArray(assets)) fail('release assets must be an array')

  const messages = []
  const names = assets.map((asset) => asset.name)
  if (new Set(names).size !== names.length) {
    fail('release contains duplicate asset names')
  }

  const manifestAsset = exactlyOne(
    assets,
    (asset) => asset.name === 'latest.json',
    'latest.json',
  )
  assertNonEmpty(manifestAsset)

  if (!manifest || typeof manifest !== 'object' || Array.isArray(manifest)) {
    fail('latest.json root must be an object')
  }
  if (expectVersion && manifest.version !== expectVersion) {
    fail(`latest.json version "${manifest.version}" != expected "${expectVersion}"`)
  }
  if (expectVersion) messages.push(`manifest version matches: ${manifest.version}`)

  const platforms = manifest.platforms
  if (!platforms || typeof platforms !== 'object' || Array.isArray(platforms)) {
    fail('latest.json.platforms must be an object')
  }

  const requiredPlatforms = ['windows-x86_64']
  if (requireMacosUniversal) {
    requiredPlatforms.push('darwin-aarch64', 'darwin-x86_64')
  }
  for (const platform of requiredPlatforms) {
    if (!platforms[platform]) {
      fail(`latest.json has no platforms."${platform}" entry (platforms: ${JSON.stringify(Object.keys(platforms))})`)
    }
  }

  const windowsInstaller = exactlyOne(
    assets,
    (asset) => /-setup\.exe$/i.test(asset.name),
    '*-setup.exe',
  )
  assertNonEmpty(windowsInstaller)

  const referencedAssets = new Map()
  for (const [platform, entry] of Object.entries(platforms)) {
    if (!entry || typeof entry !== 'object' || Array.isArray(entry)) {
      fail(`${platform} entry must be an object`)
    }
    if (typeof entry.signature !== 'string' || entry.signature.trim().length < MIN_SIGNATURE_LENGTH) {
      fail(`${platform}.signature is missing or suspiciously short — was the build signed?`)
    }

    const assetName = releaseAssetName(platform, entry.url, tag, expectedRepository)
    const asset = assets.find((candidate) => candidate.name === assetName)
    if (!asset) {
      fail(`${platform}.url points at "${assetName}" but that asset is not in this release`)
    }
    assertNonEmpty(asset)

    const signatureAssetName = `${assetName}.sig`
    const signatureAsset = assets.find((candidate) => candidate.name === signatureAssetName)
    if (!signatureAsset) fail(`missing signature asset "${signatureAssetName}" for ${platform}`)
    assertNonEmpty(signatureAsset)

    if (signatureText(signatureTexts, signatureAssetName) !== entry.signature) {
      fail(`${platform}.signature does not match uploaded asset ${signatureAssetName}`)
    }

    referencedAssets.set(platform, assetName)
    messages.push(`${platform} points at signed asset ${assetName}`)
  }

  if (referencedAssets.get('windows-x86_64') !== windowsInstaller.name) {
    fail(`windows-x86_64 points at "${referencedAssets.get('windows-x86_64')}" but the uploaded NSIS installer is "${windowsInstaller.name}"`)
  }

  if (requireMacosUniversal) {
    const dmg = exactlyOne(assets, (asset) => /\.dmg$/i.test(asset.name), '*.dmg')
    assertNonEmpty(dmg)
    const armAsset = referencedAssets.get('darwin-aarch64')
    const intelAsset = referencedAssets.get('darwin-x86_64')
    if (armAsset !== intelAsset) {
      fail(`universal macOS entries must reference the same updater asset: darwin-aarch64=${armAsset}, darwin-x86_64=${intelAsset}`)
    }
    if (!/\.app\.tar\.gz$/i.test(armAsset)) {
      fail(`macOS updater asset must end in .app.tar.gz, found "${armAsset}"`)
    }
    const updaterArchive = exactlyOne(
      assets,
      (asset) => /\.app\.tar\.gz$/i.test(asset.name),
      '*.app.tar.gz',
    )
    assertNonEmpty(updaterArchive)
    if (armAsset !== updaterArchive.name) {
      fail(`universal macOS entries point at "${armAsset}" but the uploaded updater archive is "${updaterArchive.name}"`)
    }
    if (platforms['darwin-aarch64'].signature !== platforms['darwin-x86_64'].signature) {
      fail('universal macOS entries must carry the same updater signature')
    }

    messages.push(`macOS universal updater contract and DMG present: ${armAsset}, ${dmg.name}`)
  }

  return messages
}
