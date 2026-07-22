#!/usr/bin/env bash
set -euo pipefail

DEB="${1:?usage: $0 dist/codex-voice-version-x86_64.deb}"
[[ -f "$DEB" ]] || { echo "package not found: $DEB" >&2; exit 2; }
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT_DIR/packaging/codex-asr-release.sh"
mkdir -p "$ROOT_DIR/tmp"
EXTRACT="$(mktemp -d "$ROOT_DIR/tmp/inspect-deb.XXXXXX")"
trap 'rm -rf "$EXTRACT"' EXIT
dpkg-deb --extract "$DEB" "$EXTRACT/root"
dpkg-deb --control "$DEB" "$EXTRACT/control"

control="$(dpkg-deb --field "$DEB")"
contents="$(dpkg-deb --contents "$DEB")"
scripts="$(dpkg-deb --ctrl-tarfile "$DEB" | tar -tf -)"
package_version="$(dpkg-deb --field "$DEB" Version)"
icon_dimensions() {
  local path="$1"
  { dpkg-deb --fsys-tarfile "$DEB" | tar -xOf - "$path"; } | python3 -c 'import sys; data = sys.stdin.buffer.read(24); print(f"{int.from_bytes(data[16:20])}x{int.from_bytes(data[20:24])}")'
}
require_mode() {
  local expected="$1"
  shift
  local path mode
  for path in "$@"; do
    [[ -e "$EXTRACT/$path" ]] || { echo "missing package path: /$path" >&2; exit 1; }
    mode="$(stat -c '%a' "$EXTRACT/$path")"
    [[ "$mode" == "$expected" ]] || {
      echo "invalid mode for /$path: expected $expected, got $mode" >&2
      exit 1
    }
  done
}

expected_conffile='/etc/xdg/autostart/io.github.andy_spike.CodexVoice.desktop'
[[ "$(cat "$EXTRACT/control/conffiles")" == "$expected_conffile" ]] || {
  echo "invalid conffiles metadata" >&2
  exit 1
}
[[ -s "$EXTRACT/control/md5sums" ]] || { echo "missing md5sums metadata" >&2; exit 1; }
(cd "$EXTRACT/root" && md5sum --strict --check "$EXTRACT/control/md5sums")
expected_md5_paths="$(cd "$EXTRACT/root" && find . -type f -printf '%P\n' | sort)"
actual_md5_paths="$(sed -nE 's/^[0-9a-f]{32}  //p' "$EXTRACT/control/md5sums" | sort)"
[[ "$actual_md5_paths" == "$expected_md5_paths" ]] || {
  echo "md5sums does not cover exactly all regular package files" >&2
  exit 1
}

cmp "$ROOT_DIR/debian/copyright" "$EXTRACT/root/usr/share/doc/codex-voice/copyright"
cmp "$ROOT_DIR/debian/rust-dependency-license-review.md" "$EXTRACT/root/usr/share/doc/codex-voice/rust-dependency-license-review.md"
gzip --test "$EXTRACT/root/usr/share/doc/codex-voice/changelog.gz"
gzip --test "$EXTRACT/root/usr/share/doc/codex-voice/changelog.Debian.gz"
cmp "$ROOT_DIR/CHANGELOG.md" <(gzip -dc "$EXTRACT/root/usr/share/doc/codex-voice/changelog.gz")
cmp "$ROOT_DIR/debian/changelog" <(gzip -dc "$EXTRACT/root/usr/share/doc/codex-voice/changelog.Debian.gz")
grep -q '^Copyright: 2026 codex-asr contributors$' "$EXTRACT/root/usr/share/doc/codex-voice/copyright"
grep -q '^License: public-domain$' "$EXTRACT/root/usr/share/doc/codex-voice/copyright"
grep -q '^License: CDLA-Permissive-2.0$' "$EXTRACT/root/usr/share/doc/codex-voice/copyright"
grep -q '^## 0.2.0 - 2026-07-21$' <(gzip -dc "$EXTRACT/root/usr/share/doc/codex-voice/changelog.gz")
grep -q '^codex-voice (0.2.0) unstable; urgency=medium$' <(gzip -dc "$EXTRACT/root/usr/share/doc/codex-voice/changelog.Debian.gz")
grep -q '^`rusqlite 0.40.1` enables its `bundled` feature' "$EXTRACT/root/usr/share/doc/codex-voice/rust-dependency-license-review.md"
grep -q '^`codex-asr 0.1.2` is not part of the Codex Voice Cargo graph' "$EXTRACT/root/usr/share/doc/codex-voice/rust-dependency-license-review.md"

