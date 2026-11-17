#!/bin/bash

# Import the Developer ID Application certificate and materialize the App Store
# Connect API key on an ephemeral GitHub-hosted macOS runner. Secrets are read
# only from environment variables and are never printed.

set -euo pipefail
umask 077

required=(
  APPLE_CERTIFICATE
  APPLE_CERTIFICATE_PASSWORD
  KEYCHAIN_PASSWORD
  APPLE_API_ISSUER
  APPLE_API_KEY
  APPLE_API_KEY_BASE64
  GITHUB_ENV
  RUNNER_TEMP
)

for name in "${required[@]}"; do
  if [ -z "${!name:-}" ]; then
    echo "::error::Required macOS signing environment variable is missing: $name"
    exit 1
  fi
done

if [[ ! "$APPLE_API_KEY" =~ ^[A-Z0-9]{10}$ ]]; then
  echo "::error::APPLE_API_KEY must be a 10-character App Store Connect key ID"
  exit 1
fi

if [[ ! "$APPLE_API_ISSUER" =~ ^[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}$ ]]; then
  echo "::error::APPLE_API_ISSUER must be an App Store Connect issuer UUID"
  exit 1
fi

keychain_path="$RUNNER_TEMP/mutsumi-signing.keychain-db"
certificate_path="$RUNNER_TEMP/mutsumi-developer-id.p12"
api_key_path="$RUNNER_TEMP/AuthKey_${APPLE_API_KEY}.p8"

# Record cleanup targets before materializing any secret so an `if: always()`
# cleanup step can remove partial setup after any later failure.
{
  printf 'MUTSUMI_SIGNING_KEYCHAIN=%s\n' "$keychain_path"
  printf 'MUTSUMI_SIGNING_CERTIFICATE=%s\n' "$certificate_path"
  printf 'MUTSUMI_SIGNING_API_KEY=%s\n' "$api_key_path"
} >> "$GITHUB_ENV"

printf '%s' "$APPLE_CERTIFICATE" | /usr/bin/base64 --decode > "$certificate_path"
printf '%s' "$APPLE_API_KEY_BASE64" | /usr/bin/base64 --decode > "$api_key_path"

if [ ! -s "$certificate_path" ]; then
  echo "::error::Decoded Developer ID certificate is empty"
  exit 1
fi

if ! /usr/bin/grep -q -- 'BEGIN PRIVATE KEY' "$api_key_path"; then
  echo "::error::Decoded App Store Connect API key is not a PEM private key"
  exit 1
fi

/usr/bin/security create-keychain -p "$KEYCHAIN_PASSWORD" "$keychain_path"
/usr/bin/security set-keychain-settings -lut 21600 "$keychain_path"
/usr/bin/security unlock-keychain -p "$KEYCHAIN_PASSWORD" "$keychain_path"
/usr/bin/security default-keychain -d user -s "$keychain_path"
/usr/bin/security list-keychains -d user -s "$keychain_path"
/usr/bin/security import "$certificate_path" \
  -k "$keychain_path" \
  -P "$APPLE_CERTIFICATE_PASSWORD" \
  -T /usr/bin/codesign \
  -T /usr/bin/security
/usr/bin/security set-key-partition-list \
  -S apple-tool:,apple:,codesign: \
  -s \
  -k "$KEYCHAIN_PASSWORD" \
  "$keychain_path" >/dev/null

identities="$(
  /usr/bin/security find-identity -v -p codesigning "$keychain_path" |
    /usr/bin/awk -F '"' '$2 ~ /^Developer ID Application:/ { print $2 }'
)"
identity_count="$(printf '%s\n' "$identities" | /usr/bin/awk 'NF { count++ } END { print count + 0 }')"

if [ "$identity_count" -ne 1 ]; then
  echo "::error::Expected exactly one valid Developer ID Application identity; found $identity_count"
  exit 1
fi

identity="$(printf '%s\n' "$identities" | /usr/bin/awk 'NF { print; exit }')"
echo "::add-mask::$identity"
{
  printf 'APPLE_SIGNING_IDENTITY=%s\n' "$identity"
  printf 'APPLE_API_KEY_PATH=%s\n' "$api_key_path"
} >> "$GITHUB_ENV"

echo "Developer ID Application identity and App Store Connect API key are ready."
