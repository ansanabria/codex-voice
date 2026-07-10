#!/usr/bin/env bash
# codex-voice uninstall script
# Removes scripts from ~/.local/bin and the GNOME custom shortcut.
set -euo pipefail

INSTALL_DIR="$HOME/.local/bin"
BINDING_PREFIX="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings"

echo "=== codex-voice uninstaller ==="

# --- Remove scripts ---
if [ -f "$INSTALL_DIR/codex-voice" ]; then
  rm -f "$INSTALL_DIR/codex-voice"
  echo "Removed: $INSTALL_DIR/codex-voice"
else
  echo "Script not found at $INSTALL_DIR/codex-voice"
fi

if [ -d "$HOME/.local/share/codex-voice" ]; then
  rm -rf "$HOME/.local/share/codex-voice"
  echo "Removed: $HOME/.local/share/codex-voice/"
fi

# --- Remove GNOME shortcut ---
existing=$(gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings)

NEW_LIST=$(python3 -c "
import ast, sys
s = sys.argv[1].replace('@as []', '[]')
items = ast.literal_eval(s)
filtered = [item for item in items if 'codex-voice' not in item and 'Codex Voice' not in item]
# Also check the command for each remaining item to be safe
print(repr(filtered).replace(chr(39), chr(34)))
" "$existing" 2>/dev/null || echo "$existing")

if [ "$NEW_LIST" != "$existing" ]; then
  gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "$NEW_LIST"
  echo "Removed GNOME custom shortcut entries."
else
  echo "No GNOME custom shortcut found."
fi

echo ""
echo "=== Uninstall complete ==="
echo ""
echo "Note: codex-asr was installed via cargo and is NOT removed."
echo "  To remove it: cargo uninstall codex-asr"
echo ""
echo "Note: System packages (ydotool, wl-clipboard, etc.) are NOT removed."
