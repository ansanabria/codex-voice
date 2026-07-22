#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GIT_ROOT="$(git -C "$ROOT_DIR" rev-parse --show-toplevel 2>/dev/null || true)"
[[ "$GIT_ROOT" == "$ROOT_DIR" ]] || {
  echo "reproducibility check must run from a Git worktree root" >&2
  exit 2
}

WORKTREE_STATUS="$(git -C "$ROOT_DIR" status --short --untracked-files=all)"
if [[ -n "$WORKTREE_STATUS" ]]; then
  echo "reproducibility check requires a clean committed release tree; commit, stash, or remove these changes:" >&2
  printf '%s\n' "$WORKTREE_STATUS" >&2
  exit 2
fi

RELEASE_COMMIT="$(git -C "$ROOT_DIR" rev-parse --verify 'HEAD^{commit}')"
if git -C "$ROOT_DIR" cat-file -e "$RELEASE_COMMIT:packaging/resources/codex-asr" 2>/dev/null; then
  echo "release commit must not track packaging/resources/codex-asr; the pinned binary must come from the verified fetch path" >&2
  exit 2
fi
SOURCE_DATE_EPOCH="$(git -C "$ROOT_DIR" show -s --format=%ct "$RELEASE_COMMIT")"
export SOURCE_DATE_EPOCH

mkdir -p "$ROOT_DIR/tmp"
TEST_ROOT="$(mktemp -d "$ROOT_DIR/tmp/reproducible-build.XXXXXX")"
trap 'rm -rf "$TEST_ROOT"' EXIT
SOURCE_ARCHIVE="$TEST_ROOT/source.tar"
git -C "$ROOT_DIR" archive --format=tar --output="$SOURCE_ARCHIVE" "$RELEASE_COMMIT"

for build in 1 2; do
  source_root="$TEST_ROOT/source-$build"
  mkdir -p "$source_root"
  tar --extract --file "$SOURCE_ARCHIVE" --directory "$source_root"

  CARGO_TARGET_DIR="$TEST_ROOT/target-$build" \
    STAGE="$TEST_ROOT/stage-$build" \
    OUTPUT_DIR="$TEST_ROOT/dist-$build" \
    "$source_root/build.sh"

  (cd "$TEST_ROOT/dist-$build" && sha256sum --check SHA256SUMS)
done

shopt -s nullglob
first_packages=("$TEST_ROOT/dist-1"/*.deb)
second_packages=("$TEST_ROOT/dist-2"/*.deb)
[[ "${#first_packages[@]}" -eq 1 && "${#second_packages[@]}" -eq 1 ]] || {
  echo "expected exactly one Debian package from each build" >&2
  exit 1
}

first_hash="$(sha256sum "${first_packages[0]}" | cut -d' ' -f1)"
second_hash="$(sha256sum "${second_packages[0]}" | cut -d' ' -f1)"
cmp --silent "${first_packages[0]}" "${second_packages[0]}" || {
  echo "release builds are not reproducible: $first_hash != $second_hash" >&2
  exit 1
}
cmp --silent "$TEST_ROOT/dist-1/SHA256SUMS" "$TEST_ROOT/dist-2/SHA256SUMS"

FINAL_COMMIT="$(git -C "$ROOT_DIR" rev-parse --verify 'HEAD^{commit}')"
FINAL_WORKTREE_STATUS="$(git -C "$ROOT_DIR" status --short --untracked-files=all)"
if [[ "$FINAL_COMMIT" != "$RELEASE_COMMIT" || -n "$FINAL_WORKTREE_STATUS" ]]; then
  echo "release tree changed during the reproducibility check; rerun from a stable clean commit" >&2
  [[ -z "$FINAL_WORKTREE_STATUS" ]] || printf '%s\n' "$FINAL_WORKTREE_STATUS" >&2
  exit 2
fi

printf 'Reproducible Debian package: %s\n' "$(basename "${first_packages[0]}")"
printf 'SHA-256: %s\n' "$first_hash"
printf 'Release commit: %s\n' "$RELEASE_COMMIT"
printf 'SOURCE_DATE_EPOCH: %s\n' "$SOURCE_DATE_EPOCH"
