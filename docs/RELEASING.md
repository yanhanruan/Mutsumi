# Releasing Mutsumi

Mutsumi's release workflow is configured to ship a signed Windows NSIS
installer and a Developer ID-signed, notarized universal macOS DMG through one
GitHub Release. Push a `vX.Y.Z` tag and
[`.github/workflows/release.yml`](../.github/workflows/release.yml) runs the
tests, creates a **draft** with Windows assets, appends macOS assets, merges both
platforms into `latest.json`, and verifies the complete release. You publish it
manually only after the upgrade smoke tests — see the release gate below and
[`TESTING-UPDATES.md`](TESTING-UPDATES.md). Once published, the running app
checks the manifest once a day and offers an in-app update.

The workflow wiring is complete, but a signed/notarized macOS positive run is
still pending the repository secrets and first credentialed staging dispatch
documented below. Do not interpret static workflow tests or unsigned desktop CI
as proof that Apple accepted a notarization submission.

## macOS CI build baseline (not a release)

[`desktop-ci.yml`](../.github/workflows/desktop-ci.yml) is a separate pull
request/main-branch gate. Its Windows job runs the existing frontend and Rust
regression suite. Its macOS job installs both Apple Rust targets and builds one
`universal-apple-darwin` app and DMG. The reusable bundle verifier requires the
executable to contain exactly `arm64` and `x86_64`, checks macOS 13.0 and the
bundle identifier from `Info.plist`, rejects Agent/background-only bundle flags
that would remove the permanent Dock icon, requires exactly one DMG, and runs
`hdiutil verify` against it:

```bash
node scripts/verify-macos-bundle.mjs \
  --bundle-dir src-tauri/target/universal-apple-darwin/release/bundle \
  --mode unsigned \
  --expected-minimum-system-version 13.0
```

Its artifact is deliberately named `unsigned-macos-universal-*`, retained for
only seven days, and is **not uploaded to GitHub Releases**. It exists to prove
the cross-architecture compile and packaging chain before Developer ID secrets
are connected. It has not passed signing, notarization, stapling or Gatekeeper
checks and must not be distributed to users.

`release.yml` and `staging-release.yml` first run a structural secret preflight,
then serialize three release jobs: Windows creates the release and initial
manifest, macOS builds one universal target and uploads to the same release ID,
and a final Linux job requires the complete Windows + macOS asset contract. The
preflight catches absent or basically malformed inputs before a GitHub Release
is created; the macOS import/build still performs the authoritative certificate,
identity, signing and notarization checks. Both platform uploads use
`tauri-action@v1`; the macOS upload reuses the canonical release notes because
v1 regenerates the top-level manifest metadata while preserving existing
platform entries. Because v1 emits GitHub API asset URLs, the final job resolves
the returned release ID, tag and draft/prerelease channel first, resolves each
API asset ID only against that same release, and rewrites `latest.json` to the
stable public `/releases/download/<tag>/<asset>` form before applying the
existing release contract. This avoids anonymous GitHub API limits without
weakening release, repository, tag or asset-name validation.

On a logged-in Mac, the opt-in lifecycle smoke launches a built app through
LaunchServices, verifies the requested Mach-O slice is a ready foreground/Dock
application, launches it a second time to exercise singleton activation, then
sends a standard macOS Quit Apple Event and verifies the process, LaunchServices
registration and single-instance socket are cleaned up. It refuses to run when
an instance with the same bundle identifier is already open:

```bash
npm run test:macos-lifecycle-smoke -- \
  --app src-tauri/target/universal-apple-darwin/release/bundle/macos/mutsumi.app \
  --arch arm64

# On Apple Silicon with Rosetta installed, also exercise the Intel slice:
npm run test:macos-lifecycle-smoke -- \
  --app src-tauri/target/universal-apple-darwin/release/bundle/macos/mutsumi.app \
  --arch x86_64
```

