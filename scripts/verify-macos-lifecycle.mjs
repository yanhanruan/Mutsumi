#!/usr/bin/env node
/**
 * Opt-in macOS LaunchServices smoke for a built Mutsumi.app.
 *
 * The script refuses to touch an existing instance. It launches the exact app
 * path, verifies the Dock/foreground contract and singleton activation, sends
 * a standard Quit Apple Event, and confirms the process plus socket are gone.
 */

import { existsSync, lstatSync, realpathSync, unlinkSync } from 'node:fs'
import { basename, join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'

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

function fail(message) {
  throw new MacosLifecycleContractError(message)
}

function run(command, args) {
  const result = spawnSync(command, args, { encoding: 'utf8' })
  return {
    ok: result.status === 0,
    stdout: result.stdout ?? '',
    stderr: result.stderr ?? '',
    output: [result.stdout, result.stderr, result.error?.message]
      .filter(Boolean)
      .join('\n')
      .trim(),
  }
}

function required(command, args, description) {
  const result = run(command, args)
  if (!result.ok) fail(`${description} failed${result.output ? `: ${result.output}` : ''}`)
  return result.output
}

function sleep(milliseconds) {
  return new Promise((resolvePromise) => setTimeout(resolvePromise, milliseconds))
}

async function waitFor(description, timeoutMs, predicate) {
  const deadline = Date.now() + timeoutMs
  do {
    const value = predicate()
    if (value) return value
    await sleep(200)
  } while (Date.now() < deadline)
  fail(`${description} timed out after ${timeoutMs} ms`)
}

function readInfoPlist(appPath) {
  const infoPlist = join(appPath, 'Contents', 'Info.plist')
  const json = required(
    'plutil',
    ['-convert', 'json', '-o', '-', infoPlist],
    'reading the app Info.plist',
  )
  try {
    return JSON.parse(json)
  } catch (error) {
    fail(`Info.plist JSON could not be parsed: ${error.message}`)
  }
}

function findAsns(identifier) {
  const result = run('lsappinfo', [
    'find',
    `kLSBundleIdentifierLowerCaseKey=${identifier.toLowerCase()}`,
  ])
  if (!result.ok) fail(`querying LaunchServices failed: ${result.output}`)
  return parseLsappinfoAsns(result.stdout)
}

function infoForAsn(asn) {
  return parseLsappinfoInfo(required(
    'lsappinfo',
    ['info', '-long', asn],
    `reading LaunchServices info for ${asn}`,
  ))
}

function tryInfoForAsn(asn) {
  const result = run('lsappinfo', ['info', '-long', asn])
  return result.ok ? parseLsappinfoInfo(result.stdout) : null
}

function currentFrontAsn() {
  return normalizeLsappinfoAsn(required(
    'lsappinfo',
    ['front'],
    'reading the frontmost application',
  ))
}

function socketPathFor(identifier) {
  return `/tmp/${identifier.replace(/[.-]/g, '_')}_si.sock`
}

function isSocket(path) {
  try {
    return lstatSync(path).isSocket()
  } catch {
    return false
  }
}

function processPidsForExecutable(executablePath) {
  const output = required(
    'ps',
    ['-ax', '-o', 'pid=,comm='],
    'reading the process table',
  )
  const pids = []
  for (const line of output.split(/\r?\n/)) {
    const match = line.match(/^\s*(\d+)\s+(.+)$/)
    if (match?.[2] === executablePath) pids.push(Number(match[1]))
  }
  return pids
}

function launch(appPath, architecture) {
  required(
    'open',
    [
      '--arch', architecture,
      '-na', appPath,
      '--stdout', '/dev/null',
      '--stderr', '/dev/null',
    ],
    `launching ${basename(appPath)} as ${architecture}`,
  )
}

function initialLaunch(appPath, architecture, source) {
  const command = macosInitialLaunchCommand({
    appPath,
    architecture,
    initialLaunch: source,
  })
  required(
    command.command,
    command.args,
    `launching ${basename(appPath)} through ${source} as ${architecture}`,
  )
}

function runtimeSnapshot(asn, plist) {
  const facts = infoForAsn(asn)
  return {
    bundlePath: facts.LSBundlePath,
    bundleIdentifier: facts.CFBundleIdentifier,
    packageType: facts.CFBundlePackageType,
    uiElement: plist.LSUIElement,
    backgroundOnly: plist.LSBackgroundOnly,
    applicationType: facts.ApplicationType,
    canBecomeFrontmost: facts.CanBecomeFrontmost,
    registered: facts.LSApplicationHasRegistered,
    signalledReady: facts.LSApplicationHasSignalledItIsReady,
    readyToBeFrontable: facts.LSApplicationReadyToBeFrontableKey,
    hidden: facts.Hidden,
    architecture: facts.LSArchitecture,
    pid: facts.pid,
  }
}

let parsedArgs
try {
  parsedArgs = parseMacosLifecycleArgs(process.argv.slice(2))
} catch (error) {
  console.error(`::error::${error.message}`)
  process.exit(1)
}

if (process.platform !== 'darwin') {
  console.error('::error::macOS lifecycle smoke must run on macOS')
  process.exit(1)
}

const requestedPath = resolve(parsedArgs.appPath)
if (!existsSync(requestedPath) || !lstatSync(requestedPath).isDirectory() || !requestedPath.endsWith('.app')) {
  console.error(`::error::--app must point to an existing .app bundle: ${requestedPath}`)
  process.exit(1)
}
const appPath = realpathSync(requestedPath)
let plist
try {
  plist = readInfoPlist(appPath)
} catch (error) {
  console.error(`::error::${error.message}`)
  process.exit(1)
}
const identifier = plist.CFBundleIdentifier
if (typeof identifier !== 'string' || !/^[A-Za-z0-9.-]+$/.test(identifier)) {
  console.error(`::error::bundle identifier is missing or unsafe: "${identifier}"`)
  process.exit(1)
}

const executableName = plist.CFBundleExecutable
if (typeof executableName !== 'string'
  || executableName !== basename(executableName)
  || executableName === '.'
  || executableName === '..'
  || executableName.includes('\\')) {
  console.error(`::error::CFBundleExecutable must be one file name, found "${executableName}"`)
  process.exit(1)
}
const executablePath = join(appPath, 'Contents', 'MacOS', executableName)
if (!existsSync(executablePath) || !lstatSync(executablePath).isFile()) {
  console.error(`::error::bundle executable is missing: ${executablePath}`)
  process.exit(1)
}
const socketPath = socketPathFor(identifier)
let ownedAsn = null
let ownedPid = null
let launchAttempted = false
let gracefulExitComplete = false

try {
  const executableAvailable = run(
    'arch',
    [`-${parsedArgs.architecture}`, '/usr/bin/true'],
  ).ok
  validateMacosLifecycleExecution({
    requestedArchitecture: parsedArgs.architecture,
    executableAvailable,
  })
} catch (error) {
  console.error(`::error::${error.message}`)
  process.exit(1)
}

function currentOwnershipFacts() {
  const currentAsns = findAsns(identifier)
  const currentPids = processPidsForExecutable(executablePath)
  const infoByAsn = {}
  for (const asn of currentAsns) {
    const facts = tryInfoForAsn(asn)
    if (facts) infoByAsn[asn] = facts
  }
  return { currentAsns, currentPids, infoByAsn }
}

function rememberOwnership() {
  const facts = currentOwnershipFacts()
  const discovered = discoverLifecycleOwnership({
    ...facts,
    expectedAppPath: appPath,
  })
  ownedAsn ??= discovered.ownedAsn
  ownedPid ??= discovered.ownedPid
  return facts
}

async function waitForOwnedExit(timeoutMs) {
  try {
    await waitFor('owned application cleanup', timeoutMs, () => {
      const currentAsns = findAsns(identifier)
      const currentPids = processPidsForExecutable(executablePath)
      return lifecycleCleanupState({
        ownedPid,
        currentAsns,
        currentPids,
        socketPresent: isSocket(socketPath),
      }).complete
    })
    return true
  } catch {
    return false
  }
}

function signalOwnedPid(signal) {
  if (!Number.isInteger(ownedPid)) return false
  if (!processPidsForExecutable(executablePath).includes(ownedPid)) return true
  try {
    process.kill(ownedPid, signal)
    return true
  } catch (error) {
    if (error?.code === 'ESRCH') return true
    console.error(`::warning::failed to send ${signal} to test-owned PID ${ownedPid}: ${error.message}`)
    return false
  }
}

async function cleanupOwnedInstance() {
  if (!Number.isInteger(ownedPid)) {
    try {
      rememberOwnership()
    } catch (error) {
      console.error(`::warning::could not resolve launched ownership during cleanup: ${error.message}`)
    }
  }
  if (!Number.isInteger(ownedPid)) {
    const remainingAsns = findAsns(identifier)
    const remainingPids = processPidsForExecutable(executablePath)
    if (remainingAsns.length === 0 && remainingPids.length === 0) {
      if (isSocket(socketPath)) {
        console.error(`::warning::preserving unknown stale socket for manual inspection: ${socketPath}`)
      }
      return true
    }
    console.error('::warning::could not prove ownership of a launched PID; no destructive cleanup attempted')
    return false
  }

  const currentAsns = findAsns(identifier)
  if (ownedAsn && currentAsns.includes(ownedAsn)) {
    const facts = tryInfoForAsn(ownedAsn)
    if (facts?.LSBundlePath === appPath) {
      const termination = run('lsappinfo', ['kill', ownedAsn])
      if (!termination.ok) {
        console.error(`::warning::lsappinfo could not terminate ${ownedAsn}: ${termination.output}`)
      }
    }
  }

  let exited = await waitForOwnedExit(1500)
  if (!exited && signalOwnedPid('SIGTERM')) exited = await waitForOwnedExit(2000)
  if (!exited && signalOwnedPid('SIGKILL')) exited = await waitForOwnedExit(2000)

  const finalAsns = findAsns(identifier)
  const finalPids = processPidsForExecutable(executablePath)
  const cleanup = lifecycleCleanupState({
    ownedPid,
    currentAsns: finalAsns,
    currentPids: finalPids,
    socketPresent: isSocket(socketPath),
  })
  if (!cleanup.complete) {
    console.error(
      `::warning::cleanup incomplete; preserving socket with ASNs [${finalAsns.join(', ')}] `
      + `and PIDs [${finalPids.join(', ')}]`,
    )
    return false
  }

  if (cleanup.removeSocket) {
    try {
      unlinkSync(socketPath)
    } catch (error) {
      console.error(`::warning::failed to remove test-owned socket ${socketPath}: ${error.message}`)
      return false
    }
  }
  return !isSocket(socketPath)
}

try {
  const existing = findAsns(identifier)
  const existingPids = processPidsForExecutable(executablePath)
  if (existing.length > 0 || existingPids.length > 0) {
    fail(
      `refusing to touch ${existing.length} existing ${identifier} registration(s) `
      + `and ${existingPids.length} exact executable process(es); quit Mutsumi first`,
    )
  }

  launchAttempted = true
  initialLaunch(appPath, parsedArgs.architecture, parsedArgs.initialLaunch)
  const first = await waitFor('application LaunchServices readiness', parsedArgs.timeoutMs, () => {
    const { currentAsns } = rememberOwnership()
    if (currentAsns.length !== 1 || !ownedAsn) return null
    const runtime = runtimeSnapshot(ownedAsn, plist)
    if (runtime.bundlePath !== appPath
      || runtime.registered !== true
      || runtime.signalledReady !== true
      || runtime.readyToBeFrontable !== true) return null
    ownedPid ??= runtime.pid
    return { asn: ownedAsn, runtime }
  })

  const socketCreated = await waitFor(
    'single-instance socket creation',
    parsedArgs.timeoutMs,
    () => isSocket(socketPath),
  )
  const initialFrontmost = currentFrontAsn() === ownedAsn

  required(
    'open',
    ['-a', 'Finder', '--stdout', '/dev/null', '--stderr', '/dev/null'],
    'moving focus away from Mutsumi before the singleton relaunch',
  )
  await waitFor(
    'Finder foreground activation',
    parsedArgs.timeoutMs,
    () => currentFrontAsn() !== ownedAsn,
  )

  launch(appPath, parsedArgs.architecture)
  const relaunched = await waitFor('single-instance activation', parsedArgs.timeoutMs, () => {
    const asns = findAsns(identifier)
    const pids = processPidsForExecutable(executablePath)
    const front = currentFrontAsn()
    if (asns.length !== 1 || pids.length !== 1 || front !== ownedAsn) return null
    return { asns, pids, front }
  })

  const quit = run('osascript', [
    '-e',
    `tell application id "${identifier}" to quit`,
  ])
  const exitedAfterQuit = await waitFor('normal application Quit', parsedArgs.timeoutMs, () => (
    findAsns(identifier).length === 0
      && processPidsForExecutable(executablePath).length === 0
  ))
  const socketRemovedAfterQuit = await waitFor(
    'single-instance socket cleanup',
    parsedArgs.timeoutMs,
    () => !isSocket(socketPath),
  )
  gracefulExitComplete = true

  const result = validateMacosLifecycleSnapshot({
    ...first.runtime,
    initialLaunch: parsedArgs.initialLaunch,
    initialFrontmost,
    socketCreated,
    relaunchInstanceCount: relaunched.asns.length,
    relaunchProcessCount: relaunched.pids.length,
    relaunchSameAsn: relaunched.asns[0] === ownedAsn,
    relaunchSamePid: relaunched.pids[0] === first.runtime.pid,
    relaunchFrontmost: relaunched.front === ownedAsn,
    gracefulQuitRequested: quit.ok,
    exitedAfterQuit,
    socketRemovedAfterQuit,
  }, {
    expectedAppPath: appPath,
    expectedIdentifier: identifier,
    expectedArchitecture: parsedArgs.architecture,
    expectedInitialLaunch: parsedArgs.initialLaunch,
  })

  for (const message of result.messages) console.log(`✓ ${message}`)
  console.log(`✓ app bundle: ${appPath}`)
  console.log(`✓ single-instance socket: ${socketPath}`)
} catch (error) {
  if (error instanceof MacosLifecycleContractError) {
    console.error(`::error::${error.message}`)
    process.exitCode = 1
  } else {
    throw error
  }
} finally {
  if (launchAttempted && !gracefulExitComplete) {
    try {
      const cleaned = await cleanupOwnedInstance()
      if (!cleaned) process.exitCode = 1
    } catch (error) {
      console.error(`::warning::fallback cleanup failed: ${error.message}`)
      process.exitCode = 1
    }
  }
}
