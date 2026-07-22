#!/usr/bin/env bash
set -euo pipefail

UUID="codex-voice@andy-spike.github.io"
CLEANUP="/usr/lib/codex-voice/remove-legacy-shortcut.py"
SETUP_MARKER="$HOME/.config/codex-voice/extension-enabled-0.2.0"

# Remove the legacy settings-process autostart entry. Dictation is owned by
# the Shell extension and needs no background settings process.
rm -f "$HOME/.config/autostart/io.github.andy_spike.CodexVoice.desktop"
if command -v dconf >/dev/null 2>&1; then
  dconf reset /io/github/andy_spike/CodexVoice/launch-on-startup || true
  dconf reset /io/github/andy_spike/CodexVoice/start-hidden || true
fi

"$CLEANUP" >/dev/null 2>&1 || true
[[ -e "$SETUP_MARKER" ]] && exit 0
command -v gnome-extensions >/dev/null 2>&1 || exit 1

# The UUID remains in one of these lists across package upgrades. Respect
# either prior state; a UUID absent from both lists is a genuinely new setup.
enabled_extensions="$(gsettings get org.gnome.shell enabled-extensions 2>/dev/null || true)"
disabled_extensions="$(gsettings get org.gnome.shell disabled-extensions 2>/dev/null || true)"
if [[ "$enabled_extensions" == *"$UUID"* || "$disabled_extensions" == *"$UUID"* ]]; then
  mkdir -p "$(dirname "$SETUP_MARKER")"
  touch "$SETUP_MARKER"
  exit 0
fi

gnome-extensions enable "$UUID" >/dev/null 2>&1 || exit 1
enabled_extensions="$(gsettings get org.gnome.shell enabled-extensions 2>/dev/null || true)"
[[ "$enabled_extensions" == *"$UUID"* ]] || exit 1
mkdir -p "$(dirname "$SETUP_MARKER")"
touch "$SETUP_MARKER"
