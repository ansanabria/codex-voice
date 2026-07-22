#!/usr/bin/env bash
set -euo pipefail

case "${1:-remove}" in
  remove|purge) ;;
  *) exit 0 ;;
esac

if command -v glib-compile-schemas >/dev/null 2>&1; then
  glib-compile-schemas /usr/share/glib-2.0/schemas || true
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
