#!/bin/bash

# Fail before either workflow creates or mutates a GitHub Release when required
# updater-signing or Apple distribution credentials are missing/malformed.
# Values are consumed from the environment and never printed.

set -euo pipefail

required=(
  TAURI_SIGNING_PRIVATE_KEY
  APPLE_CERTIFICATE
  APPLE_CERTIFICATE_PASSWORD
  KEYCHAIN_PASSWORD
  APPLE_API_ISSUER
  APPLE_API_KEY
  APPLE_API_KEY_BASE64
)

for name in "${required[@]}"; do
  if [ -z "${!name:-}" ]; then
    echo "::error::Required release secret is missing: $name"
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

if ! printf '%s' "$APPLE_CERTIFICATE" | base64 --decode >/dev/null 2>&1; then
  echo "::error::APPLE_CERTIFICATE is not valid base64"
  exit 1
fi

if ! printf '%s' "$APPLE_API_KEY_BASE64" |
  base64 --decode 2>/dev/null |
  grep -q -- 'BEGIN PRIVATE KEY'; then
  echo "::error::APPLE_API_KEY_BASE64 is not a base64-encoded PEM private key"
  exit 1
fi

echo "Required updater and Apple release secrets passed structural preflight."