dpkg --compare-versions "$package_version" gt 0.1.0 || {
  echo "package version must be newer than 0.1.0 (got $package_version)" >&2
  exit 1
}
[[ "$package_version" == "0.2.0" ]] || {
  echo "release artifact must have version 0.2.0 (got $package_version)" >&2
  exit 1
}
binary_version="$("$EXTRACT/root/usr/bin/codex-voice" --version)"
[[ "$binary_version" == "$package_version" ]] || {
  echo "package version $package_version does not match bundled binary $binary_version" >&2
  exit 1
}
codex_asr="$EXTRACT/root/usr/lib/codex-voice/codex-asr"
codex_asr_digest="$(sha256sum "$codex_asr" | awk '{print $1}')"
[[ "$codex_asr_digest" == "$PINNED_CODEX_ASR_BINARY_SHA256" ]] || {
  echo "bundled codex-asr digest mismatch: expected $PINNED_CODEX_ASR_BINARY_SHA256, got $codex_asr_digest" >&2
  exit 1
}
codex_asr_version="$("$codex_asr" --version)"
[[ "$codex_asr_version" == "codex-asr $PINNED_CODEX_ASR_VERSION" ]] || {
  echo "bundled codex-asr version mismatch: expected codex-asr $PINNED_CODEX_ASR_VERSION, got $codex_asr_version" >&2
  exit 1
}

for field in \
  'Package: codex-voice' \
  'Version: 0.2.0' \
  'Architecture: amd64' \
  'Description: Push-to-talk voice dictation for GNOME'; do
  grep -qx "$field" <<<"$control"
done
grep -q '^Depends:.*libgtk-3-0t64' <<<"$control"
grep -q '^Depends:.*gir1.2-adw-1' <<<"$control"
grep -q '^Depends:.*ydotoold | ydotool (>= 1.0.0)' <<<"$control"
for path in \
  './usr/bin/codex-voice' \
  './usr/bin/codex-voice-settings' \
  './usr/lib/codex-voice/codex-asr' \
  './usr/lib/codex-voice/codex_voice_settings.py' \
  './usr/lib/codex-voice/codex-voice-session-setup.sh' \
  './usr/lib/codex-voice/remove-legacy-shortcut.py' \
  './usr/lib/systemd/user/codex-voice-ydotoold.service' \
  './usr/lib/udev/rules.d/70-codex-voice-uinput.rules' \
  './usr/share/codex-voice/overlay.py' \
  './usr/share/doc/codex-voice/copyright' \
  './usr/share/doc/codex-voice/changelog.gz' \
  './usr/share/doc/codex-voice/changelog.Debian.gz' \
  './usr/share/doc/codex-voice/rust-dependency-license-review.md' \
  './usr/share/glib-2.0/schemas/io.github.andy_spike.CodexVoice.gschema.xml' \
  './usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/metadata.json' \
  './usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/extension.js' \
  './usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/compat.js' \
  './usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/protocol.js' \
  './usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/stylesheet.css' \
  './usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/icons/codex-voice-panel.png' \
  './usr/share/icons/hicolor/48x48/apps/codex-voice.png' \
  './usr/share/icons/hicolor/128x128/apps/codex-voice.png' \
  './usr/share/applications/io.github.andy_spike.CodexVoice.Settings.desktop' \
  './etc/xdg/autostart/io.github.andy_spike.CodexVoice.desktop'; do
  grep -q " $path$" <<<"$contents"
done
grep -q ' ./usr/lib/systemd/user/graphical-session.target.wants/codex-voice-ydotoold.service -> ../codex-voice-ydotoold.service$' <<<"$contents"
for script in ./preinst ./postinst ./postrm ./conffiles ./md5sums; do
  grep -qx "$script" <<<"$scripts"
done
for absent in \
  '/opt/Codex Voice Settings' \
  '/usr/share/applications/codex-voice-settings.desktop' \
  '/usr/lib/codex-voice/gnome-custom-shortcuts.py' \
  '/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/protocol.test.js' \
  'electron' \
  'apparmor-profile'; do
  ! grep -Fq "$absent" <<<"$contents"
