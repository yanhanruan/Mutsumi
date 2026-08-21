import assert from 'node:assert/strict'
import test from 'node:test'

import {
  classifyCodesignEntitlements,
  developerIdRequirementForTeam,
  MacosBundleContractError,
  parseMacosBundleArgs,
  validateMacosBundleContract,
} from './macos-bundle-contract.mjs'

const SIGNATURE_DETAILS = [
  'CodeDirectory v=20500 size=12345 flags=0x10000(runtime) hashes=350+7 location=embedded',
  'Authority=(unavailable)',
  'TeamIdentifier=TEAM123456',
  'Timestamp=Aug 21, 2026 at 5:00:00 PM',
].join('\n')

function snapshot(overrides = {}) {
  return {
    appName: 'mutsumi.app',
    executableName: 'Mutsumi',
    executablePresent: true,
    architectures: ['x86_64', 'arm64'],
    minimumSystemVersion: '13.0',
    identifier: 'com.mutsumi.app',
    packageType: 'APPL',
    uiElement: undefined,
    backgroundOnly: undefined,
    dmgNames: ['mutsumi_1.5.3_universal.dmg'],
    dmgIntegrityValid: true,
    ...overrides,
  }
}

function signedSnapshot(overrides = {}) {
  return snapshot({
    identifier: 'com.example.mutsumi',
    signatureValid: true,
    signatureDetails: SIGNATURE_DETAILS,
    developerIdRequirementValid: true,
    entitlementsReadable: true,
    entitlementsPresent: true,
    entitlementsText: '<?xml version="1.0"?><plist><dict/></plist>',
    entitlementsDiagnostics: 'Executable=/Applications/Mutsumi.app/Contents/MacOS/Mutsumi',
    appGatekeeperAccepted: true,
    dmgGatekeeperAccepted: true,
    dmgStapleValid: true,
    ...overrides,
  })
}

const SIGNED_OPTIONS = {
  mode: 'signed',
  expectedIdentifier: 'com.example.mutsumi',
}

test('CLI requires an explicit mode and accepts the documented unsigned invocation', () => {
  assert.deepEqual(
    parseMacosBundleArgs([
      '--bundle-dir', 'target/bundle',
      '--mode', 'unsigned',
      '--expected-minimum-system-version', '13.0',
    ]),
    {
      bundleDir: 'target/bundle',
      mode: 'unsigned',
      expectedIdentifier: undefined,
      expectedMinimumSystemVersion: '13.0',
    },
  )
  assert.throws(
    () => parseMacosBundleArgs(['--bundle-dir', 'target/bundle']),
    /--mode is required/,
  )
})

test('CLI rejects typoed, duplicate and positional arguments', () => {
  assert.throws(
    () => parseMacosBundleArgs(['--bundle-dir', 'target/bundle', '--mdoe', 'signed']),
    /unknown or positional argument: "--mdoe"/,
  )
  assert.throws(
    () => parseMacosBundleArgs([
      '--bundle-dir', 'target/bundle',
      '--mode', 'unsigned',
      '--mode', 'signed',
    ]),
    /duplicate argument: "--mode"/,
  )
  assert.throws(
    () => parseMacosBundleArgs(['--bundle-dir', 'target/bundle', '--mode', 'unsigned', 'extra']),
    /unknown or positional argument: "extra"/,
  )
})

test('CLI requires signed identity only for signed mode', () => {
  assert.throws(
    () => parseMacosBundleArgs(['--bundle-dir', 'target/bundle', '--mode', 'signed']),
    /signed mode requires --expected-identifier/,
  )
  assert.throws(
    () => parseMacosBundleArgs([
      '--bundle-dir', 'target/bundle',
      '--mode', 'unsigned',
      '--expected-identifier', 'com.example.mutsumi',
    ]),
    /only valid in signed mode/,
  )
})

test('classifies codesign entitlement stdout separately from diagnostics', () => {
  assert.deepEqual(
    classifyCodesignEntitlements({
      ok: true,
      stdout: '',
      stderr: 'Executable=/Applications/Mutsumi.app/Contents/MacOS/Mutsumi\n',
    }),
    {
      entitlementsReadable: true,
      entitlementsPresent: false,
      entitlementsText: '',
      entitlementsDiagnostics: 'Executable=/Applications/Mutsumi.app/Contents/MacOS/Mutsumi',
    },
  )

  const abstract = '[Dict]\n\t[Key] com.apple.security.get-task-allow\n\t[Value]\n\t\t[Bool] true\n'
  assert.deepEqual(
    classifyCodesignEntitlements({ ok: true, stdout: abstract, stderr: 'Executable=...' }),
    {
      entitlementsReadable: true,
      entitlementsPresent: true,
      entitlementsText: abstract.trim(),
      entitlementsDiagnostics: 'Executable=...',
    },
  )
})

