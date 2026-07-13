#!/usr/bin/env bash
set -euo pipefail

DEB="${1:?usage: $0 dist/codex-voice-version-x86_64.deb}"
[[ -f "$DEB" ]] || { echo "package not found: $DEB" >&2; exit 2; }
control="$(dpkg-deb --field "$DEB")"
contents="$(dpkg-deb --contents "$DEB")"
scripts="$(dpkg-deb --ctrl-tarfile "$DEB" | tar -tf -)"

grep -qx 'Package: codex-voice' <<<"$control"
grep -q '^Depends:.*libgtk-3-0' <<<"$control"
grep -q '/resources/codex-voice/codex-voice$' <<<"$contents"
grep -q '/opt/Codex Voice Settings/codex-voice-settings$' <<<"$contents"
grep -q 'io.github.andy_spike.CodexVoice.desktop$' <<<"$contents"
grep -q 'io.github.andy_spike.CodexVoice.gschema.xml$' <<<"$contents"
grep -q '/extension/metadata.json$' <<<"$contents"
grep -q 'apparmor-profile$' <<<"$contents"
grep -qx './postinst' <<<"$scripts"
grep -qx './postrm' <<<"$scripts"
echo "Debian package inspection passed: $DEB"
