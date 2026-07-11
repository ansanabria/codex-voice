#!/usr/bin/env bash
set -euo pipefail

# Keep the packaged runtime self-contained. The checksum and release are pinned
# so building the Debian artifact does not silently package an unreviewed ASR
# binary.
VERSION="0.1.2"
ARCHIVE="codex-asr-x86_64-unknown-linux-gnu.tar.xz"
URL="https://github.com/wangnov/codex-asr/releases/download/v${VERSION}/${ARCHIVE}"
EXPECTED_SHA256="16a27b87d45f91caaf9f0803cc4674a05f1f2bb5251b129426e7e502eba24f33"
EXPECTED_BINARY_SHA256="b7f535e889f1a06d130b0614bf0009333f1d45c7189f6b2839fb4111bf592038"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DESTINATION="$ROOT_DIR/packaging/resources/codex-asr"

if [[ -x "$DESTINATION" ]]; then
  actual="$(sha256sum "$DESTINATION" | awk '{print $1}')"
  if [[ "$EXPECTED_BINARY_SHA256" == "${CODEX_ASR_BINARY_SHA256:-$actual}" ]]; then
    exit 0
  fi
fi

temporary_root="$(mktemp -d)"
trap 'rm -rf "$temporary_root"' EXIT
archive="$temporary_root/$ARCHIVE"
curl --proto '=https' --tlsv1.2 -fLsSf "$URL" -o "$archive"
printf '%s  %s\n' "$EXPECTED_SHA256" "$archive" | sha256sum --check --status
tar -xJf "$archive" -C "$temporary_root"
install -Dm755 "$temporary_root/codex-asr-x86_64-unknown-linux-gnu/codex-asr" "$DESTINATION"

actual="$(sha256sum "$DESTINATION" | awk '{print $1}')"
[[ "$actual" == "$EXPECTED_BINARY_SHA256" ]]
printf 'Prepared codex-asr %s (%s)\n' "$VERSION" "$actual"