test('treats exit-zero invalid entitlement diagnostics as unreadable', () => {
  const result = classifyCodesignEntitlements({
    ok: true,
    stdout: '',
    stderr: 'warning: binary contains an invalid entitlements blob. The OS will ignore these entitlements.',
  })
  assert.equal(result.entitlementsReadable, false)
  assert.equal(result.entitlementsPresent, false)
})

test('builds a codesign-evaluated Developer ID requirement for numeric or letter Team IDs', () => {
  assert.equal(
    developerIdRequirementForTeam('2DC432GLL2'),
    'anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf[subject.OU] = "2DC432GLL2"',
  )
  assert.match(developerIdRequirementForTeam('TEAM123456'), /"TEAM123456"$/)
  assert.throws(
    () => developerIdRequirementForTeam('not-a-team'),
    /invalid Developer Team identifier/,
  )
})

test('accepts the current unsigned universal bundle and warns about deferred identity', () => {
  const result = validateMacosBundleContract(snapshot())
  assert.equal(result.messages.length, 4)
  assert.match(result.warnings[0], /freeze a replacement before signed release/)
})

test('requires a Dock-eligible foreground application bundle', () => {
  assert.throws(
    () => validateMacosBundleContract(snapshot({ packageType: 'FMWK' })),
    /CFBundlePackageType must be APPL/,
  )
  assert.throws(
    () => validateMacosBundleContract(snapshot({ uiElement: true })),
    /LSUIElement must be absent or false/,
  )
  assert.throws(
    () => validateMacosBundleContract(snapshot({ backgroundOnly: true })),
    /LSBackgroundOnly must be absent or false/,
  )
})

test('requires exactly the two universal Mach-O architectures', () => {
  assert.throws(
    () => validateMacosBundleContract(snapshot({ architectures: null })),
    /architecture list is missing or malformed/,
  )
  assert.throws(
    () => validateMacosBundleContract(snapshot({ architectures: ['arm64'] })),
    /must contain exactly arm64 and x86_64/,
  )
  assert.throws(
    () => validateMacosBundleContract(snapshot({ architectures: ['arm64', 'x86_64', 'i386'] })),
    /must contain exactly arm64 and x86_64/,
  )
})

test('requires a single in-bundle executable name and an executable file', () => {
  assert.throws(
    () => validateMacosBundleContract(snapshot({ executableName: '../Mutsumi' })),
    /CFBundleExecutable must be one file name/,
  )
  assert.throws(
    () => validateMacosBundleContract(snapshot({ executablePresent: false })),
    /bundle executable is missing or not executable/,
  )
})

test('enforces the declared minimum system version', () => {
  assert.throws(
    () => validateMacosBundleContract(snapshot({ minimumSystemVersion: '12.0' })),
    /expected "13.0"/,
  )
})

test('requires one structurally valid DMG that passes hdiutil', () => {
  assert.throws(
    () => validateMacosBundleContract(snapshot({ dmgNames: null })),
    /DMG list is missing or malformed/,
  )
  assert.throws(
    () => validateMacosBundleContract(snapshot({ dmgNames: [] })),
    /expected exactly one DMG/,
  )
  assert.throws(
    () => validateMacosBundleContract(snapshot({ dmgIntegrityValid: false })),
    /hdiutil verification failed/,
  )
})

test('accepts a complete signed, hardened and notarized bundle snapshot', () => {
  const result = validateMacosBundleContract(signedSnapshot(), SIGNED_OPTIONS)
  assert.equal(result.warnings.length, 0)
  assert.match(result.messages.at(-1), /Gatekeeper accepts/)
})

test('signed mode requires a frozen non-.app identifier and exact bundle match', () => {
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot(), { mode: 'signed' }),
    /requires --expected-identifier/,
  )
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot(), {
      mode: 'signed',
      expectedIdentifier: 'com.mutsumi.APP',
    }),
    /must not end in \.app/,
  )
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot(), {
      mode: 'signed',
      expectedIdentifier: 'not a bundle identifier',
    }),
    /identifier is malformed/,
  )
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot(), {
      mode: 'signed',
      expectedIdentifier: 'com.example.other',
    }),
    /CFBundleIdentifier.*expected/,
  )
})

