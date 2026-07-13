#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"
"$ROOT_DIR/packaging/fetch-codex-asr.sh"
npm --prefix "$ROOT_DIR/settings" run build
npm --prefix "$ROOT_DIR/settings" run package:deb

echo "Built Codex Voice packages in $ROOT_DIR/dist"
