#!/bin/bash

# Exercise the imported Developer ID private key and authenticate to Apple's
# notary service using a read-only history request. This never submits a build.

set -euo pipefail
umask 077

required=(
  APPLE_SIGNING_IDENTITY
  APPLE_API_KEY_PATH
  APPLE_API_ISSUER
  APPLE_API_KEY
  RUNNER_TEMP
)

for name in "${required[@]}"; do
  if [ -z "${!name:-}" ]; then
    echo "::error::Required macOS release readiness variable is missing: $name"
    exit 1
  fi
done

case "$APPLE_API_KEY_PATH" in
  "$RUNNER_TEMP"/*) ;;
  *)
    echo "::error::App Store Connect API key must be materialized under RUNNER_TEMP"
    exit 1
    ;;
esac

codesign_probe="$(mktemp "$RUNNER_TEMP/mutsumi-codesign-readiness.XXXXXX")"
notary_history="$(mktemp "$RUNNER_TEMP/mutsumi-notary-history.XXXXXX")"

cleanup() {
  /bin/rm -f -- "$codesign_probe" "$notary_history"
}
trap cleanup EXIT

/bin/cp /usr/bin/true "$codesign_probe"
/usr/bin/codesign \
  --force \
  --sign "$APPLE_SIGNING_IDENTITY" \
  --options runtime \
  --timestamp \
  "$codesign_probe"
/usr/bin/codesign --verify --strict --verbose=2 "$codesign_probe"

xcrun notarytool history \
  --key "$APPLE_API_KEY_PATH" \
  --key-id "$APPLE_API_KEY" \
  --issuer "$APPLE_API_ISSUER" \
  --output-format json \
  > "$notary_history"

node -e '
  const fs = require("node:fs");
  const value = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
  if (!value || typeof value !== "object" || Array.isArray(value)) process.exit(1);
' "$notary_history"

echo "Developer ID signing and read-only notary authentication succeeded."
