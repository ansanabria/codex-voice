#!/usr/bin/env bash
# codex-voice install script
# Installs codex-voice system-wide: checks dependencies, installs codex-asr,
# copies scripts to ~/.local/bin, and configures a GNOME custom shortcut.
set -euo pipefail

HOTKEY="${1:-<Control><Super>space}"
BINDING_NAME="Codex Voice Dictation"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
INSTALL_DIR="$HOME/.local/bin"
BINDING_PREFIX="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings"

echo "=== codex-voice installer ==="
echo ""

# --- Dependencies ---
echo "Checking dependencies..."

missing=()

command -v arecord >/dev/null 2>&1 || missing+=(arecord "sudo apt install alsa-utils")
command -v wl-copy >/dev/null 2>&1 || missing+=(wl-copy "sudo apt install wl-clipboard")
command -v ydotool >/dev/null 2>&1 || missing+=(ydotool "sudo apt install ydotool")
command -v python3 >/dev/null 2>&1 || missing+=(python3 "sudo apt install python3")
python3 -c "import gi; gi.require_version('Gtk','3.0')" 2>/dev/null || missing+=(python3-gi "sudo apt install python3-gi")
python3 -c "import gi; gi.require_foreign('cairo')" 2>/dev/null || missing+=(python3-gi-cairo "sudo apt install python3-gi-cairo")
command -v notify-send >/dev/null 2>&1 || missing+=(libnotify-bin "sudo apt install libnotify-bin")

if [ ${#missing[@]} -gt 0 ]; then
  echo "Missing dependencies:"
  for ((i=0; i<${#missing[@]}; i+=2)); do
    echo "  - ${missing[$i]}: install with: ${missing[$i+1]}"
  done
  echo ""
  echo "Install all with:"
  echo "  sudo apt install alsa-utils wl-clipboard ydotool python3-gi python3-gi-cairo libnotify-bin"
  exit 1
fi

# Check codex-asr
if ! command -v codex-asr >/dev/null 2>&1; then
  echo "codex-asr not found. Installing via official installer..."
  curl --proto '=https' --tlsv1.2 -LsSf \
    https://github.com/wangnov/codex-asr/releases/latest/download/codex-asr-installer.sh \
    | sh
  export PATH="$HOME/.cargo/bin:$PATH"
fi

if ! command -v codex-asr >/dev/null 2>&1; then
  echo "ERROR: codex-asr installation failed."
  exit 1
fi

echo "  all dependencies satisfied."
echo ""

# --- Check Codex auth ---
if [ ! -f "$HOME/.codex/auth.json" ]; then
  echo "WARNING: ~/.codex/auth.json not found."
  echo "  Run 'codex login' first to authenticate with your ChatGPT account."
  exit 1
fi
echo "Codex auth: OK"
echo ""

# --- Install scripts ---
echo "Installing scripts to $INSTALL_DIR..."
mkdir -p "$INSTALL_DIR"
cp "$SCRIPT_DIR/codex-voice" "$INSTALL_DIR/codex-voice"
chmod +x "$INSTALL_DIR/codex-voice"
echo "  $INSTALL_DIR/codex-voice"
echo ""

# --- Configure GNOME shortcut ---
echo "Configuring GNOME custom shortcut..."
echo "  Name:    $BINDING_NAME"
echo "  Hotkey:  $HOTKEY"

KEY_PATH="$BINDING_PREFIX/$slot/"

# Find a free custom-keybinding slot
existing=$(gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings)
slot=$(python3 -c "
import ast, sys
s = sys.argv[1].replace('@as []', '[]')
items = ast.literal_eval(s)
used = set(item.rstrip('/').split('/')[-1] for item in items)
for i in range(100):
    name = f'custom{i}'
    if name not in used:
        print(name)
        break
" "$existing")

KEY_PATH="$BINDING_PREFIX/$slot/"

NEW_LIST=$(python3 -c "
import ast, sys
s = sys.argv[1].replace('@as []', '[]')
items = ast.literal_eval(s)
items.append(sys.argv[2])
print(repr(items).replace(chr(39), chr(34)))
" "$existing" "$KEY_PATH")

gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "$NEW_LIST"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:"$KEY_PATH" name "$BINDING_NAME"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:"$KEY_PATH" command "$INSTALL_DIR/codex-voice"
gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:"$KEY_PATH" binding "$HOTKEY"

echo "  Slot:    $slot"
echo "  Path:    $KEY_PATH"
echo ""

# --- Ensure ydotoold is running ---
if ! systemctl --user is-active ydotool.service >/dev/null 2>&1; then
  echo "NOTE: ydotool service is not active."
  echo "  Enable it with: systemctl --user enable --now ydotool.service"
  echo ""
fi

echo "=== Installation complete ==="
echo ""
echo "Press $HOTKEY to toggle dictation:"
echo "  1. First press  → start recording (overlay pill appears)"
echo "  2. Second press → stop, transcribe, type at cursor"
echo ""
echo "Set CODEX_VOICE_LANG to change language hint (default: en):"
echo "  export CODEX_VOICE_LANG=es"
