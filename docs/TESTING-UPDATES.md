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
| `missing-windows` | no windows platform in manifest | failed view — the plugin errors ("none of the fallback platforms… found"). Correct for a Windows-only app: such a manifest is broken, not "no update"; `verify-release-assets.mjs` blocks it from ever shipping |
| `installer-404` | download URL returns 404 | failed view with reason; Retry works |
| `bad-signature` | signature tampered | **reject the install** — failed view, never 🎉 |
| `malformed-json` | manifest is not JSON | failed view, no crash |
| `interrupt` | connection dropped mid-download | failed view — **never** "installed" |

`bad-signature` and `interrupt` are the two that justify this whole layer:
they prove the *plugin's* signature verification and your failure handling
against a real binary, which no mock can.

---

## Layer 3 — GitHub staging release + asset verification

Run the **staging-release** workflow (GitHub → Actions → staging-release →
*Run workflow*, choosing any branch). It:

1. runs the unit tests;
2. builds + signs with the **production** key (repo secrets);
3. publishes to the rolling **`staging` tag as a prerelease** — prereleases
   never resolve through `releases/latest/download/…`, so a staging build can
   never leak to real clients;
4. runs [`scripts/verify-release-assets.mjs`](../scripts/verify-release-assets.mjs),
   which fails the workflow unless the release carries exactly one
   `*-setup.exe`, its `.sig`, and a `latest.json` whose version matches the
   committed version, whose platform URLs point at actually-uploaded assets,
   and whose embedded signatures exactly match the uploaded `.sig` contents.

Step 4 is the guard against the **partial-success** hazard: a release that
exists (clients see an update) but whose installer/signature/manifest is
missing or mismatched (every update attempt fails). The same script gates
production drafts in `release.yml`. Its tested `--require-macos-universal`
mode additionally requires one DMG and one signed `.app.tar.gz` shared by the
`darwin-aarch64` and `darwin-x86_64` manifest entries; the flag stays disabled
until the signed macOS release job is added.

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

The production flow in [`RELEASING.md`](RELEASING.md) enforces the gate:
releases are created as **drafts**, verified automatically, smoke-tested via
staging, and only then published by a human.
