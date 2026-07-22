#!/usr/bin/env bash
set -euo pipefail
umask 022

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [[ -z "${SOURCE_DATE_EPOCH:-}" ]]; then
  if ! SOURCE_DATE_EPOCH="$(git -C "$ROOT_DIR" show -s --format=%ct HEAD 2>/dev/null)" || [[ -z "$SOURCE_DATE_EPOCH" ]]; then
    echo "SOURCE_DATE_EPOCH is required when the release commit is unavailable" >&2
    exit 2
  fi
fi
[[ "$SOURCE_DATE_EPOCH" =~ ^[0-9]+$ ]] || { echo "SOURCE_DATE_EPOCH must be a non-negative integer" >&2; exit 2; }
export SOURCE_DATE_EPOCH
export LC_ALL=C
export TZ=UTC

ARCHITECTURE="$(dpkg --print-architecture)"
[[ "$ARCHITECTURE" == "amd64" ]] || { echo "codex-voice packages are only built for Debian amd64 (got $ARCHITECTURE)" >&2; exit 2; }
VERSION="$(cargo metadata --no-deps --format-version 1 --manifest-path "$ROOT_DIR/Cargo.toml" | python3 -c 'import json,sys; print(json.load(sys.stdin)["packages"][0]["version"])')"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT_DIR/target}"
STAGE="${STAGE:-$ROOT_DIR/tmp/deb-root}"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT_DIR/dist}"
PACKAGE="$OUTPUT_DIR/codex-voice-${VERSION}-x86_64.deb"

for source in \
  "$TARGET_DIR/release/codex-voice" \
  "$ROOT_DIR/packaging/resources/codex-asr" \
  "$ROOT_DIR/settings/codex_voice_settings.py" \
  "$ROOT_DIR/distribution/codex-voice-48.png" \
  "$ROOT_DIR/distribution/codex-voice-settings"; do
  [[ -f "$source" ]] || { echo "missing package input: $source" >&2; exit 2; }
done

rm -rf "$STAGE"
install -d "$STAGE/DEBIAN" \
  "$STAGE/usr/bin" \
  "$STAGE/usr/lib/codex-voice" \
  "$STAGE/usr/share/codex-voice" \
  "$STAGE/usr/share/doc/codex-voice" \
  "$STAGE/usr/share/glib-2.0/schemas" \
  "$STAGE/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io" \
  "$STAGE/usr/share/icons/hicolor/48x48/apps" \
  "$STAGE/usr/share/icons/hicolor/128x128/apps" \
  "$STAGE/usr/share/applications" \
  "$STAGE/etc/xdg/autostart" \
  "$STAGE/usr/lib/systemd/user/graphical-session.target.wants" \
  "$STAGE/usr/lib/udev/rules.d"

