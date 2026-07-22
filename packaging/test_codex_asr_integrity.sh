#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEB="${1:?usage: $0 path/to/codex-voice.deb}"
source "$ROOT_DIR/packaging/codex-asr-release.sh"

mkdir -p "$ROOT_DIR/tmp"
TEST_ROOT="$(mktemp -d "$ROOT_DIR/tmp/test-codex-asr-integrity.XXXXXX")"
trap 'rm -rf "$TEST_ROOT"' EXIT

archive="$TEST_ROOT/$PINNED_CODEX_ASR_ARCHIVE"
curl --proto '=https' --tlsv1.2 -fLsSf "$PINNED_CODEX_ASR_URL" -o "$archive"
printf '%s  %s\n' "$PINNED_CODEX_ASR_ARCHIVE_SHA256" "$archive" | sha256sum --check --status

tampered_binary="$TEST_ROOT/codex-asr"
printf '#!/usr/bin/env sh\necho tampered\n' > "$tampered_binary"
chmod 755 "$tampered_binary"
tampered_digest="$(sha256sum "$tampered_binary" | awk '{print $1}')"
CODEX_ASR_BINARY_SHA256="$tampered_digest" "$ROOT_DIR/packaging/fetch-codex-asr.sh" \
  --archive "$archive" \
  --destination "$tampered_binary"
actual="$(sha256sum "$tampered_binary" | awk '{print $1}')"
[[ "$actual" == "$PINNED_CODEX_ASR_BINARY_SHA256" ]] || {
  echo "tampered existing codex-asr was not replaced with the pinned binary" >&2
  exit 1
}

dpkg-deb --raw-extract "$DEB" "$TEST_ROOT/package-root"
printf 'tampered\n' >> "$TEST_ROOT/package-root/usr/lib/codex-voice/codex-asr"
(cd "$TEST_ROOT/package-root" && find . -type f ! -path './DEBIAN/*' -printf '%P\0' | sort -z | xargs -0 md5sum > DEBIAN/md5sums)
dpkg-deb --root-owner-group --build "$TEST_ROOT/package-root" "$TEST_ROOT/tampered.deb" >/dev/null
if "$ROOT_DIR/packaging/inspect-deb.sh" "$TEST_ROOT/tampered.deb" >"$TEST_ROOT/inspection.log" 2>&1; then
  echo "package inspection accepted a tampered codex-asr" >&2
  exit 1
fi
grep -q '^bundled codex-asr digest mismatch:' "$TEST_ROOT/inspection.log" || {
  echo "package inspection failed for an unexpected reason:" >&2
  cat "$TEST_ROOT/inspection.log" >&2
  exit 1
}

echo "codex-asr integrity tests passed"
