#!/usr/bin/env node
/**
 * Inspect a built universal macOS bundle with public Apple command-line tools.
 *
 * unsigned mode is used by desktop CI today. signed mode is intentionally
 * dormant until the bundle identifier and Developer ID credentials are ready.
 */

import { constants, accessSync, lstatSync, readdirSync } from 'node:fs'
import { basename, join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'

import {
  classifyCodesignEntitlements,
  developerIdRequirementForTeam,
  MacosBundleContractError,
  parseMacosBundleArgs,
  validateMacosBundleContract,
} from './macos-bundle-contract.mjs'

function fail(message) {
  console.error(`::error::${message}`)
  process.exit(1)
}

let parsedArgs
try {
  parsedArgs = parseMacosBundleArgs(process.argv.slice(2))
} catch (error) {
  if (error instanceof MacosBundleContractError) fail(error.message)
  throw error
}
if (process.platform !== 'darwin') fail('macOS bundle verification must run on macOS')

const {
  bundleDir: bundleDirArg,
  mode,
  expectedIdentifier,
  expectedMinimumSystemVersion,
} = parsedArgs
const bundleDir = resolve(bundleDirArg)
const macosDir = join(bundleDir, 'macos')
const dmgDir = join(bundleDir, 'dmg')
const diagnostics = []

function run(command, args) {
  const result = spawnSync(command, args, { encoding: 'utf8' })
  const stdout = result.stdout ?? ''
  const stderr = result.stderr ?? ''
  const output = [stdout, stderr, result.error?.message]
    .filter(Boolean)
    .join('\n')
    .trim()
  const record = {
    command: [command, ...args].join(' '),
    ok: result.status === 0,
    stdout,
    stderr,
    output,
  }
  diagnostics.push(record)
  return record
}

function required(command, args, description) {
  const result = run(command, args)
  if (!result.ok) {
    fail(`${description} failed${result.output ? `: ${result.output}` : ''}`)
  }
  return result.output
}

function entriesAt(path, predicate) {
  try {
    return readdirSync(path, { withFileTypes: true })
      .filter(predicate)
      .map((entry) => entry.name)
      .sort()
  } catch (error) {
    fail(`cannot read ${path}: ${error.message}`)
  }
}

const appNames = entriesAt(macosDir, (entry) => entry.isDirectory() && entry.name.endsWith('.app'))
if (appNames.length !== 1) fail(`expected exactly one .app in ${macosDir}, found [${appNames.join(', ')}]`)
const appPath = join(macosDir, appNames[0])
const infoPlist = join(appPath, 'Contents', 'Info.plist')

const executableName = required(
  'plutil',
  ['-extract', 'CFBundleExecutable', 'raw', infoPlist],
  'reading CFBundleExecutable',
)
if (executableName !== basename(executableName)
  || executableName === '.'
  || executableName === '..'
  || executableName.includes('\\')) {
  fail(`CFBundleExecutable must be one file name, found "${executableName}"`)
}
const executablePath = join(appPath, 'Contents', 'MacOS', executableName)
let executablePresent = false
try {
  accessSync(executablePath, constants.X_OK)
  executablePresent = lstatSync(executablePath).isFile()
} catch {
  executablePresent = false
}

const architectures = executablePresent
  ? required('lipo', ['-archs', executablePath], 'reading Mach-O architectures').split(/\s+/).filter(Boolean)
  : []
const minimumSystemVersion = required(
  'plutil',
  ['-extract', 'LSMinimumSystemVersion', 'raw', infoPlist],
  'reading LSMinimumSystemVersion',
)
const identifier = required(
  'plutil',
  ['-extract', 'CFBundleIdentifier', 'raw', infoPlist],
  'reading CFBundleIdentifier',
)
const dmgNames = entriesAt(dmgDir, (entry) => entry.isFile() && entry.name.toLowerCase().endsWith('.dmg'))
const dmgPath = dmgNames.length === 1 ? join(dmgDir, dmgNames[0]) : null
const dmgIntegrity = dmgPath ? run('hdiutil', ['verify', dmgPath]) : { ok: false, output: '' }

const snapshot = {
  appName: basename(appPath),
  executableName,
  executablePresent,
  architectures,
  minimumSystemVersion,
  identifier,
  dmgNames,
  dmgIntegrityValid: dmgIntegrity.ok,
}

if (mode === 'signed') {
  const signature = run('codesign', ['--verify', '--deep', '--strict', '--verbose=3', appPath])
  const signatureDetails = run('codesign', ['-dvv', appPath])
  const teamIdentifier = signatureDetails.output.match(/^TeamIdentifier=([A-Z0-9]{10})$/m)?.[1]
  const developerIdRequirement = teamIdentifier
    ? run('codesign', [
        '--verify',
        '--strict',
        '--verbose=3',
        `-R=${developerIdRequirementForTeam(teamIdentifier)}`,
        appPath,
      ])
    : { ok: false, output: 'TeamIdentifier is missing or malformed' }
  const entitlements = run('codesign', ['--display', '--entitlements', '-', appPath])
  const appGatekeeper = run('spctl', ['-vvv', '--assess', '--type', 'exec', appPath])
  const dmgGatekeeper = dmgPath
    ? run('spctl', ['-a', '-t', 'open', '--context', 'context:primary-signature', '-v', dmgPath])
    : { ok: false, output: '' }
  const dmgStaple = dmgPath ? run('xcrun', ['stapler', 'validate', dmgPath]) : { ok: false, output: '' }
  const entitlementFacts = classifyCodesignEntitlements(entitlements)

  Object.assign(snapshot, {
    signatureValid: signature.ok,
    signatureDetails: signatureDetails.output,
    developerIdRequirementValid: developerIdRequirement.ok,
    ...entitlementFacts,
    appGatekeeperAccepted: appGatekeeper.ok,
    dmgGatekeeperAccepted: dmgGatekeeper.ok,
    dmgStapleValid: dmgStaple.ok,
  })
}

try {
  const result = validateMacosBundleContract(snapshot, {
    mode,
    expectedIdentifier,
    expectedMinimumSystemVersion,
  })
  for (const message of result.messages) console.log(`✓ ${message}`)
  for (const warning of result.warnings) console.log(`::warning::${warning}`)
  console.log(`✓ app bundle: ${appPath}`)
  console.log(`✓ identifier: ${identifier}`)
} catch (error) {
  if (error instanceof MacosBundleContractError) {
    for (const diagnostic of diagnostics.filter((entry) => !entry.ok && entry.output)) {
      console.error(`~ ${diagnostic.command}: ${diagnostic.output}`)
    }
    fail(error.message)
  }
  throw error
}
