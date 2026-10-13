#!/bin/bash

# Sign a fixed, non-secret probe with the configured updater private key and
# verify it against the public key committed in tauri.conf.json. No key material
# is written to disk or printed.

set -euo pipefail
umask 077

for name in RUNNER_TEMP TAURI_SIGNING_PRIVATE_KEY; do
  if [ -z "${!name:-}" ]; then
    echo "::error::Required updater signing environment variable is missing: $name"
    exit 1
  fi
done

probe_path="$(mktemp "$RUNNER_TEMP/mutsumi-updater-readiness.XXXXXX")"
signature_path="$probe_path.sig"

cleanup() {
  /bin/rm -f -- "$probe_path" "$signature_path"
}
trap cleanup EXIT

printf 'mutsumi release credential readiness probe\n' > "$probe_path"
npx --no-install tauri signer sign "$probe_path" >/dev/null

if [ ! -s "$signature_path" ]; then
  echo "::error::Tauri signer did not create the expected probe signature"
  exit 1
fi

node scripts/verify-updater-signature.mjs \
  --config src-tauri/tauri.conf.json \
  --file "$probe_path" \
  --signature "$signature_path"
