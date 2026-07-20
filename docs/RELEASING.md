# Releasing Mutsumi

Mutsumi ships as a signed Windows NSIS installer. Releases are automated by the
GitHub Actions workflow [`.github/workflows/release.yml`](../.github/workflows/release.yml):
push a `vX.Y.Z` tag and CI runs the tests, builds, signs, and creates a **draft**
GitHub Release with the installer and the updater manifest (`latest.json`),
then verifies the release is complete and internally consistent. You publish it
manually after the upgrade smoke test — see the release gate below and
[`TESTING-UPDATES.md`](TESTING-UPDATES.md). Once published, the running app
checks the manifest once a day and offers an in-app update.

## One-time setup (before the first signed release)

The auto-updater verifies every download against a signing key, so you must
generate a keypair once and wire it up. **The private key is a secret — never
commit it.**

1. **Generate the keypair** (writes a private key file, prints the public key):

   ```bash
   npm run tauri signer generate -- -w "$HOME/.tauri/mutsumi.key"
   ```

   You'll set a password when prompted — remember it for step 2.

2. **Add two GitHub repo secrets** (Settings → Secrets and variables → Actions):

   | Secret | Value |
   | --- | --- |
   | `TAURI_SIGNING_PRIVATE_KEY` | the full contents of `~/.tauri/mutsumi.key` |
   | `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | the password you chose in step 1 — **skip this secret entirely if the key has no password** (GitHub rejects empty secret values; an unset secret becomes an empty env var, which is exactly right for a passwordless key) |

3. **Paste the public key** into
   [`src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json) →
   `plugins.updater.pubkey`, replacing the
   `REPLACE_WITH_MINISIGN_PUBLIC_KEY_FROM_TAURI_SIGNER_GENERATE` placeholder with
   the public key printed in step 1 (also saved as `~/.tauri/mutsumi.key.pub`).
   Commit this change.

Until this is done the app still builds and runs; it just can't verify updates,
and the daily check fails quietly (there is no `latest.json` to fetch yet).

## Cutting a release (gated)

The version lives in exactly two files, kept in sync by one script — you never
hand-edit them, and the About window reads the version at runtime.

```bash
npm run release 1.5.0                 # writes 1.5.0 into tauri.conf.json + Cargo.toml
git commit -am "chore(release): v1.5.0"
git tag -a v1.5.0 -m "What's new:
- Fixed the flying-mode ↔ music-mode transition
- Added the in-app auto-updater"
git push --follow-tags
```

- The **annotated tag message** becomes the GitHub Release body **and** the
  release notes shown in the in-app update pop-up. Write it for users.
- CI then appends an auto-generated changelog to the **GitHub release body
  only**: a "What's Changed" section plus a `Full Changelog: vPREV...vNEW`
  compare link against the previous tag. The in-app pop-up notes stay as just
  your hand-written tag message (`latest.json` is generated before the append).
- CI **fails fast** if the tag (`v1.5.0`) doesn't match the committed version
  (`1.5.0`), so a mistagged release can't ship.
- A lightweight tag (`git tag v1.5.0`) also works, but then the release body is
  empty — prefer an annotated tag so users see real notes.

### The release gate

Pushing the tag does **not** publish anything to users. The full gate is:

```text
tag push
  → CI: unit tests
  → CI: build + sign
  → CI: create DRAFT release (invisible to the updater endpoint)
  → CI: verify assets + latest.json contract (scripts/verify-release-assets.mjs)
  → you: staging upgrade smoke test  (docs/TESTING-UPDATES.md, Layer 4)
  → you: click "Publish release" on GitHub
  → CI: post-publish gate — verify the live latest.json download resolves,
        auto-rebind an orphaned `untagged-*` release (heal-published-release.mjs)
```

- Drafts (and prereleases) never resolve through
  `releases/latest/download/latest.json`, so an unverified build cannot reach
  users even though the release object already exists.
- The verification step blocks the "partial upload" hazard: it fails the
  workflow unless the draft carries the installer, its `.sig`, and a
  `latest.json` whose version and download URL match the actually-uploaded
  assets.
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

1. reads the live `latest.json` → the exact installer URL clients will fetch;
2. if the release isn't bound to `vX.Y.Z`, **rebinds it** (PATCH `tag_name`,
   the same fix as editing the tag in the UI) using the in-CI `GITHUB_TOKEN`;
3. verifies the URL actually resolves, and **fails loudly** if it still doesn't.

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
