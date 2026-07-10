#!/usr/bin/env bash
# Install codex-voice for the Ubuntu 24.04/26.04 LTS support contract.
set -euo pipefail

HOTKEY="${1:-<Control><Super>space}"
GITHUB_REPOSITORY="${CODEX_VOICE_GITHUB_REPOSITORY:-ansanabria/codex-voice}"
SCRIPT_SOURCE="${BASH_SOURCE[0]:-}"
if [[ -n "$SCRIPT_SOURCE" && "$SCRIPT_SOURCE" != /dev/stdin && "$SCRIPT_SOURCE" != /dev/fd/* ]]; then
  ROOT_DIR="$(cd "$(dirname "$SCRIPT_SOURCE")" && pwd)"
else
  ROOT_DIR="$PWD"
fi
TEMP_ROOT=""
# `curl | bash` has no checkout beside the script. Fetch a source snapshot for
# the small Rust/GTK/extension pieces; the Electron application itself remains
# a prebuilt AppImage and does not require Node on the target machine.
if [[ ! -f "$ROOT_DIR/Cargo.toml" ]]; then
  TEMP_ROOT="$(mktemp -d)"
  trap 'rm -rf "$TEMP_ROOT"' EXIT
  SOURCE_REF="${CODEX_VOICE_SOURCE_REF:-master}"
  curl --proto '=https' --tlsv1.2 -LsSf "https://github.com/$GITHUB_REPOSITORY/archive/refs/heads/$SOURCE_REF.tar.gz" |
    tar -xz --strip-components=1 -C "$TEMP_ROOT"
  ROOT_DIR="$TEMP_ROOT"
fi
INSTALL_BIN="$HOME/.local/bin"
DATA_DIR="$HOME/.local/share/codex-voice"
SCHEMA_DIR="$DATA_DIR/schemas"
EXTENSION_UUID="codex-voice@andy-spike.github.io"
EXTENSION_DIR="$HOME/.local/share/gnome-shell/extensions/$EXTENSION_UUID"
VERSION="$(sed -n 's/^version = "\([^"]*\)"/\1/p' "$ROOT_DIR/Cargo.toml" | head -n1)"
APPIMAGE="$DATA_DIR/codex-voice-settings.AppImage"
APPIMAGE_URL="${CODEX_VOICE_SETTINGS_APPIMAGE_URL:-https://github.com/$GITHUB_REPOSITORY/releases/download/v$VERSION/codex-voice-settings-$VERSION-x86_64.AppImage}"
APPIMAGE_SHA_URL="${CODEX_VOICE_SETTINGS_SHA256_URL:-$APPIMAGE_URL.sha256}"
LOCAL_APPIMAGE="${CODEX_VOICE_SETTINGS_APPIMAGE:-}"

if [[ -z "$LOCAL_APPIMAGE" ]]; then
  for candidate in \
    "$ROOT_DIR/settings/dist/codex-voice-settings-$VERSION-x86_64.AppImage" \
    "$ROOT_DIR/settings/dist/Codex Voice Settings-$VERSION.AppImage" \
    "$ROOT_DIR/settings/release/codex-voice-settings-$VERSION-x86_64.AppImage"; do
    if [[ -f "$candidate" ]]; then
      LOCAL_APPIMAGE="$candidate"
      break
    fi
  done
fi

source /etc/os-release 2>/dev/null || true
UBUNTU_VERSION="${VERSION_ID:-unknown}"
GNOME_VERSION="$(gnome-shell --version 2>/dev/null | sed -n 's/.* \([0-9][0-9]*\)\..*/\1/p')"
SUPPORTED_UBUNTU=false
SUPPORTED_SHELL=false
[[ "${ID:-}" == ubuntu && ( "$UBUNTU_VERSION" == 24.04 || "$UBUNTU_VERSION" == 26.04 ) ]] && SUPPORTED_UBUNTU=true
[[ "$GNOME_VERSION" == 46 || "$GNOME_VERSION" == 50 ]] && SUPPORTED_SHELL=true

echo "codex-voice $VERSION"
echo "Detected Ubuntu $UBUNTU_VERSION; GNOME Shell ${GNOME_VERSION:-unknown}."

INSTALL_EXTENSION=false
if "$SUPPORTED_UBUNTU" && "$SUPPORTED_SHELL"; then
  INSTALL_EXTENSION=true
elif "$SUPPORTED_UBUNTU"; then
  echo "WARNING: Ubuntu is supported, but GNOME Shell ${GNOME_VERSION:-unknown} is not in the supported 46/50 contract."
  if [[ "${CODEX_VOICE_EXTENSION_OVERRIDE:-}" == 1 ]]; then
    echo "Proceeding with extension installation because CODEX_VOICE_EXTENSION_OVERRIDE=1 was supplied."
    INSTALL_EXTENSION=true
  else
    echo "Skipping extension. Re-run with CODEX_VOICE_EXTENSION_OVERRIDE=1 to explicitly override this warning."
  fi
else
  echo "WARNING: this distribution/version is outside the supported Ubuntu 24.04/26.04 contract. CLI installation will continue; extension compatibility is not claimed."
fi

missing=()
command -v arecord >/dev/null 2>&1 || missing+=(alsa-utils)
command -v wl-copy >/dev/null 2>&1 || missing+=(wl-clipboard)
command -v ydotool >/dev/null 2>&1 || missing+=(ydotool)
command -v cargo >/dev/null 2>&1 || missing+=(cargo)
command -v glib-compile-schemas >/dev/null 2>&1 || missing+=(libglib2.0-bin)
python3 -c "import gi; gi.require_version('Gtk','3.0')" 2>/dev/null || missing+=(python3-gi)
python3 -c "import gi; gi.require_foreign('cairo')" 2>/dev/null || missing+=(python3-gi-cairo)
if ((${#missing[@]})); then
  command -v apt-get >/dev/null 2>&1 || { echo "Missing dependencies: ${missing[*]}" >&2; exit 1; }
  sudo apt-get update
  sudo apt-get install -y "${missing[@]}"
fi

if ! command -v codex-asr >/dev/null 2>&1; then
  echo "Installing codex-asr…"
  curl --proto '=https' --tlsv1.2 -LsSf https://github.com/wangnov/codex-asr/releases/latest/download/codex-asr-installer.sh | sh
  export PATH="$HOME/.cargo/bin:$PATH"
fi
command -v codex-asr >/dev/null 2>&1 || { echo "codex-asr installation failed" >&2; exit 1; }

cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"
mkdir -p "$INSTALL_BIN" "$DATA_DIR" "$SCHEMA_DIR"
install -m 0755 "$ROOT_DIR/target/release/codex-voice" "$INSTALL_BIN/codex-voice"
install -m 0755 "$ROOT_DIR/src/overlay.py" "$DATA_DIR/overlay.py"
install -m 0644 "$ROOT_DIR/schemas/io.github.andy_spike.CodexVoice.gschema.xml" "$SCHEMA_DIR/"
glib-compile-schemas "$SCHEMA_DIR"
# The extension consumes this same keybinding. A local install argument also
# migrates the old custom-shortcut choice before the legacy entry is removed.
GSETTINGS_SCHEMA_DIR="$SCHEMA_DIR${GSETTINGS_SCHEMA_DIR:+:$GSETTINGS_SCHEMA_DIR}" \
  gsettings set io.github.andy_spike.CodexVoice keybinding "['$HOTKEY']"

install_settings_appimage() {
  local temp checksum expected actual
  if [[ -n "$LOCAL_APPIMAGE" ]]; then
    [[ -f "$LOCAL_APPIMAGE" ]] || { echo "ERROR: AppImage not found: $LOCAL_APPIMAGE" >&2; exit 1; }
    install -m 0755 "$LOCAL_APPIMAGE" "$APPIMAGE"
    echo "Installed local settings AppImage: $LOCAL_APPIMAGE"
    return
  fi

  echo "Downloading settings AppImage $VERSION…"
  temp="$(mktemp "$DATA_DIR/.settings.XXXXXX")"
  checksum="$(mktemp "$DATA_DIR/.settings.sha.XXXXXX")"
  trap 'rm -f "$temp" "$checksum"' RETURN
  curl --proto '=https' --tlsv1.2 -fL "$APPIMAGE_URL" -o "$temp"
  curl --proto '=https' --tlsv1.2 -fL "$APPIMAGE_SHA_URL" -o "$checksum"
  expected="$(awk 'NR==1 {print $1}' "$checksum")"
  actual="$(sha256sum "$temp" | awk '{print $1}')"
  [[ "$expected" =~ ^[a-fA-F0-9]{64}$ && "$actual" == "$expected" ]] || { echo "ERROR: settings AppImage checksum verification failed" >&2; exit 1; }
  install -m 0755 "$temp" "$APPIMAGE"
}

# A curl-pipe installation downloads this pinned AppImage; it never installs Node or builds Electron.
install_settings_appimage
install -m 0755 "$ROOT_DIR/distribution/codex-voice-settings" "$INSTALL_BIN/codex-voice-settings"

install -Dm644 "$ROOT_DIR/distribution/io.github.andy_spike.CodexVoice.desktop" "$HOME/.local/share/applications/io.github.andy_spike.CodexVoice.desktop"
install -Dm644 "$ROOT_DIR/distribution/codex-voice.svg" "$HOME/.local/share/icons/hicolor/scalable/apps/codex-voice.svg"

legacy_shortcut() {
  local prefix="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings" existing slot keypath list
  existing="$(gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings)"
  slot="codex-voice"
  keypath="$prefix/$slot/"
  list="$(python3 -c 'import ast,sys; old=ast.literal_eval(sys.argv[1].replace("@as []", "[]")); p=sys.argv[2]; print(repr(old if p in old else old+[p]).replace("\x27", "\x22"))' "$existing" "$keypath")"
  gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "$list"
  gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:"$keypath" name "Codex Voice Dictation"
  gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:"$keypath" command "$INSTALL_BIN/codex-voice"
  gsettings set org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:"$keypath" binding "$HOTKEY"
}

remove_legacy_shortcut() {
  local prefix="/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings" existing keypath
  keypath="$prefix/codex-voice/"
  existing="$(gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings)"
  gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "$(python3 -c 'import ast,sys; print(repr([x for x in ast.literal_eval(sys.argv[1].replace("@as []", "[]")) if x != sys.argv[2]]).replace("\x27", "\x22"))' "$existing" "$keypath")"
}

if "$INSTALL_EXTENSION"; then
  rm -rf "$EXTENSION_DIR"
  mkdir -p "$EXTENSION_DIR"
  cp -a "$ROOT_DIR/extension/." "$EXTENSION_DIR/"
  if gnome-extensions enable "$EXTENSION_UUID"; then
    remove_legacy_shortcut
    echo "GNOME Shell extension enabled."
  else
    echo "WARNING: extension enablement failed; retaining the legacy shortcut." >&2
    legacy_shortcut
  fi
else
  legacy_shortcut
fi

echo "Installed CLI, settings AppImage, shared schema, and GTK fallback."
echo "  CLI:      $INSTALL_BIN/codex-voice"
echo "  Settings: $INSTALL_BIN/codex-voice-settings"
echo "Language defaults to automatic detection; set CODEX_VOICE_LANG=es (or another hint) to override it."
