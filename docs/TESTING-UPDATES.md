# Testing the auto-updater

Mock tests prove the UI logic. They do **not** prove that an installed old
Windows client can *detect → download → verify the signature → install →
relaunch → report the new version*. That claim is only earned by walking the
whole chain below, in order — each layer catches a class of failure the
previous one structurally cannot.

| Layer | What it proves | What it cannot prove |
| --- | --- | --- |
| 1. State-machine unit tests | The UI can never mis-order (e.g. an interrupted download shown as success) | Anything about the real updater protocol |
| 2. Local fake update server | A real staging build speaks the updater protocol end to end, incl. rejecting tampered signatures | Anything about GitHub / CI |
| 3. GitHub staging release + asset verification | CI produces a complete, signed, contract-valid release | That an installed client actually upgrades |
| 4. Real upgrade from an old client | The whole thing, for real | — (this is the finish line) |

---

## Layer 1 — state-machine unit tests (`npm test`)

The update pop-up's lifecycle is a pure reducer:
[`src/config/updateFlow.ts`](../src/config/updateFlow.ts), tested in
[`updateFlow.test.ts`](../src/config/updateFlow.test.ts). **Illegal transitions
are no-ops**, which converts ordering bugs from "untested" to "impossible":
a `DOWNLOAD_DONE` or `INSTALL_DONE` arriving after a failure is simply ignored.

Covered rows (see the test file for the full table): no update → up-to-date;
update → offer with version + notes; check timeout / malformed manifest →
failed with reason, no crash; download failure → failed + Retry; interrupted
download + stray success callbacks → still failed; signature rejection →
failed, never installed. The scheduling policy (once daily, snooze 1–30 days)
is covered separately in
[`updatePolicy.test.ts`](../src/config/updatePolicy.test.ts) and
[`useUpdateCheck.test.ts`](../src/composables/useUpdateCheck.test.ts).

### Manual UI states (dev harness)

In dev builds only (`npm run tauri dev` — the code is compiled out of
production bundles), you can force any pop-up state without touching source:

```js
// in ANY window's devtools console (all windows share one origin):
localStorage.setItem('mutsumi-update-mock', 'available')
// values: 'available' | 'available:install-fail' | 'uptodate' | 'error'
localStorage.removeItem('mutsumi-update-mock')   // back to the real updater
```

Then open the pop-up via **About → Check for updates**. `available` also mocks
the download (ending in 🎉); `available:install-fail` interrupts it mid-way and
must land on the failed view with a Retry button — never on 🎉.

---

## Layer 2 — local fake update server

This layer runs a **real staging build** of the app against a local HTTP
server that speaks the updater protocol — including the negative cases that
matter most.

The server supports Windows and both macOS architectures. `--platform` defaults
to `windows-x86_64`; use `darwin-aarch64` or `darwin-x86_64` for the actual Mac
client being tested. `missing-platform` removes only that target's entries, and
`bad-signature` corrupts only their cryptographic signature line. "Entries"
includes installer-specific keys such as `darwin-aarch64-app`, which the client
looks up before the bare `darwin-aarch64` key. The legacy `missing-windows`
scenario remains available for Windows only; with a macOS `--platform` the
server refuses to start, because it would leave the Mac client a valid update
and silently act like `ok`. The server listens on loopback (`localhost`), not
the LAN; on macOS that resolved to IPv6 `[::1]` only, which the client reached.

### macOS local protocol runbook

Use a disposable test checkout and a writable test app location. Quit any other
Mutsumi instance first: test builds retain the macOS bundle identifier and can
share app data/Keychain with your regular build. Back up the app data before
testing; do not overwrite your daily installation. These local artifacts are
not Developer ID/notarized distribution evidence.

1. Run `node scripts/gen-test-keys.mjs` and put its printed test public key into
   the local `src-tauri/tauri.staging.conf.json`. The checked-in sample key does
   **not** automatically match a newly generated private key. Keep this local
   configuration change and the test keys out of commits.