This smoke intentionally moves focus away from Mutsumi and then brings Mutsumi
back to the foreground while it runs. It proves LaunchServices/Dock eligibility
and activation, but it does not replace manual clicking of the Dock icon,
window hide/restore, multi-display or sleep/wake testing.

The release verifier's tested `--require-macos-universal` contract is enabled in
both signed release workflows. It requires one DMG for direct installation and
one signed `.app.tar.gz` updater archive referenced by both `darwin-aarch64` and
`darwin-x86_64`, in addition to the existing Windows NSIS assets.

The long-lived macOS bundle identifier is frozen as
`io.github.yanhanruan.mutsumi` in the platform-only
`src-tauri/tauri.macos.conf.json` overlay. The base identifier remains
`com.mutsumi.app` so existing Windows app-data and credential namespaces do not
move. Every signed macOS build must run the prepared bundle gate with the frozen
value:

```bash
node scripts/verify-macos-bundle.mjs \
  --bundle-dir src-tauri/target/universal-apple-darwin/release/bundle \
  --mode signed \
  --expected-identifier io.github.yanhanruan.mutsumi \
  --expected-minimum-system-version 13.0
```

Signed mode rejects an identifier ending in `.app` and requires a valid
Developer ID Application signature, hardened runtime, secure timestamp,
readable distribution entitlements without `get-task-allow`, Gatekeeper
acceptance for the app and DMG, and a notarization ticket stapled to the DMG.
Both signed workflows invoke this gate on the freshly built bundle before final
cross-platform asset verification. It has not yet produced a real positive
result because no Developer ID/App Store Connect credentials were supplied in
this adaptation session.

## One-time setup (before the first signed release)

The auto-updater verifies every download against a signing key. Mutsumi already
has a production public key committed in `plugins.updater.pubkey`; the release
secrets must use its matching existing private key. **The private key is a
secret—never commit it, and do not generate a replacement as part of the macOS
release work.** Rotating it without a migration would make installed Windows
clients reject future updates.

1. **Locate the existing production updater keypair** and compare its `.pub`
   contents byte-for-byte with `src-tauri/tauri.conf.json` →
   `plugins.updater.pubkey`. If the private key is missing or the values differ,
   stop and plan a coordinated updater-key recovery/rotation; do not overwrite
   the committed public key to make CI green.

   For a genuinely new, never-shipped product only, the original initialization
   command would be:

   ```bash
   npm run tauri signer generate -- -w "$HOME/.tauri/mutsumi.key"
   ```

   This repository is not in that state; the command is retained only as a
   reference for fresh deployments.

2. **Add the two updater-signing GitHub repo secrets** (Settings → Secrets and
   variables → Actions):

   | Secret | Value |
   | --- | --- |
   | `TAURI_SIGNING_PRIVATE_KEY` | the full contents of `~/.tauri/mutsumi.key` |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the password you chose in step 1 — **skip this secret entirely if the key has no password** (GitHub rejects empty secret values; an unset secret becomes an empty env var, which is exactly right for a passwordless key) |

3. **Leave the committed updater public key unchanged.** Re-check that the
   private key placed in GitHub is the matching key before dispatching staging.

4. **Create and export a `Developer ID Application` certificate.** Use Developer
   ID Application—not Apple Distribution, which is for the Mac App Store. Export
   the certificate and private key together as a password-protected `.p12`, then
   encode it as one base64 line:

   ```bash
   openssl base64 -A -in DeveloperIDApplication.p12 -out certificate-base64.txt
   ```

5. **Create an App Store Connect API key** under Users and Access → Integrations,
   record its issuer UUID and 10-character key ID, download the `.p8` private key
   once, and encode it without line wrapping:

   ```bash
   openssl base64 -A -in AuthKey_XXXXXXXXXX.p8 -out api-key-base64.txt
   ```

