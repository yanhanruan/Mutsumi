/** Pure validation for collected macOS app/DMG bundle facts. */

export class MacosBundleContractError extends Error {
  constructor(message) {
    super(message)
    this.name = 'MacosBundleContractError'
  }
}

function fail(message) {
  throw new MacosBundleContractError(message)
}

function hasTrueEntitlement(plist, key) {
  const escaped = key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const xml = new RegExp(`<key>\\s*${escaped}\\s*</key>\\s*<true\\s*/>`, 'i')
  const abstract = new RegExp(
    `\\[Key\\]\\s*${escaped}\\s*\\r?\\n\\s*\\[Value\\]\\s*\\r?\\n\\s*\\[Bool\\]\\s*true`,
    'i',
  )
  return xml.test(plist) || abstract.test(plist)
}

function requireSignedFact(value, message) {
  if (value !== true) fail(message)
}

function isBundleIdentifier(value) {
  return typeof value === 'string'
    && /^[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+$/.test(value)
}

export function classifyCodesignEntitlements({ ok, stdout = '', stderr = '' }) {
  const entitlementsText = stdout.trim()
  const entitlementsDiagnostics = stderr.trim()
  const invalidOutput = /invalid entitlements blob|internal error in Code Signing subsystem/i
    .test(`${entitlementsText}\n${entitlementsDiagnostics}`)

  return {
    entitlementsReadable: ok === true && !invalidOutput,
    entitlementsPresent: entitlementsText.length > 0,
    entitlementsText,
    entitlementsDiagnostics,
  }
}

export function developerIdRequirementForTeam(teamIdentifier) {
  if (!/^[A-Z0-9]{10}$/.test(teamIdentifier ?? '')) {
    fail(`invalid Developer Team identifier: "${teamIdentifier}"`)
  }
  return [
    'anchor apple generic',
    'certificate leaf[field.1.2.840.113635.100.6.1.13] exists',
    `certificate leaf[subject.OU] = "${teamIdentifier}"`,
  ].join(' and ')
}

const CLI_VALUE_OPTIONS = new Set([
  '--bundle-dir',
  '--mode',
  '--expected-identifier',
  '--expected-minimum-system-version',
])

export function parseMacosBundleArgs(argv) {
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

  const bundleDir = values.get('--bundle-dir')
  if (!bundleDir) fail('--bundle-dir is required')

  const mode = values.get('--mode')
  if (!mode) fail('--mode is required; choose unsigned or signed explicitly')
  if (mode !== 'unsigned' && mode !== 'signed') {
    fail(`mode must be "unsigned" or "signed", found "${mode}"`)
  }

  const expectedIdentifier = values.get('--expected-identifier')
  if (mode === 'signed' && !expectedIdentifier) {
    fail('signed mode requires --expected-identifier')
  }
  if (mode === 'unsigned' && expectedIdentifier) {
    fail('--expected-identifier is only valid in signed mode')
  }

  return {
    bundleDir,
    mode,
    expectedIdentifier,
    expectedMinimumSystemVersion: values.get('--expected-minimum-system-version') ?? '13.0',
  }
}

export function validateMacosBundleContract(snapshot, {
  mode = 'unsigned',
  expectedIdentifier,
  expectedMinimumSystemVersion = '13.0',
} = {}) {
  if (mode !== 'unsigned' && mode !== 'signed') {
    fail(`mode must be "unsigned" or "signed", found "${mode}"`)
  }
  if (!snapshot || typeof snapshot !== 'object') fail('bundle snapshot is required')

  const messages = []
  const warnings = []

  if (typeof snapshot.appName !== 'string' || !snapshot.appName.endsWith('.app')) {
    fail(`expected one .app bundle, found "${snapshot.appName}"`)
  }
  if (typeof snapshot.executableName !== 'string'
    || snapshot.executableName.length === 0
    || snapshot.executableName === '.'
    || snapshot.executableName === '..'
    || snapshot.executableName.includes('/')
    || snapshot.executableName.includes('\\')) {
    fail(`CFBundleExecutable must be one file name, found "${snapshot.executableName}"`)
  }
  if (snapshot.executablePresent !== true) fail('bundle executable is missing or not executable')

  if (!Array.isArray(snapshot.architectures)) fail('Mach-O architecture list is missing or malformed')
  const architectures = [...new Set(snapshot.architectures)].sort()
  if (architectures.join(',') !== 'arm64,x86_64') {
    fail(`app executable must contain exactly arm64 and x86_64, found [${architectures.join(', ')}]`)
  }
  messages.push('Mach-O contains arm64 and x86_64')

  if (snapshot.minimumSystemVersion !== expectedMinimumSystemVersion) {
    fail(`LSMinimumSystemVersion is "${snapshot.minimumSystemVersion}", expected "${expectedMinimumSystemVersion}"`)
  }
  messages.push(`minimum macOS is ${snapshot.minimumSystemVersion}`)

  if (!isBundleIdentifier(snapshot.identifier)) {
    fail(`CFBundleIdentifier is missing or malformed: "${snapshot.identifier}"`)
  }
  if (snapshot.identifier.toLowerCase().endsWith('.app')) {
    warnings.push(`bundle identifier "${snapshot.identifier}" ends in .app; freeze a replacement before signed release`)
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
  messages.push('bundle is a Dock-eligible foreground application')

  if (!Array.isArray(snapshot.dmgNames)) fail('DMG list is missing or malformed')
  const dmgNames = snapshot.dmgNames
  if (dmgNames.length !== 1
    || typeof dmgNames[0] !== 'string'
    || !dmgNames[0].toLowerCase().endsWith('.dmg')) {
    fail(`expected exactly one DMG, found [${dmgNames.join(', ')}]`)
  }
  if (snapshot.dmgIntegrityValid !== true) fail(`hdiutil verification failed for ${dmgNames[0]}`)
  messages.push(`DMG integrity verified: ${dmgNames[0]}`)

  if (mode === 'unsigned') return { messages, warnings }

  if (typeof expectedIdentifier !== 'string' || expectedIdentifier.length === 0) {
    fail('signed mode requires --expected-identifier')
  }
  if (!isBundleIdentifier(expectedIdentifier)) {
    fail(`signed release identifier is malformed: "${expectedIdentifier}"`)
  }
  if (expectedIdentifier.toLowerCase().endsWith('.app')) {
    fail(`signed release identifier must not end in .app: "${expectedIdentifier}"`)
  }
  if (snapshot.identifier !== expectedIdentifier) {
    fail(`CFBundleIdentifier is "${snapshot.identifier}", expected "${expectedIdentifier}"`)
  }

  requireSignedFact(snapshot.signatureValid, 'codesign --verify --deep --strict failed')
  const details = snapshot.signatureDetails ?? ''
  const teamIdentifier = details.match(/^TeamIdentifier=([A-Z0-9]{10})$/m)?.[1]
  if (!teamIdentifier) fail('app signature is missing a valid Developer Team identifier')
  requireSignedFact(
    snapshot.developerIdRequirementValid,
    `codesign rejected the Developer ID Application requirement for Team ${teamIdentifier}`,
  )
  if (!/^CodeDirectory\b[^\r\n]*\bflags=[^\r\n]*\bruntime\b/m.test(details)) {
    fail('hardened runtime flag is missing from the app signature')
  }
  if (!/^Timestamp=/m.test(details) || /^Signed Time=/m.test(details)) {
    fail('secure signing timestamp is missing')
  }
  messages.push('Developer ID signature, hardened runtime and secure timestamp verified')

  const entitlementsDiagnostics = snapshot.entitlementsDiagnostics ?? ''
  const entitlementsText = snapshot.entitlementsText ?? ''
  if (/invalid entitlements blob|internal error in Code Signing subsystem/i
    .test(`${entitlementsText}\n${entitlementsDiagnostics}`)) {
    fail('embedded entitlements are invalid or could not be decoded')
  }
  requireSignedFact(snapshot.entitlementsReadable, 'embedded entitlements could not be read')
  if (snapshot.entitlementsPresent !== true && snapshot.entitlementsPresent !== false) {
    fail('embedded entitlement presence was not classified')
  }
  if (snapshot.entitlementsPresent) {
    const recognizedFormat = /^\s*\[Dict\]/.test(entitlementsText)
      || /<plist(?:\s|>)/i.test(entitlementsText)
    if (!recognizedFormat) fail('embedded entitlement representation is not recognized')
  } else if (entitlementsText.length > 0) {
    fail('embedded entitlement output is inconsistent with the no-entitlements state')
  }
  if (hasTrueEntitlement(entitlementsText, 'com.apple.security.get-task-allow')) {
    fail('distribution build must not enable com.apple.security.get-task-allow')
  }
  messages.push('embedded entitlements are readable and do not enable get-task-allow')

  requireSignedFact(snapshot.appGatekeeperAccepted, 'Gatekeeper rejected the app bundle')
  requireSignedFact(snapshot.dmgGatekeeperAccepted, 'Gatekeeper rejected the DMG')
  requireSignedFact(snapshot.dmgStapleValid, 'notarization ticket is not stapled to the DMG')
  messages.push('Gatekeeper accepts the app and DMG; DMG notarization ticket is stapled')

  return { messages, warnings }
}
