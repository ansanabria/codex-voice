#!/usr/bin/env bash
set -euo pipefail

# Keep the packaged runtime self-contained. The checksum and release are pinned
# so building the Debian artifact does not silently package an unreviewed ASR
# binary.
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/packaging/codex-asr-release.sh"
DESTINATION="$ROOT_DIR/packaging/resources/codex-asr"
SOURCE_ARCHIVE=""

while (( $# > 0 )); do
  case "$1" in
    --archive)
      SOURCE_ARCHIVE="${2:?--archive requires a path}"
      shift 2
      ;;
    --destination)
      DESTINATION="${2:?--destination requires a path}"
      shift 2
      ;;
    *)
      echo "usage: $0 [--archive PATH] [--destination PATH]" >&2
      exit 2
      ;;
  esac
done

if [[ -x "$DESTINATION" ]]; then
  actual="$(sha256sum "$DESTINATION" | awk '{print $1}')"
  if [[ "$actual" == "$PINNED_CODEX_ASR_BINARY_SHA256" ]]; then
    exit 0
  fi
fi

mkdir -p "$ROOT_DIR/tmp"
temporary_root="$(mktemp -d "$ROOT_DIR/tmp/fetch-codex-asr.XXXXXX")"
trap 'rm -rf "$temporary_root"' EXIT
archive="${SOURCE_ARCHIVE:-$temporary_root/$PINNED_CODEX_ASR_ARCHIVE}"
if [[ -z "$SOURCE_ARCHIVE" ]]; then
  curl --proto '=https' --tlsv1.2 -fLsSf "$PINNED_CODEX_ASR_URL" -o "$archive"
fi
[[ -f "$archive" ]] || { echo "codex-asr archive not found: $archive" >&2; exit 2; }
printf '%s  %s\n' "$PINNED_CODEX_ASR_ARCHIVE_SHA256" "$archive" | sha256sum --check --status
tar -xJf "$archive" -C "$temporary_root"
prepared="$temporary_root/codex-asr-x86_64-unknown-linux-gnu/codex-asr"
prepared_digest="$(sha256sum "$prepared" | awk '{print $1}')"
[[ "$prepared_digest" == "$PINNED_CODEX_ASR_BINARY_SHA256" ]] || {
  echo "prepared codex-asr digest mismatch: expected $PINNED_CODEX_ASR_BINARY_SHA256, got $prepared_digest" >&2
  exit 1
}
install -Dm755 "$prepared" "$DESTINATION"

actual="$(sha256sum "$DESTINATION" | awk '{print $1}')"
[[ "$actual" == "$PINNED_CODEX_ASR_BINARY_SHA256" ]] || {
  echo "installed codex-asr digest mismatch: expected $PINNED_CODEX_ASR_BINARY_SHA256, got $actual" >&2
  exit 1
}
printf 'Prepared codex-asr %s (%s)\n' "$PINNED_CODEX_ASR_VERSION" "$actual"