test('rejects a missing or malformed identifier in every mode', () => {
  assert.throws(
    () => validateMacosBundleContract(snapshot({ identifier: '' })),
    /CFBundleIdentifier is missing or malformed/,
  )
  assert.throws(
    () => validateMacosBundleContract(snapshot({ identifier: 'com.example.invalid/id' })),
    /CFBundleIdentifier is missing or malformed/,
  )
})

test('rejects a failed signature or failed Developer ID requirement evaluation', () => {
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot({ signatureValid: false }), SIGNED_OPTIONS),
    /codesign --verify/,
  )
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot({
      developerIdRequirementValid: false,
    }), SIGNED_OPTIONS),
    /codesign rejected the Developer ID Application requirement/,
  )
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot({
      signatureDetails: SIGNATURE_DETAILS.replace('TeamIdentifier=TEAM123456', 'TeamIdentifier=not-set'),
    }), SIGNED_OPTIONS),
    /missing a valid Developer Team identifier/,
  )
})

test('requires hardened runtime and a secure timestamp', () => {
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot({
      signatureDetails: SIGNATURE_DETAILS.replace('(runtime)', ''),
    }), SIGNED_OPTIONS),
    /hardened runtime flag is missing/,
  )
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot({
      signatureDetails: SIGNATURE_DETAILS.replace('Timestamp=', 'Signed Time='),
    }), SIGNED_OPTIONS),
    /secure signing timestamp is missing/,
  )
})

test('rejects unreadable or debug-enabled distribution entitlements', () => {
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot({ entitlementsReadable: false }), SIGNED_OPTIONS),
    /embedded entitlements could not be read/,
  )
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot({
      entitlementsText: '<plist><dict><key>com.apple.security.get-task-allow</key><true/></dict></plist>',
    }), SIGNED_OPTIONS),
    /must not enable com.apple.security.get-task-allow/,
  )
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot({
      entitlementsText: [
        '[Dict]',
        '\t[Key] com.apple.security.get-task-allow',
        '\t[Value]',
        '\t\t[Bool] true',
      ].join('\n'),
    }), SIGNED_OPTIONS),
    /must not enable com.apple.security.get-task-allow/,
  )
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot({
      entitlementsReadable: false,
      entitlementsPresent: false,
      entitlementsText: '',
      entitlementsDiagnostics: 'warning: binary contains an invalid entitlements blob. The OS will ignore these entitlements.',
    }), SIGNED_OPTIONS),
    /entitlements are invalid or could not be decoded/,
  )
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot({
      entitlementsPresent: true,
      entitlementsText: 'not a recognized entitlement representation',
    }), SIGNED_OPTIONS),
    /entitlement representation is not recognized/,
  )
})

test('allows get-task-allow when explicitly false', () => {
  const result = validateMacosBundleContract(signedSnapshot({
    entitlementsText: '<plist><dict><key>com.apple.security.get-task-allow</key><false/></dict></plist>',
  }), SIGNED_OPTIONS)
  assert.match(result.messages.join('\n'), /entitlements are readable/)

  const abstractResult = validateMacosBundleContract(signedSnapshot({
    entitlementsText: [
      '[Dict]',
      '\t[Key] com.apple.security.get-task-allow',
      '\t[Value]',
      '\t\t[Bool] false',
    ].join('\n'),
  }), SIGNED_OPTIONS)
  assert.match(abstractResult.messages.join('\n'), /entitlements are readable/)
})

test('allows an explicit no-entitlements result', () => {
  const result = validateMacosBundleContract(signedSnapshot({
    entitlementsPresent: false,
    entitlementsText: '',
  }), SIGNED_OPTIONS)
  assert.match(result.messages.join('\n'), /entitlements are readable/)
})

test('requires Gatekeeper acceptance for both app and DMG', () => {
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot({ appGatekeeperAccepted: false }), SIGNED_OPTIONS),
    /Gatekeeper rejected the app bundle/,
  )
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot({ dmgGatekeeperAccepted: false }), SIGNED_OPTIONS),
    /Gatekeeper rejected the DMG/,
  )
})

test('requires the notarization ticket on the outer DMG', () => {
  assert.throws(
    () => validateMacosBundleContract(signedSnapshot({ dmgStapleValid: false }), SIGNED_OPTIONS),
    /ticket is not stapled to the DMG/,
  )
})

test('reports invalid modes as contract errors', () => {
  assert.throws(
    () => validateMacosBundleContract(snapshot(), { mode: 'future' }),
    (error) => error instanceof MacosBundleContractError && /mode must be/.test(error.message),
  )
})
