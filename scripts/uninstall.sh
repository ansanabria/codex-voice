#!/usr/bin/env bash
set -euo pipefail

PURGE=false
[[ "${1:-}" == "--purge" ]] && PURGE=true
[[ $# -le 1 ]] || { echo "usage: $0 [--purge]" >&2; exit 2; }

INSTALL_BIN="$HOME/.local/bin"
DATA_DIR="$HOME/.local/share/codex-voice"
EXTENSION_UUID="codex-voice@andy-spike.github.io"
EXTENSION_DIR="$HOME/.local/share/gnome-shell/extensions/$EXTENSION_UUID"

gnome-extensions disable "$EXTENSION_UUID" 2>/dev/null || true
if [[ -x "$DATA_DIR/gnome-custom-shortcuts.py" ]]; then
  "$DATA_DIR/gnome-custom-shortcuts.py" remove || echo "WARNING: could not remove legacy GNOME shortcuts." >&2
fi
rm -rf "$EXTENSION_DIR"
rm -f "$INSTALL_BIN/codex-voice" "$INSTALL_BIN/codex-voice-settings"
rm -f "$HOME/.local/share/applications/io.github.andy_spike.CodexVoice.desktop"
rm -f "$HOME/.local/share/icons/hicolor/scalable/apps/codex-voice.svg"
rm -f "$DATA_DIR/overlay.py" "$DATA_DIR/gnome-custom-shortcuts.py"
for package in codex-voice codex-voice-settings; do
  if dpkg-query -W -f='${Status}' "$package" 2>/dev/null | grep -q 'ok installed'; then
    sudo apt-get remove -y "$package"
  fi
done

if "$PURGE"; then
  GSETTINGS_SCHEMA_DIR="$DATA_DIR/schemas${GSETTINGS_SCHEMA_DIR:+:$GSETTINGS_SCHEMA_DIR}" gsettings reset-recursively io.github.andy_spike.CodexVoice 2>/dev/null || true
  rm -rf "$DATA_DIR"
  echo "Removed Codex Voice and purged saved preferences."
else
  echo "Removed application components. Saved GSettings preferences were preserved; use --purge to reset them."
fi
echo "Shared system packages and codex-asr were not removed."