6. **Add the six Apple GitHub repo secrets:**

   | Secret | Value |
   | --- | --- |
   | `APPLE_CERTIFICATE` | contents of `certificate-base64.txt` |
   | `APPLE_CERTIFICATE_PASSWORD` | password used when exporting the `.p12` |
   | `KEYCHAIN_PASSWORD` | a new random CI-only password for the ephemeral keychain |
   | `APPLE_API_ISSUER` | App Store Connect issuer UUID |
   | `APPLE_API_KEY` | App Store Connect 10-character key ID |
   | `APPLE_API_KEY_BASE64` | contents of `api-key-base64.txt` |

The macOS jobs decode these only under `RUNNER_TEMP`, import exactly one valid
Developer ID Application identity into an ephemeral keychain, pass the `.p8`
path to Tauri for notarization, mask the selected identity, and clean up with an
`if: always()` best-effort step. The initial preflight rejects absent or
structurally malformed inputs before a release is created; certificate password,
Developer ID identity and notarization validity are proven later on the macOS
runner. Never paste any of these values into workflow YAML, logs, issues or this
documentation.

After the seven required updater/Apple secrets (plus the optional updater-key
password when applicable) have been configured, run the manual
**release-credential-readiness** workflow from GitHub Actions before the first
staging release. It is intentionally non-mutating and uses only
`contents: read`: it signs and verifies a disposable updater probe against the
public key committed in `tauri.conf.json`, signs a temporary executable with the
imported Developer ID identity, and authenticates to the Apple notary service
through a read-only history request. It does **not** create or delete a Release
or tag, upload an artifact, submit a notarization request, or publish anything.
Disposable probes are removed by guarded shell `EXIT` traps; the temporary
keychain, certificate and API key use a separate `if: always()` best-effort
cleanup step. A forcibly terminated job can skip process cleanup, so this check
must remain on an ephemeral GitHub-hosted runner and never uploads those paths.
A green result proves credential readiness, not a signed/notarized Mutsumi
bundle; staging remains the first end-to-end release gate.

Until updater steps 1–3 are done, the app still builds and runs but cannot verify
updates, and the daily check fails quietly when there is no `latest.json` to
fetch. Without Apple steps 4–6, signed staging/production macOS jobs fail closed
and no release may be considered cross-platform complete.

## Cutting a release (gated)

The version lives in exactly two files, kept in sync by one script — you never
hand-edit them, and the About window reads the version at runtime.

```bash
npm run release 1.5.0                 # writes 1.5.0 into tauri.conf.json + Cargo.toml
# write the user-facing notes file for this tag (reviewed in the release PR):
#   docs/release-notes/v1.5.0.md
git add docs/release-notes/v1.5.0.md
git commit -am "chore(release): v1.5.0"
# GUARD — must print the version you're releasing (1.5.0 in this example);
# an older number means the release PR isn't merged/pulled yet, so don't tag:
node -p "require('./src-tauri/tauri.conf.json').version"
git tag -a v1.5.0 -m "v1.5.0"
git push --follow-tags
```

- **`docs/release-notes/vX.Y.Z.md`** becomes the GitHub Release body **and**
  the release notes shown in the in-app update pop-up. Write it for users —
  see [`docs/release-notes/README.md`](release-notes/README.md). Because it's
  a committed file, it rides in the release PR and gets reviewed like code.
  If the file is absent, CI falls back to the **annotated tag message**.
- **Notes must be final before the tag is pushed** — `latest.json` freezes
  them at draft time; editing the GitHub release page afterwards changes the
  web page only, never the in-app pop-up.
- CI then appends an auto-generated changelog to the **GitHub release body
  only**: a "What's Changed" section plus a `Full Changelog: vPREV...vNEW`
  compare link against the previous tag. The in-app pop-up notes stay as just
  your hand-written tag message (`latest.json` is generated before the append).
- CI **fails fast** if the tag (`v1.5.0`) doesn't match the committed version
  (`1.5.0`), so a mistagged release can't ship.
