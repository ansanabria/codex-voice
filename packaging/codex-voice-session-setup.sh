#!/usr/bin/env bash
set -euo pipefail

UUID="codex-voice@andy-spike.github.io"
HELPER="/usr/lib/codex-voice/gnome-custom-shortcuts.py"

# Migration from releases that could launch the Electron settings process at
# login. Dictation is owned by the Shell extension and needs no background app.
rm -f "$HOME/.config/autostart/io.github.andy_spike.CodexVoice.desktop"
if command -v dconf >/dev/null 2>&1; then
  dconf reset /io/github/andy_spike/CodexVoice/launch-on-startup || true
  dconf reset /io/github/andy_spike/CodexVoice/start-hidden || true
fi

if command -v gnome-extensions >/dev/null 2>&1 && gnome-extensions enable "$UUID" >/dev/null 2>&1; then
  "$HELPER" remove >/dev/null 2>&1 || true
else
  "$HELPER" install >/dev/null 2>&1 || true
fi
