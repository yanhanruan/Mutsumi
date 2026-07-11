#!/usr/bin/env node
/**
 * bump-version.mjs — single source of truth for the app version.
 *
 * Writes the given semver into BOTH src-tauri/tauri.conf.json and
 * src-tauri/Cargo.toml so they can never drift. The About window reads the
 * version at runtime (getVersion()), so there is nothing else to hand-edit.
 *
 *   node scripts/bump-version.mjs 1.5.0
 *   npm run release 1.5.0
 *
 * After bumping: commit, then `git tag v1.5.0 && git push --follow-tags`.
 * The release workflow fails fast if the tag and this committed version differ.
 */
import { readFileSync, writeFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const here = dirname(fileURLToPath(import.meta.url))
const root = join(here, '..')

const version = process.argv[2]
if (!version) {
  console.error('usage: node scripts/bump-version.mjs <version>   e.g. 1.5.0')
  process.exit(1)
}
if (!/^\d+\.\d+\.\d+$/.test(version)) {
  console.error(`error: "${version}" is not valid semver (expected MAJOR.MINOR.PATCH, no leading "v")`)
  process.exit(1)
}

/** Replace the single occurrence matched by `re`, or fail loudly if absent. */
function patch(path, re, replacement, label) {
  const before = readFileSync(path, 'utf8')
  if (!re.test(before)) {
    console.error(`error: could not find the ${label} in ${path}`)
    process.exit(1)
  }
  writeFileSync(path, before.replace(re, replacement))
}

const confPath = join(root, 'src-tauri', 'tauri.conf.json')
patch(confPath, /("version":\s*")[^"]*(")/, `$1${version}$2`, '"version" field')

const cargoPath = join(root, 'src-tauri', 'Cargo.toml')
// Only the [package] version — the first `version = "..."` after that header,
// never a dependency's version.
patch(cargoPath, /(\[package\][\s\S]*?\nversion\s*=\s*")[^"]*(")/, `$1${version}$2`, '[package] version')

console.log(`bumped to ${version}`)
console.log('  • src-tauri/tauri.conf.json')
console.log('  • src-tauri/Cargo.toml')
console.log('')
console.log(`next:  git commit -am "chore(release): v${version}"  &&  git tag v${version}  &&  git push --follow-tags`)
