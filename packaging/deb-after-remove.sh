#!/usr/bin/env bash
set -euo pipefail

# Do not remove shared runtime files while dpkg is upgrading to a newer
# package; the new postinst has already installed its replacements.
case "${1:-remove}" in
  remove|purge) ;;
  *) exit 0 ;;
esac

APP_ROOT="/opt/Codex Voice Settings"
EXTENSION_UUID="codex-voice@andy-spike.github.io"
SCHEMA_DIR="/usr/share/glib-2.0/schemas"

if command -v update-alternatives >/dev/null 2>&1; then
  update-alternatives --remove codex-voice-settings "$APP_ROOT/codex-voice-settings" || true
else
  rm -f /usr/bin/codex-voice-settings
fi
rm -f /usr/bin/codex-voice
rm -rf /usr/lib/codex-voice /usr/share/codex-voice
rm -rf "/usr/share/gnome-shell/extensions/$EXTENSION_UUID"
rm -f "$SCHEMA_DIR/io.github.andy_spike.CodexVoice.gschema.xml"
rm -f /usr/share/applications/codex-voice-settings.desktop
rm -f /usr/share/icons/hicolor/128x128/apps/codex-voice.png
rm -f /usr/share/icons/hicolor/scalable/apps/codex-voice.svg
rm -f /etc/xdg/autostart/io.github.andy_spike.CodexVoice.desktop
glib-compile-schemas "$SCHEMA_DIR" || true

if command -v apparmor_status >/dev/null 2>&1 && apparmor_status --enabled >/dev/null 2>&1; then
  if command -v apparmor_parser >/dev/null 2>&1; then
    apparmor_parser --remove /etc/apparmor.d/codex-voice-settings || true
  fi
fi
rm -f /etc/apparmor.d/codex-voice-settings