2. In a shell without Apple release credentials, build an old and new test
   version with the same test key. The macOS overlay is loaded automatically;
   the extra version configuration avoids editing committed version files:

   ```bash
   export TAURI_SIGNING_PRIVATE_KEY="$PWD/scripts/test-keys/mutsumi-test.key"
   export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=''
   npx tauri build --target universal-apple-darwin --bundles app --ci \
     --config src-tauri/tauri.staging.conf.json --config '{"version":"9.9.8"}'
   ```

   Copy the generated `.app` from
   `src-tauri/target/universal-apple-darwin/release/bundle/macos/` to your writable
   test location **before** the next build. Launch that copy and confirm About
   says `9.9.8`, then quit it. Build again with `{"version":"9.9.9"}`; keep the
   new `.app.tar.gz` and adjacent `.sig`. Do not substitute the DMG, or launch
   the new client before testing the old one.
3. Create a universal manifest (replace the archive path with the actual output):

   ```bash
   node scripts/make-latest-json.mjs \
     --installer src-tauri/target/universal-apple-darwin/release/bundle/macos/mutsumi.app.tar.gz \
     --version 9.9.9 --platform darwin-universal
   node scripts/fake-update-server.mjs --platform darwin-aarch64 --scenario lower-version
   ```

   Both Darwin entries point to the same archive/signature. On native Intel use
   `--platform darwin-x86_64`; use the architecture of the running client, not
   just the physical host. Repeat the server command for `missing-platform`,
   `malformed-json`, `installer-404`, `bad-signature`, `interrupt`, then `ok` last.
4. For each scenario use the **old copied app**, About → Check for updates.
   The existing scenario table defines the expected UI. In all download-failure
   cases confirm About still reports `9.9.8`; for `ok`, confirm the relaunched
   app reports `9.9.9`. Record architecture, OS, both versions, scenario, actual
   result and evidence. A Gatekeeper/launch failure is a blocked local test,
   not a protocol pass; do not disable Gatekeeper to claim success.

`npm run test:local-updater` exercises real loopback HTTP responses and fixture
generation for Windows and both Darwin targets. Its synthetic signatures and
archive bytes test the **harness**, not Tauri signature verification or macOS
installation. Those require the manual procedure above and signed staging for
the final release gate. The manual procedure has passed once (below); signed
staging and the Layer 4 real upgrade are still pending.

#### macOS local results — 2026-09-05

Host: Apple M5 (`Mac17,4`), macOS 26.5.1 (`25F80`). Builds from `ebf12c0`:
`universal-apple-darwin`, `--bundles app`, staging overlay with a locally
generated test key, ad-hoc signed — **not** Developer ID/notarized evidence.
Old 9.9.8 → new 9.9.9 (`mutsumi.app.tar.gz`). One copied old app was launched
natively (`open --arch arm64`) and under Rosetta (`open --arch x86_64`, confirmed
by the Rosetta runtime mapped into the process). Evidence for every row: the
server request log, the SHA-256 of `Contents/MacOS/Mutsumi` compared with both
builds, the process ID, and the pop-up as seen by the tester.

| Client | `--platform` | Scenario | Pop-up | Requests | Installed afterwards |
| --- | --- | --- | --- | --- | --- |
| arm64 | `darwin-aarch64` | `missing-platform` | failed: none of `darwin-aarch64-app`, `darwin-aarch64` found | manifest only | 9.9.8, unchanged |
| arm64 | `darwin-aarch64` | `bad-signature` | offered 9.9.9 → failed after download | manifest + archive | 9.9.8, unchanged |
| arm64 | `darwin-aarch64` | `interrupt` | failed mid-download | manifest + archive | 9.9.8, unchanged |
| x86_64 | `darwin-x86_64` | `missing-platform` | failed: none of `darwin-x86_64-app`, `darwin-x86_64` found | manifest only | 9.9.8, unchanged |
| x86_64 | `darwin-x86_64` | `bad-signature` | failed after download | manifest + archive | 9.9.8, unchanged |
| x86_64 | `darwin-x86_64` | `interrupt` | failed: `error decoding response body` | manifest + archive | 9.9.8, unchanged |
| x86_64 | `darwin-aarch64` | `missing-platform` (cross-check) | offered 9.9.9; closed without installing | manifest only | 9.9.8, unchanged |
| arm64 | `darwin-x86_64` | `missing-platform` (cross-check) | offered 9.9.9; closed without installing | manifest only | 9.9.8, unchanged |
| arm64 | `darwin-aarch64` | `lower-version` | up to date | manifest only | 9.9.8, unchanged |
| arm64 | `darwin-aarch64` | `malformed-json` | failed, no crash | manifest only | 9.9.8, unchanged |
| arm64 | `darwin-aarch64` | `installer-404` | failed; Retry failed again | manifest + archive 404 ×2 | 9.9.8, unchanged |
| arm64 | `darwin-aarch64` | `ok` (last) | downloaded, installed, relaunched; About shows 9.9.9 | manifest + archive | 9.9.9 = new build, new process |

