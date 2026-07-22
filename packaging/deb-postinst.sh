#!/usr/bin/env bash
set -euo pipefail

case "${1:-configure}" in
  configure) ;;
  *) exit 0 ;;
esac

SCHEMA_DIR="/usr/share/glib-2.0/schemas"
SYSTEM_LIB="/usr/lib/codex-voice"

if command -v glib-compile-schemas >/dev/null 2>&1; then
  glib-compile-schemas --strict "$SCHEMA_DIR"
fi
if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t /usr/share/icons/hicolor || true
fi
if command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules || true
  udevadm trigger --action=change --name-match=/dev/uinput --settle || true
fi

# If apt runs through sudo in an active GNOME session, configure that account
# now as well as at the next login. TTY and chroot installs remain valid.
target_user="${SUDO_USER:-}"
if [[ -n "$target_user" && "$target_user" != root ]] && command -v runuser >/dev/null 2>&1; then
  target_uid="$(id -u "$target_user" 2>/dev/null || true)"
  target_home="$(getent passwd "$target_user" | cut -d: -f6)"
  if [[ -n "$target_uid" && -n "$target_home" ]]; then
    target_environment=("HOME=$target_home" "XDG_RUNTIME_DIR=/run/user/$target_uid")
    if [[ -S "/run/user/$target_uid/bus" ]]; then
      target_environment+=("DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$target_uid/bus")
    fi
    runuser -u "$target_user" -- env "${target_environment[@]}" "$SYSTEM_LIB/codex-voice-session-setup.sh" || true
    runuser -u "$target_user" -- env "${target_environment[@]}" systemctl --user daemon-reload >/dev/null 2>&1 || true
    if ! runuser -u "$target_user" -- env "${target_environment[@]}" systemctl --user restart ydotool.service >/dev/null 2>&1; then
      runuser -u "$target_user" -- env "${target_environment[@]}" systemctl --user restart codex-voice-ydotoold.service >/dev/null 2>&1 || true
    fi
  fi
fi