done
for size in 48 128; do
  dimensions="$(icon_dimensions "./usr/share/icons/hicolor/${size}x${size}/apps/codex-voice.png")"
  [[ "$dimensions" == "${size}x${size}" ]] || {
    echo "invalid app icon dimensions: expected ${size}x${size}, got $dimensions" >&2
    exit 1
  }
done

require_mode 755 \
  root/usr/bin/codex-voice \
  root/usr/bin/codex-voice-settings \
  root/usr/lib/codex-voice/codex-asr \
  root/usr/lib/codex-voice/codex-voice-session-setup.sh \
  root/usr/lib/codex-voice/remove-legacy-shortcut.py \
  root/usr/share/codex-voice/overlay.py \
  root/usr/share/glib-2.0/schemas \
  root/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io \
  root/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/icons \
  control/preinst \
  control/postinst \
  control/postrm
require_mode 644 \
  root/usr/lib/systemd/user/codex-voice-ydotoold.service \
  root/usr/lib/udev/rules.d/70-codex-voice-uinput.rules \
  root/usr/lib/codex-voice/codex_voice_settings.py \
  root/usr/share/doc/codex-voice/copyright \
  root/usr/share/doc/codex-voice/changelog.gz \
  root/usr/share/doc/codex-voice/changelog.Debian.gz \
  root/usr/share/doc/codex-voice/rust-dependency-license-review.md \
  root/usr/share/glib-2.0/schemas/io.github.andy_spike.CodexVoice.gschema.xml \
  root/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/metadata.json \
  root/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/extension.js \
  root/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/compat.js \
  root/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/protocol.js \
  root/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/stylesheet.css \
  root/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/icons/codex-voice-panel.png \
  root/usr/share/applications/io.github.andy_spike.CodexVoice.Settings.desktop \
  root/etc/xdg/autostart/io.github.andy_spike.CodexVoice.desktop \
  control/conffiles \
  control/md5sums

glib-compile-schemas --strict "$EXTRACT/root/usr/share/glib-2.0/schemas"
if command -v desktop-file-validate >/dev/null 2>&1; then
  desktop-file-validate \
    "$EXTRACT/root/usr/share/applications/io.github.andy_spike.CodexVoice.Settings.desktop" \
    "$EXTRACT/root/etc/xdg/autostart/io.github.andy_spike.CodexVoice.desktop"
fi
python3 - \
  "$EXTRACT/root/etc/xdg/autostart/io.github.andy_spike.CodexVoice.desktop" \
  "$EXTRACT/root/usr/lib/codex-voice/codex-voice-session-setup.sh" <<'PY'
import configparser
import os
import pathlib
import sys

desktop_path = pathlib.Path(sys.argv[1])
session_setup = pathlib.Path(sys.argv[2])
parser = configparser.ConfigParser(interpolation=None)
parser.optionxform = str
with desktop_path.open(encoding="utf-8") as desktop_file:
    parser.read_file(desktop_file)
entry = parser["Desktop Entry"]
expected = "/usr/lib/codex-voice/codex-voice-session-setup.sh"
assert entry["TryExec"] == expected
assert entry["Exec"] == expected
assert session_setup.is_file() and os.access(session_setup, os.X_OK)
PY
python3 - "$EXTRACT/root/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/metadata.json" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as metadata_file:
    metadata = json.load(metadata_file)
assert metadata["uuid"] == "codex-voice@andy-spike.github.io"
assert metadata["settings-schema"] == "io.github.andy_spike.CodexVoice"
assert metadata["shell-version"] == ["46", "50"]
PY
bash -n \
  "$EXTRACT/root/usr/lib/codex-voice/codex-voice-session-setup.sh" \
  "$EXTRACT/control/preinst" \
  "$EXTRACT/control/postinst" \
  "$EXTRACT/control/postrm"
systemd-analyze verify "$EXTRACT/root/usr/lib/systemd/user/codex-voice-ydotoold.service"
python3 -m py_compile \
  "$EXTRACT/root/usr/lib/codex-voice/codex_voice_settings.py" \
  "$EXTRACT/root/usr/lib/codex-voice/remove-legacy-shortcut.py"
for source in extension.js compat.js protocol.js; do
  node --check "$EXTRACT/root/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/$source"
done
echo "Debian package inspection passed: $DEB"