After `ok`, nothing was left next to the app or in the app's temp directory, and
the updated bundle carried no quarantine attribute. Two observations from the
run:

- About 5 s after the `lower-version` "up to date" view, with no user input,
  9.9.8 crashed once (SIGSEGV on the main thread in WebKit,
  `ScrollingTree::takePendingScrollUpdates` from the display-link refresh). The
  installed bundle was unaffected; the app was relaunched and the run
  continued. The cause remains undetermined: an unchanged installed bundle
  does not rule out the update window as a trigger. Further investigation is
  required; this observation does not establish that the crash is unrelated
  to the updater.
- The archive was an odd number of bytes, so `interrupt` dropped the
  connection correctly but never printed its `interrupting after` line. The
  server has since been fixed; `npm run test:local-updater` covers it.

### One-time setup

```powershell
# 1. Generate the TEST-ONLY signing keypair (gitignored; never commit keys)
node scripts/gen-test-keys.mjs
# 2. The script prints the test public key — it is already pasted into
#    src-tauri/tauri.staging.conf.json (re-paste if you regenerate with --force)
```

The staging overlay also sets `dangerousInsecureTransportProtocol: true`:
release builds otherwise **refuse the `http://localhost` endpoint at plugin
init and the installed app crashes instantly at startup** (a panic-abort,
Event Viewer code `0xc0000409` — dev builds don't enforce this, so you only
see it on an installed build). The flag must never be copied into the
production config or the github-staging overlay (both are `https`).

### Build two staging versions

> **The `.sig` comes from TWO things, not one.** A build only produces the
> `*-setup.exe.sig` when *both* are true: `bundle.createUpdaterArtifacts: true`
> in `tauri.conf.json` (turns the signing step **on**), and the two signing env
> vars below (supply the **key**). If `make-latest-json.mjs` reports a missing
> `.sig`, the build silently skipped signing — check the flag first, then the
> env vars. The flag is in the base config, so it applies to staging *and* the
> real GitHub release.

```powershell
$env:TAURI_SIGNING_PRIVATE_KEY = "$PWD\scripts\test-keys\mutsumi-test.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = ""

# old client (current committed version, e.g. 1.4.0) — install this one.
# NOTE: use npx, NOT "npm run tauri" — npm swallows "--config" (a reserved npm
# option) even after "--", and the mangled args end up crashing cargo.
npx tauri build --config src-tauri/tauri.staging.conf.json

# new version: bump, rebuild, keep the installer for the server
npm run release 1.5.0
npx tauri build --config src-tauri/tauri.staging.conf.json
node scripts/make-latest-json.mjs `
  --installer src-tauri/target/release/bundle/nsis/mutsumi_1.5.0_x64-setup.exe `
  --version 1.5.0
# Revert the local version afterwards (edits ONLY tauri.conf.json + Cargo.toml —
# no git tags, no GitHub; cannot collide with any published release):
#   git restore src-tauri/tauri.conf.json src-tauri/Cargo.toml
```

### Run the scenarios

Install the **old** staging build, then for each scenario: start the server,
open **About → Check for updates**, compare the pop-up against the
**Client MUST** column, close the pop-up, stop the server (Ctrl-C). The
*Server behavior* column is automatic — it is the fault the server injects,
not something you check; your only job is observing the client. The server
logs every request it serves, so you can also see the client actually
fetching `/latest.json` and the installer.

