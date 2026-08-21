/** Pure validation and parsers for the opt-in macOS lifecycle smoke test. */

export class MacosLifecycleContractError extends Error {
  constructor(message) {
    super(message)
    this.name = 'MacosLifecycleContractError'
  }
}

function fail(message) {
  throw new MacosLifecycleContractError(message)
}

function normalizeBoolean(value) {
  if (value === 'true') return true
  if (value === 'false') return false
  return value
}

export function normalizeLsappinfoAsn(value) {
  const match = String(value ?? '').match(
    /ASN:(0x[0-9a-f]+)-(0x[0-9a-f]+)(?:-"[^"\r\n]*")?:/i,
  )
  return match ? `ASN:${match[1].toLowerCase()}-${match[2].toLowerCase()}:` : null
}

export function parseLsappinfoAsns(output) {
  const matches = String(output ?? '').matchAll(
    /ASN:(0x[0-9a-f]+)-(0x[0-9a-f]+)(?:-"[^"\r\n]*")?:/gi,
  )
  return [...new Set([...matches].map((match) => (
    `ASN:${match[1].toLowerCase()}-${match[2].toLowerCase()}:`
  )))]
}

export function parseLsappinfoInfo(output) {
  const facts = {}
  for (const line of String(output ?? '').split(/\r?\n/)) {
    const match = line.match(/^"([^"]+)"=(.*)$/)
    if (!match) continue

    const [, key, rawValue] = match
    if (rawValue.startsWith('"') && rawValue.endsWith('"')) {
      facts[key] = rawValue.slice(1, -1)
    } else if (/^-?\d+$/.test(rawValue)) {
      facts[key] = Number(rawValue)
    } else {
      facts[key] = normalizeBoolean(rawValue)
    }
  }
  return facts
}

export function discoverLifecycleOwnership({
  currentAsns = [],
  currentPids = [],
  infoByAsn = {},
  expectedAppPath,
} = {}) {
  if (!expectedAppPath) fail('expected app path is required for ownership discovery')
  if (!Array.isArray(currentAsns) || !Array.isArray(currentPids)) {
    fail('current ASN and PID lists must be arrays')
  }

  const matchingAsns = currentAsns.filter((asn) => infoByAsn[asn]?.LSBundlePath === expectedAppPath)
  if (matchingAsns.length > 1 || currentPids.length > 1) {
    fail('launched application ownership is ambiguous')
  }

  const ownedAsn = matchingAsns[0] ?? null
  const asnPid = ownedAsn ? infoByAsn[ownedAsn]?.pid : null
  const ownedPid = Number.isInteger(asnPid) && currentPids.includes(asnPid)
    ? asnPid
    : (currentPids[0] ?? null)

  return { ownedAsn, ownedPid }
}

export function lifecycleCleanupState({
  ownedPid,
  currentAsns = [],
  currentPids = [],
  socketPresent = false,
} = {}) {
  const ownershipKnown = Number.isInteger(ownedPid)
  const noMatchingApplication = currentAsns.length === 0 && currentPids.length === 0
  const complete = ownershipKnown && noMatchingApplication
  return {
    complete,
    removeSocket: complete && socketPresent === true,
  }
}

const CLI_VALUE_OPTIONS = new Set(['--app', '--arch', '--timeout-ms'])

export function parseMacosLifecycleArgs(argv) {
  const values = new Map()
  for (let index = 0; index < argv.length; index += 1) {
    const option = argv[index]
    if (!CLI_VALUE_OPTIONS.has(option)) {
      fail(`unknown or positional argument: "${option}"`)
    }
    if (values.has(option)) fail(`duplicate argument: "${option}"`)

    const value = argv[index + 1]
    if (!value || value.startsWith('--')) fail(`${option} requires a value`)
    values.set(option, value)
    index += 1
  }

  const appPath = values.get('--app')
  if (!appPath) fail('--app is required')

  const architecture = values.get('--arch')
  if (architecture !== 'arm64' && architecture !== 'x86_64') {
    fail('--arch must be arm64 or x86_64')
  }

  const timeoutText = values.get('--timeout-ms') ?? '15000'
  if (!/^\d+$/.test(timeoutText)) fail('--timeout-ms must be an integer')
  const timeoutMs = Number(timeoutText)
  if (timeoutMs < 1000 || timeoutMs > 60000) {
    fail('--timeout-ms must be between 1000 and 60000')
  }

  return { appPath, architecture, timeoutMs }
}