- With a notes file committed, the tag message no longer carries the notes —
  a plain `git tag -a v1.5.0 -m "v1.5.0"` (or even a lightweight tag) is fine.
  Only when there is no notes file does the annotated tag message matter.

### The release gate

Pushing the tag does **not** publish anything to users. The full gate is:

```text
tag push
  → CI: unit tests
  → CI/Windows: build + sign NSIS, create DRAFT release
  → CI/macOS: build universal app/DMG, Developer ID sign, notarize + staple,
              upload to the same release and merge latest.json
  → CI: verify signed macOS bundle and complete Windows + macOS assets
  → you: staging upgrade smoke test  (docs/TESTING-UPDATES.md, Layer 4)
  → you: click "Publish release" on GitHub
  → CI: post-publish gate — verify the live latest.json download resolves,
        auto-rebind an orphaned `untagged-*` release (heal-published-release.mjs)
```

- Drafts (and prereleases) never resolve through
  `releases/latest/download/latest.json`, so an unverified build cannot reach
  users even though the release object already exists.
- The verification step blocks the "partial upload" hazard: it fails unless the
  draft carries the Windows installer, universal macOS DMG, both updater
  archives/signatures, and a `latest.json` whose version and every platform URL
  match assets on that release. Each embedded signature must exactly match its
  uploaded `.sig`; a merely non-empty but stale signature is rejected.
- For the smoke test (and for exercising failure cases like tampered
  signatures against a real build), follow
  [`TESTING-UPDATES.md`](TESTING-UPDATES.md) — it also documents the
  `staging-release` workflow, which publishes to a rolling `staging`
  prerelease that production clients can never see.

### The post-publish gate (auto-heal)

Publishing the draft is where a subtle failure hides. `tauri-action` uploads the
draft's assets under an `untagged-<hash>` placeholder ref, but bakes
`latest.json`'s installer URL from the **tag** (`/releases/download/vX.Y.Z/...`),
expecting publish to move the assets onto the tag path. Because our tag is pushed
*first* (it's the build trigger), publishing can leave the release **orphaned on
the `untagged-*` ref** instead of bound to `vX.Y.Z` — so `latest.json`'s
version-check still succeeds, but its download URL **404s**, and every client's
update fails. (This silently bit v1.5.0 and v1.5.1.)

The draft-time gate can't catch it: the URL only settles on publish. So a second
workflow — [`verify-published-release.yml`](../.github/workflows/verify-published-release.yml)
— runs on the `release: published` event (i.e. the moment you click **Publish**)
and via [`heal-published-release.mjs`](../scripts/heal-published-release.mjs):

1. reads the live `latest.json` → every distinct updater URL clients may fetch;
2. if the release isn't bound to `vX.Y.Z`, **rebinds it** (PATCH `tag_name`,
   the same fix as editing the tag in the UI) using the in-CI `GITHUB_TOKEN`;
3. verifies every URL actually resolves, and **fails loudly** if any still does
   not.

Drafts remain owned by the pre-publish gate. Prereleases are also skipped here:
the rolling staging release must stay bound to the literal `staging` tag and
must never be "healed" onto a production `vX.Y.Z` tag.

So a normal release self-heals after you publish. If you ever need to re-run it
by hand (e.g. to repair an old release), trigger the workflow manually:
**Actions → verify-published-release → Run workflow → version `X.Y.Z`**.

## How the in-app updater consumes this

- `plugins.updater.endpoints` points at
  `https://github.com/yanhanruan/Mutsumi/releases/latest/download/latest.json`,
  which always resolves to the newest published release.
- `tauri-action` generates `latest.json` (version, signature, download URL, and
  `notes` = the release body) and attaches it to the release.
- The pet window checks once a day (see `src/composables/useUpdateCheck.ts`);
  the pop-up (`src/components/UpdateWindow.vue`) shows the notes and lets the
  user install now or snooze 1–30 days.