**Run `ok` LAST.** It really upgrades the installed app to the new version,
after which every other scenario just reports "up to date". Recommended
order: `lower-version`, `missing-windows`, `malformed-json` (no download),
then `installer-404`, `bad-signature`, `interrupt` (download attempted, must
fail, app stays on the old version — confirm in About), then `ok` as the
finale. That order needs zero reinstalls.

```powershell
node scripts/fake-update-server.mjs --scenario lower-version
```

| `--scenario` | Server behavior | Client MUST |
| --- | --- | --- |
| `ok` | valid manifest + installer + signature | offer 1.5.0 → download → install → relaunch as 1.5.0 |
| `lower-version` | manifest advertises 0.0.1 | show "latest version", no offer |
| `missing-platform` | removes the requested `--platform` entries, including installer-specific keys | failed view because no matching client platform exists; no download or install |
| `missing-windows` | no windows platform in manifest (Windows `--platform` only) | failed view — the plugin errors ("none of the fallback platforms… found"). Correct for a Windows-only app: such a manifest is broken, not "no update"; `verify-release-assets.mjs` blocks it from ever shipping |
| `installer-404` | download URL returns 404 | failed view with reason; Retry works |
| `bad-signature` | signature tampered | **reject the install** — failed view, never 🎉 |
| `malformed-json` | manifest is not JSON | failed view, no crash |
| `interrupt` | connection dropped mid-download | failed view — **never** "installed" |

`bad-signature` and `interrupt` are the two that justify this whole layer:
they prove the *plugin's* signature verification and your failure handling
against a real binary, which no mock can.

---

## Layer 3 — GitHub staging release + asset verification

Before the first staging run, configure the seven required updater/Apple
secrets described in [`RELEASING.md`](RELEASING.md), then run the manual
**release-credential-readiness** workflow. That check has only `contents: read`:
it proves the updater private key matches the committed public key, exercises a
disposable Developer ID signature, and authenticates through a read-only
`notarytool history` request. It does not create a release/tag/artifact or submit
anything for notarization. A green readiness run proves credentials, not the
signed Mutsumi bundle; continue with staging for the end-to-end gate.

Run the **staging-release** workflow (GitHub → Actions → staging-release →
*Run workflow*, choosing any branch). It:

1. runs a structural preflight for the updater and Apple release secrets before
   creating or mutating a GitHub Release; certificate/password validity,
   signing identity and notarization are still proven later on the macOS runner;
2. runs the unit tests, then builds and updater-signs Windows with the
   **production** key, creates the
   rolling **`staging` prerelease**, and passes its release ID to the macOS job.
   This prerelease never resolves through `releases/latest/download/…`, so it
   cannot leak to production clients;
3. builds one `universal-apple-darwin` app/DMG, signs it with Developer ID,
   notarizes/staples the app and uploads initial assets through Tauri. An
   explicit outer-DMG submission then waits for `Accepted`, staples and
   validates the DMG ticket, and runs the full signed bundle gate. Only after
   that gate passes does the workflow replace the initial DMG in the same
   ID/tag/channel-validated release. The action merges macOS updater entries
   into the existing `latest.json`; replacing the DMG does not alter the
   updater archive or its signature;
4. runs [`scripts/verify-release-assets.mjs`](../scripts/verify-release-assets.mjs)
   with `--require-macos-universal`, which fails unless the release carries the
   complete Windows NSIS and universal macOS DMG/updater asset set, with a
   version-matched manifest whose URLs and embedded signatures exactly match
   the uploaded assets. Before this gate, a normalizer binds every
   returned release ID/tag/channel, then binds every `tauri-action@v1` API asset
   ID to an asset on that same release and rewrites it to the stable public tag
   download URL; wrong releases, foreign repositories, unknown IDs and
   ambiguous IDs fail closed.

Step 4 is the guard against the **partial-success** hazard: a release that
exists (clients see an update) but whose installer/signature/manifest is
missing or mismatched (every update attempt fails). The same script gates
production drafts in `release.yml`. The staging prerelease is visible while its
jobs are running, so test clients must not check for updates until the final
verify job is green; this channel is isolated from production clients. Reset
errors are no longer treated as “not found”, and the final job also requires the
rolling `staging` tag to resolve to the workflow source SHA.

