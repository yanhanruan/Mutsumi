import assert from 'node:assert/strict'
import test from 'node:test'

import {
  discoverLifecycleOwnership,
  lifecycleCleanupState,
  macosInitialLaunchCommand,
  MacosLifecycleContractError,
  normalizeLsappinfoAsn,
  parseLsappinfoAsns,
  parseLsappinfoInfo,
  parseMacosLifecycleArgs,
  validateMacosLifecycleExecution,
  validateMacosLifecycleSnapshot,
} from './macos-lifecycle-contract.mjs'

const APP_PATH = '/Applications/Mutsumi.app'
const IDENTIFIER = 'com.example.mutsumi'

function snapshot(overrides = {}) {
  return {
    bundlePath: APP_PATH,
    bundleIdentifier: IDENTIFIER,
    packageType: 'APPL',
    uiElement: undefined,
    backgroundOnly: undefined,
    applicationType: 'Foreground',
    canBecomeFrontmost: true,
    registered: true,
    signalledReady: true,
    readyToBeFrontable: true,
    hidden: false,
    architecture: 'arm64',
    initialFrontmost: true,
    initialLaunch: 'open',
    socketCreated: true,
    relaunchInstanceCount: 1,
    relaunchProcessCount: 1,
    relaunchSameAsn: true,
    relaunchSamePid: true,
    relaunchFrontmost: true,
    gracefulQuitRequested: true,
    exitedAfterQuit: true,
    socketRemovedAfterQuit: true,
    ...overrides,
  }
}

const OPTIONS = {
  expectedAppPath: APP_PATH,
  expectedIdentifier: IDENTIFIER,
  expectedArchitecture: 'arm64',
  expectedInitialLaunch: 'open',
}

test('CLI requires an app and one supported architecture', () => {
  assert.deepEqual(
    parseMacosLifecycleArgs(['--app', APP_PATH, '--arch', 'x86_64']),
    { appPath: APP_PATH, architecture: 'x86_64', initialLaunch: 'open', timeoutMs: 15000 },
  )
  assert.throws(
    () => parseMacosLifecycleArgs(['--arch', 'arm64']),
    /--app is required/,
  )
  assert.throws(
    () => parseMacosLifecycleArgs(['--app', APP_PATH, '--arch', 'universal']),
    /must be arm64 or x86_64/,
  )
})

test('CLI accepts Finder initial launch and rejects unknown launch sources', () => {
  assert.deepEqual(
    parseMacosLifecycleArgs([
      '--app', APP_PATH,
      '--arch', 'arm64',
      '--initial-launch', 'finder',
    ]),
    { appPath: APP_PATH, architecture: 'arm64', initialLaunch: 'finder', timeoutMs: 15000 },
  )
  assert.throws(
    () => parseMacosLifecycleArgs([
      '--app', APP_PATH,
      '--arch', 'arm64',
      '--initial-launch', 'dock',
    ]),
    /must be open or finder/,
  )
})

test('builds a Finder Apple Event initial launch without shell interpolation', () => {
  assert.deepEqual(macosInitialLaunchCommand({
    appPath: APP_PATH,
    architecture: 'arm64',
    initialLaunch: 'finder',
  }), {
    command: 'osascript',
    args: [
      '-e', 'on run argv',
      '-e', 'set appFile to POSIX file (item 1 of argv)',
      '-e', 'tell application "Finder" to open appFile',
      '-e', 'end run',
      APP_PATH,
    ],
  })
})

