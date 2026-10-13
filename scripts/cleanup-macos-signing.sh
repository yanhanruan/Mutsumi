#!/bin/bash

# Best-effort cleanup for secrets materialized by setup-macos-signing.sh.

set -u

runner_temp="${RUNNER_TEMP:-}"
if [ -z "$runner_temp" ]; then
  echo "::warning::RUNNER_TEMP is unavailable; refusing to resolve signing cleanup paths"
  exit 0
fi

keychain_path="${MUTSUMI_SIGNING_KEYCHAIN:-$runner_temp/mutsumi-signing.keychain-db}"
certificate_path="${MUTSUMI_SIGNING_CERTIFICATE:-$runner_temp/mutsumi-developer-id.p12}"
api_key_path="${MUTSUMI_SIGNING_API_KEY:-}"

safe_runner_path() {
  case "$1" in
    "$runner_temp"/*) return 0 ;;
    *) return 1 ;;
  esac
}

if safe_runner_path "$keychain_path"; then
  /usr/bin/security delete-keychain "$keychain_path" >/dev/null 2>&1 || true
else
  echo "::warning::Refusing to delete keychain outside RUNNER_TEMP"
fi

for path in "$certificate_path" "$api_key_path" "$keychain_path"; do
  if [ -n "$path" ] && safe_runner_path "$path"; then
    /bin/rm -f -- "$path"
  elif [ -n "$path" ]; then
    echo "::warning::Refusing to delete signing material outside RUNNER_TEMP"
  fi
done

echo "Ephemeral macOS signing material removed."