The macOS package itself has a separate bundle contract. Unsigned desktop CI
runs `scripts/verify-macos-bundle.mjs --mode unsigned` to check the universal
architectures, minimum OS, bundle metadata and DMG integrity. Signed staging and
production jobs invoke `--mode signed`, adding Developer ID, hardened-runtime,
timestamp, entitlement, Gatekeeper and stapled-notarization checks. The
macOS-only identifier is now
frozen as `io.github.yanhanruan.mutsumi`; signed mode must require that exact
value while the Windows base identifier and data namespace remain unchanged.
The wiring and pure contracts are automated, but the signed positive case stays
pending until the Apple secrets are installed and a credentialed staging run
passes; unsigned CI is not a substitute.

The production post-publish healer ignores this prerelease. The rolling channel
must remain attached to the literal `staging` tag rather than being rebound to
the build's `vX.Y.Z` version tag.

---

## Layer 4 — real upgrade from an old Windows client (the finish line)

This is the test that actually earns the claim "auto-update works". It stays
local and manual by design: the app is a tray-icon GUI on WebView2, and a
headless CI imitation of this flow would be flaky theater. CI instead owns
what it can prove deterministically (Layer 3).

Runbook:

1. **Build the "old" client** from the currently-released version (e.g.
   1.4.0) with the GitHub-staging overlay — production pubkey, endpoint pinned
   to the rolling staging URL:

   ```powershell
   # sign with your PRODUCTION key here (same env vars, your real key + password)
   npx tauri build --config src-tauri/tauri.github-staging.conf.json
   ```

   Install it and confirm **About** shows v1.4.0.

2. **Publish the "new" version to staging**: on a branch, `npm run release
   1.5.0`, commit, push, and dispatch the **staging-release** workflow on that
   branch. Wait for it to pass (including the asset verification step).

3. **Upgrade for real**: launch the old client → **About → Check for
   updates** → the pop-up offers v1.5.0 with the staging notes → *Update now*
   → download → install → the app relaunches.

4. **Assert the version actually changed**: About now shows **v1.5.0**, and
   "Last checked" shows a fresh timestamp with the ok badge. If About still
   shows 1.4.0, the update did not happen — no matter what the UI said.

Passing this once per release train (and re-running Layer 2's `bad-signature`
whenever the updater config changes) is the readiness bar.

---

## Release-readiness checklist

| # | Criterion | Satisfied by |
| --- | --- | --- |
| 1 | Mocks cover success, failure, and abnormal states | Layer 1 (`updateFlow.test.ts`, dev harness) |
| 2 | Full Windows update flow works against a local server | Layer 2 scenarios `ok` + failures |
| 3 | CI can create a staging release | `staging-release.yml` |
| 4 | Release asset completeness is verified | `verify-release-assets.mjs` (both workflows) |
| 5 | An old Windows client really upgrades | Layer 4 runbook |
| 6 | Invalid signatures are rejected | Layer 2 `bad-signature` |
| 7 | Download failures never display as success | Layer 1 reducer + Layer 2 `interrupt` |
| 8 | Incomplete or signature-mismatched `latest.json` can't ship a broken update | pure contract tests + `verify-release-assets.mjs` |
| 9 | A Windows smoke test has run | Layer 4, step 3–4 |
| 10 | The unsigned macOS universal app/DMG satisfies its bundle contract | `desktop-ci.yml` + `verify-macos-bundle.mjs --mode unsigned` |
| 11 | A signed macOS app/DMG passes Developer ID, Gatekeeper and stapling checks | Pending first credentialed staging run; workflow invokes `verify-macos-bundle.mjs --mode signed` |
| 12 | One manifest safely contains Windows and both universal macOS updater entries | Workflow contract tests + final `--require-macos-universal` release gate; credentialed run pending |

The production flow in [`RELEASING.md`](RELEASING.md) enforces the gate:
releases are created as **drafts**, verified automatically, smoke-tested via
staging, and only then published by a human.