test('CLI rejects malformed timeouts, typos, duplicates and positional arguments', () => {
  assert.throws(
    () => parseMacosLifecycleArgs(['--app', APP_PATH, '--arch', 'arm64', '--timeout-ms', 'soon']),
    /must be an integer/,
  )
  assert.throws(
    () => parseMacosLifecycleArgs(['--app', APP_PATH, '--arch', 'arm64', '--timeout-ms', '999']),
    /between 1000 and 60000/,
  )
  assert.throws(
    () => parseMacosLifecycleArgs(['--app', APP_PATH, '--arch', 'arm64', '--acr', 'x86_64']),
    /unknown or positional argument: "--acr"/,
  )
  assert.throws(
    () => parseMacosLifecycleArgs(['--app', APP_PATH, '--app', APP_PATH, '--arch', 'arm64']),
    /duplicate argument: "--app"/,
  )
})

test('execution preflight requires the requested architecture to run', () => {
  assert.doesNotThrow(() => validateMacosLifecycleExecution({
    requestedArchitecture: 'arm64',
    executableAvailable: true,
  }))
  assert.doesNotThrow(() => validateMacosLifecycleExecution({
    requestedArchitecture: 'x86_64',
    executableAvailable: true,
  }))
  assert.throws(
    () => validateMacosLifecycleExecution({
      requestedArchitecture: 'x86_64',
    }),
    /requires an Intel Mac or Rosetta 2/,
  )
  assert.throws(
    () => validateMacosLifecycleExecution({
      requestedArchitecture: 'arm64',
    }),
    /requires Apple Silicon/,
  )
  assert.throws(
    () => validateMacosLifecycleExecution({
      requestedArchitecture: 'powerpc',
      executableAvailable: true,
    }),
    /unsupported requested architecture/,
  )
})

test('parses and normalizes LaunchServices application identifiers', () => {
  assert.equal(
    normalizeLsappinfoAsn('ASN:0x0-0x13D63D5-"mutsumi":'),
    'ASN:0x0-0x13d63d5:',
  )
  assert.equal(normalizeLsappinfoAsn('not an ASN'), null)
  assert.deepEqual(
    parseLsappinfoAsns([
      'ASN:0x0-0x13d63d5-"mutsumi":',
      'ASN:0x0-0x13d63d5:',
      'ASN:0x0-0x99-"other":',
    ].join('\n')),
    ['ASN:0x0-0x13d63d5:', 'ASN:0x0-0x99:'],
  )
})

test('parses the LaunchServices long-info values used by the smoke test', () => {
  const facts = parseLsappinfoInfo([
    '"ApplicationType"="Foreground"',
    '"CanBecomeFrontmost"=true',
    '"Hidden"=false',
    '"LSArchitecture"="x86_64"',
    '"pid"=2564',
  ].join('\n'))
  assert.deepEqual(facts, {
    ApplicationType: 'Foreground',
    CanBecomeFrontmost: true,
    Hidden: false,
    LSArchitecture: 'x86_64',
    pid: 2564,
  })
})

test('records app ownership before readiness completes', () => {
  const asn = 'ASN:0x0-0x123:'
  assert.deepEqual(discoverLifecycleOwnership({
    currentAsns: [asn],
    currentPids: [4321],
    infoByAsn: {
      [asn]: {
        LSBundlePath: APP_PATH,
        LSApplicationHasSignalledItIsReady: false,
        pid: 4321,
      },
    },
    expectedAppPath: APP_PATH,
  }), {
    ownedAsn: asn,
    ownedPid: 4321,
  })

  assert.deepEqual(discoverLifecycleOwnership({
    currentPids: [4321],
    expectedAppPath: APP_PATH,
  }), {
    ownedAsn: null,
    ownedPid: 4321,
  })
})

test('refuses ambiguous ownership instead of selecting a process to kill', () => {
  assert.throws(() => discoverLifecycleOwnership({
    currentPids: [4321, 4322],
    expectedAppPath: APP_PATH,
  }), /ownership is ambiguous/)
})