function requireTrue(value, message) {
  if (value !== true) fail(message)
}

export function validateMacosLifecycleSnapshot(snapshot, {
  expectedAppPath,
  expectedIdentifier,
  expectedArchitecture,
} = {}) {
  if (!snapshot || typeof snapshot !== 'object') fail('lifecycle snapshot is required')
  if (!expectedAppPath) fail('expected app path is required')
  if (!expectedIdentifier) fail('expected bundle identifier is required')
  if (expectedArchitecture !== 'arm64' && expectedArchitecture !== 'x86_64') {
    fail('expected architecture must be arm64 or x86_64')
  }

  const messages = []
  if (snapshot.bundlePath !== expectedAppPath) {
    fail(`LaunchServices registered "${snapshot.bundlePath}", expected "${expectedAppPath}"`)
  }
  if (snapshot.bundleIdentifier !== expectedIdentifier) {
    fail(`LaunchServices registered bundle id "${snapshot.bundleIdentifier}", expected "${expectedIdentifier}"`)
  }
  if (snapshot.packageType !== 'APPL') {
    fail(`CFBundlePackageType must be APPL, found "${snapshot.packageType}"`)
  }
  if (snapshot.uiElement !== undefined && snapshot.uiElement !== false) {
    fail('LSUIElement must be absent or false for a permanent Dock icon')
  }
  if (snapshot.backgroundOnly !== undefined && snapshot.backgroundOnly !== false) {
    fail('LSBackgroundOnly must be absent or false for a foreground app')
  }
  if (snapshot.applicationType !== 'Foreground') {
    fail(`LaunchServices application type must be Foreground, found "${snapshot.applicationType}"`)
  }
  requireTrue(snapshot.canBecomeFrontmost, 'application cannot become frontmost')
  requireTrue(snapshot.registered, 'application did not register with LaunchServices')
  requireTrue(snapshot.signalledReady, 'application did not signal launch readiness')
  requireTrue(snapshot.readyToBeFrontable, 'application is not ready to become frontmost')
  if (snapshot.hidden !== false) fail('application unexpectedly launched hidden')
  if (snapshot.architecture !== expectedArchitecture) {
    fail(`LaunchServices architecture is "${snapshot.architecture}", expected "${expectedArchitecture}"`)
  }
  requireTrue(snapshot.initialFrontmost, 'initial launch did not make Mutsumi frontmost')
  messages.push(`LaunchServices foreground/Dock contract passed for ${snapshot.architecture}`)

  requireTrue(snapshot.socketCreated, 'single-instance socket was not created')
  if (snapshot.relaunchInstanceCount !== 1 || snapshot.relaunchProcessCount !== 1) {
    fail(
      `second launch left ${snapshot.relaunchInstanceCount} LaunchServices registrations `
      + `and ${snapshot.relaunchProcessCount} app processes`,
    )
  }
  requireTrue(snapshot.relaunchSameAsn, 'second launch replaced the original application instance')
  requireTrue(snapshot.relaunchSamePid, 'second launch replaced the original application process')
  requireTrue(snapshot.relaunchFrontmost, 'second launch did not activate the existing application')
  messages.push('second launch reused and activated the existing instance')

  requireTrue(snapshot.gracefulQuitRequested, 'standard macOS Quit Apple Event failed')
  requireTrue(snapshot.exitedAfterQuit, 'application remained registered or running after Quit')
  requireTrue(snapshot.socketRemovedAfterQuit, 'single-instance socket remained after normal Quit')
  messages.push('normal Quit removed the process, LaunchServices registration and socket')

  return { messages }
}
