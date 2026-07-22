#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

GIT_ROOT="$(git -C "$ROOT_DIR" rev-parse --show-toplevel 2>/dev/null || true)"
if [[ "$GIT_ROOT" == "$ROOT_DIR" ]]; then
  WORKTREE_STATUS="$(git -C "$ROOT_DIR" status --short --untracked-files=all)"
  if [[ -n "$WORKTREE_STATUS" ]]; then
    echo "release build requires a clean committed worktree; commit, stash, or remove these changes:" >&2
    printf '%s\n' "$WORKTREE_STATUS" >&2
    exit 2
  fi
fi

if [[ -z "${SOURCE_DATE_EPOCH:-}" ]]; then
  if ! SOURCE_DATE_EPOCH="$(git -C "$ROOT_DIR" show -s --format=%ct HEAD 2>/dev/null)" || [[ -z "$SOURCE_DATE_EPOCH" ]]; then
    echo "SOURCE_DATE_EPOCH is required when the release commit is unavailable" >&2
    exit 2
  fi
fi
[[ "$SOURCE_DATE_EPOCH" =~ ^[0-9]+$ ]] || { echo "SOURCE_DATE_EPOCH must be a non-negative integer" >&2; exit 2; }
export SOURCE_DATE_EPOCH
export LC_ALL=C
export TZ=UTC
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"
export CARGO_INCREMENTAL=0

cargo clean --release --target-dir "$CARGO_TARGET_DIR" --manifest-path "$ROOT_DIR/Cargo.toml"
cargo build --release --locked --manifest-path "$ROOT_DIR/Cargo.toml"
"$ROOT_DIR/packaging/fetch-codex-asr.sh"
"$ROOT_DIR/packaging/build-deb.sh"

echo "Built Codex Voice package in ${OUTPUT_DIR:-$ROOT_DIR/dist}"