test('cleanup never removes the socket while a PID or replacement ASN remains', () => {
  assert.deepEqual(lifecycleCleanupState({
    currentAsns: [],
    currentPids: [],
    socketPresent: true,
  }), {
    complete: false,
    removeSocket: false,
  })
  assert.deepEqual(lifecycleCleanupState({
    ownedPid: 4321,
    currentPids: [4321],
    socketPresent: true,
  }), {
    complete: false,
    removeSocket: false,
  })
  assert.deepEqual(lifecycleCleanupState({
    ownedPid: 4321,
    currentAsns: ['ASN:0x0-0x999:'],
    socketPresent: true,
  }), {
    complete: false,
    removeSocket: false,
  })
  assert.deepEqual(lifecycleCleanupState({
    ownedPid: 4321,
    socketPresent: true,
  }), {
    complete: true,
    removeSocket: true,
  })
})

test('accepts a complete foreground launch, singleton relaunch and normal Quit', () => {
  const result = validateMacosLifecycleSnapshot(snapshot(), OPTIONS)
  assert.equal(result.messages.length, 3)
})

test('records Finder as the exact initial launch source', () => {
  const result = validateMacosLifecycleSnapshot(
    snapshot({ initialLaunch: 'finder' }),
    { ...OPTIONS, expectedInitialLaunch: 'finder' },
  )
  assert.match(result.messages[0], /via finder/)
  assert.throws(
    () => validateMacosLifecycleSnapshot(snapshot(), {
      ...OPTIONS,
      expectedInitialLaunch: 'finder',
    }),
    /expected "finder"/,
  )
})

test('rejects Agent/background bundles that cannot guarantee a Dock icon', () => {
  assert.throws(
    () => validateMacosLifecycleSnapshot(snapshot({ uiElement: true }), OPTIONS),
    /LSUIElement must be absent or false/,
  )
  assert.throws(
    () => validateMacosLifecycleSnapshot(snapshot({ backgroundOnly: true }), OPTIONS),
    /LSBackgroundOnly must be absent or false/,
  )
  assert.throws(
    () => validateMacosLifecycleSnapshot(snapshot({ applicationType: 'UIElement' }), OPTIONS),
    /application type must be Foreground/,
  )
})

test('rejects incomplete readiness, hidden launch and an unexpected slice', () => {
  assert.throws(
    () => validateMacosLifecycleSnapshot(snapshot({ signalledReady: false }), OPTIONS),
    /did not signal launch readiness/,
  )
  assert.throws(
    () => validateMacosLifecycleSnapshot(snapshot({ hidden: true }), OPTIONS),
    /unexpectedly launched hidden/,
  )
  assert.throws(
    () => validateMacosLifecycleSnapshot(snapshot({ architecture: 'x86_64' }), OPTIONS),
    /expected "arm64"/,
  )
})

test('rejects duplicate relaunches or a relaunch that fails to activate the singleton', () => {
  assert.throws(
    () => validateMacosLifecycleSnapshot(snapshot({ relaunchProcessCount: 2 }), OPTIONS),
    /left 1 LaunchServices registrations and 2 app processes/,
  )
  assert.throws(
    () => validateMacosLifecycleSnapshot(snapshot({ relaunchSamePid: false }), OPTIONS),
    /replaced the original application process/,
  )
  assert.throws(
    () => validateMacosLifecycleSnapshot(snapshot({ relaunchFrontmost: false }), OPTIONS),
    /did not activate the existing application/,
  )
})

test('rejects incomplete normal-Quit cleanup', () => {
  assert.throws(
    () => validateMacosLifecycleSnapshot(snapshot({ gracefulQuitRequested: false }), OPTIONS),
    /Quit Apple Event failed/,
  )
  assert.throws(
    () => validateMacosLifecycleSnapshot(snapshot({ socketRemovedAfterQuit: false }), OPTIONS),
    /socket remained after normal Quit/,
  )
})

test('reports invalid snapshots as contract errors', () => {
  assert.throws(
    () => validateMacosLifecycleSnapshot(null, OPTIONS),
    (error) => error instanceof MacosLifecycleContractError,
  )
})