install -m755 "$TARGET_DIR/release/codex-voice" "$STAGE/usr/bin/codex-voice"
install -m755 "$ROOT_DIR/distribution/codex-voice-settings" "$STAGE/usr/bin/codex-voice-settings"
install -m755 "$ROOT_DIR/packaging/resources/codex-asr" "$STAGE/usr/lib/codex-voice/codex-asr"
install -m644 "$ROOT_DIR/settings/codex_voice_settings.py" "$STAGE/usr/lib/codex-voice/codex_voice_settings.py"
install -m755 "$ROOT_DIR/packaging/remove-legacy-shortcut.py" "$STAGE/usr/lib/codex-voice/remove-legacy-shortcut.py"
install -m755 "$ROOT_DIR/packaging/codex-voice-session-setup.sh" "$STAGE/usr/lib/codex-voice/codex-voice-session-setup.sh"
install -m644 "$ROOT_DIR/packaging/codex-voice-ydotoold.service" "$STAGE/usr/lib/systemd/user/codex-voice-ydotoold.service"
ln -s ../codex-voice-ydotoold.service "$STAGE/usr/lib/systemd/user/graphical-session.target.wants/codex-voice-ydotoold.service"
install -m644 "$ROOT_DIR/packaging/70-codex-voice-uinput.rules" "$STAGE/usr/lib/udev/rules.d/70-codex-voice-uinput.rules"
install -m755 "$ROOT_DIR/src/overlay.py" "$STAGE/usr/share/codex-voice/overlay.py"
install -m644 "$ROOT_DIR/schemas/io.github.andy_spike.CodexVoice.gschema.xml" "$STAGE/usr/share/glib-2.0/schemas/io.github.andy_spike.CodexVoice.gschema.xml"
install -m644 "$ROOT_DIR/distribution/codex-voice-48.png" "$STAGE/usr/share/icons/hicolor/48x48/apps/codex-voice.png"
install -m644 "$ROOT_DIR/distribution/codex-voice.png" "$STAGE/usr/share/icons/hicolor/128x128/apps/codex-voice.png"
install -m644 "$ROOT_DIR/distribution/io.github.andy_spike.CodexVoice.Settings.desktop" "$STAGE/usr/share/applications/io.github.andy_spike.CodexVoice.Settings.desktop"
install -m644 "$ROOT_DIR/distribution/io.github.andy_spike.CodexVoice.SessionSetup.desktop" "$STAGE/etc/xdg/autostart/io.github.andy_spike.CodexVoice.desktop"
install -m644 "$ROOT_DIR/debian/copyright" "$STAGE/usr/share/doc/codex-voice/copyright"
install -m644 "$ROOT_DIR/debian/rust-dependency-license-review.md" "$STAGE/usr/share/doc/codex-voice/rust-dependency-license-review.md"
gzip -9n -c "$ROOT_DIR/CHANGELOG.md" > "$STAGE/usr/share/doc/codex-voice/changelog.gz"
gzip -9n -c "$ROOT_DIR/debian/changelog" > "$STAGE/usr/share/doc/codex-voice/changelog.Debian.gz"
install -d "$STAGE/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/icons"
for source in extension.js compat.js protocol.js metadata.json stylesheet.css; do
  install -m644 "$ROOT_DIR/extension/$source" "$STAGE/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/$source"
done
install -m644 "$ROOT_DIR/extension/icons/codex-voice-panel.png" "$STAGE/usr/share/gnome-shell/extensions/codex-voice@andy-spike.github.io/icons/codex-voice-panel.png"

cat > "$STAGE/DEBIAN/control" <<EOF
Package: codex-voice
Version: $VERSION
Section: utils
Priority: optional
Architecture: amd64
Maintainer: codex-voice contributors <ansanabria@proton.me>
Homepage: https://github.com/ansanabria/codex-voice
Depends: libgtk-3-0t64, gir1.2-gtk-3.0, gir1.2-gtk-4.0, gir1.2-adw-1, python3, python3-gi, python3-cairo, libglib2.0-bin, alsa-utils, wl-clipboard, xclip, ydotool, ydotoold | ydotool (>= 1.0.0)
Recommends: gnome-shell
Description: Push-to-talk voice dictation for GNOME
EOF
install -m755 "$ROOT_DIR/packaging/deb-preinst.sh" "$STAGE/DEBIAN/preinst"
install -m755 "$ROOT_DIR/packaging/deb-postinst.sh" "$STAGE/DEBIAN/postinst"
install -m755 "$ROOT_DIR/packaging/deb-postrm.sh" "$STAGE/DEBIAN/postrm"
printf '%s\n' '/etc/xdg/autostart/io.github.andy_spike.CodexVoice.desktop' > "$STAGE/DEBIAN/conffiles"

(cd "$STAGE" && find . -type f ! -path './DEBIAN/*' -printf '%P\0' | sort -z | xargs -0 md5sum > DEBIAN/md5sums)

# dpkg-deb uses SOURCE_DATE_EPOCH for its ar headers. Normalize the staged
# tree as well so both tar members are independent of checkout and build time.
find "$STAGE" -depth -exec touch --no-dereference --date="@$SOURCE_DATE_EPOCH" {} +

install -d "$OUTPUT_DIR"
rm -f "$PACKAGE"
dpkg-deb \
  --root-owner-group \
  --uniform-compression \
  --compression=xz \
  --compression-level=9 \
  --threads-max=1 \
  --build "$STAGE" "$PACKAGE"
(cd "$OUTPUT_DIR" && sha256sum "$(basename "$PACKAGE")" > SHA256SUMS)
printf 'Built %s\n' "$PACKAGE"
printf 'Wrote %s\n' "$OUTPUT_DIR/SHA256SUMS"
