#!/usr/bin/env bash
set -euo pipefail

APP_ROOT="/opt/Codex Voice Settings"
RESOURCE_ROOT="$APP_ROOT/resources/codex-voice"
SYSTEM_LIB="/usr/lib/codex-voice"
SYSTEM_SHARE="/usr/share/codex-voice"
EXTENSION_UUID="codex-voice@andy-spike.github.io"
EXTENSION_DIR="/usr/share/gnome-shell/extensions/$EXTENSION_UUID"
SCHEMA_DIR="/usr/share/glib-2.0/schemas"

install -Dm755 "$RESOURCE_ROOT/codex-voice" /usr/bin/codex-voice
install -Dm755 "$RESOURCE_ROOT/codex-asr" "$SYSTEM_LIB/codex-asr"
install -Dm755 "$RESOURCE_ROOT/overlay.py" "$SYSTEM_SHARE/overlay.py"
install -Dm755 "$RESOURCE_ROOT/gnome-custom-shortcuts.py" "$SYSTEM_LIB/gnome-custom-shortcuts.py"
install -Dm755 "$RESOURCE_ROOT/codex-voice-session-setup.sh" "$SYSTEM_LIB/codex-voice-session-setup.sh"
install -Dm644 "$RESOURCE_ROOT/io.github.andy_spike.CodexVoice.gschema.xml" "$SCHEMA_DIR/io.github.andy_spike.CodexVoice.gschema.xml"
install -Dm644 "$RESOURCE_ROOT/io.github.andy_spike.CodexVoice.desktop" /usr/share/applications/codex-voice-settings.desktop
install -Dm644 "$RESOURCE_ROOT/codex-voice.png" /usr/share/icons/hicolor/128x128/apps/codex-voice.png
rm -f /usr/share/icons/hicolor/scalable/apps/codex-voice.svg

install -d "$EXTENSION_DIR"
# Keep the directory identity stable during upgrades. Removing it makes a
# running Wayland Shell forget the extension, and it will not rescan system
# extension directories until the next login.
cp -a "$RESOURCE_ROOT/extension/." "$EXTENSION_DIR/"

glib-compile-schemas "$SCHEMA_DIR"

if command -v update-alternatives >/dev/null 2>&1; then
  update-alternatives --install /usr/bin/codex-voice-settings codex-voice-settings "$APP_ROOT/codex-voice-settings" 100
else
  ln -sf "$APP_ROOT/codex-voice-settings" /usr/bin/codex-voice-settings
fi

if command -v update-mime-database >/dev/null 2>&1; then
  update-mime-database /usr/share/mime || true
fi
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t /usr/share/icons/hicolor || true
fi

# Preserve Electron's sandbox support when a host uses AppArmor.
if command -v apparmor_status >/dev/null 2>&1 && apparmor_status --enabled >/dev/null 2>&1; then
  profile_source="$APP_ROOT/resources/apparmor-profile"
  profile_target=/etc/apparmor.d/codex-voice-settings
  if command -v apparmor_parser >/dev/null 2>&1 && apparmor_parser --skip-kernel-load "$profile_source" >/dev/null 2>&1; then
    install -Dm644 "$profile_source" "$profile_target"
    if ! { command -v ischroot >/dev/null 2>&1 && ischroot; }; then
      apparmor_parser --replace --write-cache --skip-read-cache "$profile_target" || true
    fi
  fi
fi

install -Dm644 /dev/null /etc/xdg/autostart/io.github.andy_spike.CodexVoice.desktop
cat > /etc/xdg/autostart/io.github.andy_spike.CodexVoice.desktop <<'EOF'
[Desktop Entry]
Type=Application
Name=Codex Voice session setup
Comment=Enable Codex Voice controls for this desktop session
Exec=/usr/lib/codex-voice/codex-voice-session-setup.sh
OnlyShowIn=GNOME;
NoDisplay=true
X-GNOME-Autostart-enabled=true
EOF

# If apt was launched through sudo from an active desktop session, configure
# that user immediately as well as on the next login. The runuser call is
# best-effort because package installs can also happen from a TTY or chroot.
TARGET_USER="${SUDO_USER:-}"
if [[ -n "$TARGET_USER" && "$TARGET_USER" != root ]] && command -v runuser >/dev/null 2>&1; then
  TARGET_UID="$(id -u "$TARGET_USER" 2>/dev/null || true)"
  TARGET_HOME="$(getent passwd "$TARGET_USER" | cut -d: -f6)"
  if [[ -n "$TARGET_UID" && -n "$TARGET_HOME" ]]; then
    target_environment=("HOME=$TARGET_HOME" "XDG_RUNTIME_DIR=/run/user/$TARGET_UID")
    if [[ -S "/run/user/$TARGET_UID/bus" ]]; then
      target_environment+=("DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$TARGET_UID/bus")
    fi
    runuser -u "$TARGET_USER" -- env "${target_environment[@]}" "$SYSTEM_LIB/codex-voice-session-setup.sh" || true
  fi
fi
