# Release

## Release checks

Run this complete checklist from the repository root after all intended release changes are committed. `CODEX_VOICE_TEST_BIN` is required below so `CliIntegrationTests` exercises the real release binary instead of being skipped.

```bash
set -euo pipefail

test -z "$(git status --porcelain=v1 --untracked-files=all)"

bash -n build.sh packaging/*.sh scripts/*.sh
python3 -m py_compile packaging/*.py settings/*.py
for source in extension/*.js; do node --check "$source"; done
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings

cargo test --locked --all-targets --all-features
python3 -m unittest \
  packaging/test_deb_lifecycle.py \
  packaging/test_deb_preinst.py \
  packaging/test_remove_legacy_shortcut.py \
  packaging/test_session_setup.py
node --test extension/protocol.test.js

./build.sh
CODEX_VOICE_TEST_BIN="$PWD/target/release/codex-voice" \
  python3 -m unittest settings/test_codex_voice_settings.py

DEB=dist/codex-voice-0.2.0-x86_64.deb
packaging/inspect-deb.sh "$DEB"
packaging/test_codex_asr_integrity.sh "$DEB"
(cd dist && sha256sum --check SHA256SUMS)
packaging/test_reproducible_build.sh
```

The integrity test downloads the manifest-pinned codex-asr release archive over HTTPS, verifies its pinned SHA-256 digest, replaces an isolated tampered binary, and confirms that package inspection rejects an isolated tampered package specifically for its codex-asr SHA-256 digest.

Before the release changes are committed, `./build.sh` and `packaging/test_reproducible_build.sh` are expected to fail with `release build requires a clean committed worktree` or `reproducibility check requires a clean committed release tree`. This is the safety gate working; do not bypass it with a dirty-tree archive or an ignored cached ASR binary. After the release commit, `git status --short` must print nothing and the complete checklist above must pass.

## Reproducible artifacts

Build from the clean release commit with `./build.sh`. In a Git checkout, the release build rejects staged changes, working-tree changes, and non-ignored untracked files. It derives `SOURCE_DATE_EPOCH` from `HEAD`, cleans and rebuilds the release profile, normalizes the package metadata to that timestamp, and writes `dist/SHA256SUMS` for the `.deb`. When building an exported source tree without `.git`, set the timestamp from the release commit explicitly: `SOURCE_DATE_EPOCH="$(git show -s --format=%ct <release-commit>)" ./build.sh`.

Run `packaging/test_reproducible_build.sh` after all release changes are committed. The check refuses a dirty tree, rejects a release commit that tracks `packaging/resources/codex-asr`, exports tracked content directly from `HEAD`, and performs two clean release builds in separate source, target, and staging directories. Ignored files, including a cached `packaging/resources/codex-asr`, are not copied into either source export; each build acquires the pinned binary through `packaging/fetch-codex-asr.sh`, which verifies the pinned archive and binary SHA-256 digests. The check verifies each checksum manifest and requires the resulting `.deb` files to be byte-identical.

Package inspection also verifies the Debian copyright and changelog files, the
Rust dependency/license review (including bundled SQLite), the codex-asr notice,
the autostart conffile declaration, and complete `DEBIAN/md5sums` coverage.

### Maintainers: create and publish once

Run these commands once, only after the complete release checklist above succeeds for the clean release commit. `git tag -s` creates an annotated, GPG-signed tag; do not replace an existing `v0.2.0` tag.

```bash
set -euo pipefail

test -z "$(git status --porcelain=v1 --untracked-files=all)"
test -z "$(git tag --list v0.2.0)"
RELEASE_COMMIT="$(git rev-parse --verify 'HEAD^{commit}')"
RELEASE_KEY='<release-key-fingerprint>' # replace with the full fingerprint

git tag -s -u "$RELEASE_KEY" -m "Codex Voice 0.2.0" v0.2.0 "$RELEASE_COMMIT"
git tag --verify v0.2.0
test "$(git rev-parse --verify 'v0.2.0^{commit}')" = "$RELEASE_COMMIT"

./build.sh
(cd dist && sha256sum --check SHA256SUMS)
gpg --local-user "$RELEASE_KEY" --armor --detach-sign \
  --output dist/SHA256SUMS.asc dist/SHA256SUMS
gpg --verify dist/SHA256SUMS.asc dist/SHA256SUMS
git push origin refs/tags/v0.2.0
```

Publish `dist/codex-voice-0.2.0-x86_64.deb`, `dist/SHA256SUMS`, and `dist/SHA256SUMS.asc` together on the release associated with the pushed tag.

### Verifiers

Obtain the release public key and expected release commit through a trusted channel. Fetch and verify the signed source tag, and compare the printed commit with the expected release commit:

```bash
git fetch origin tag v0.2.0
git tag --verify v0.2.0
git rev-parse --verify 'v0.2.0^{commit}'
```

Put the downloaded `.deb`, `SHA256SUMS`, and `SHA256SUMS.asc` in one directory. Verify the manifest signer before checking the package bytes:

```bash
gpg --verify SHA256SUMS.asc SHA256SUMS
sha256sum --check SHA256SUMS
```

The release package is `codex-voice`; its settings executable remains `codex-voice-settings`. Verify a direct upgrade from the prior Electron package: non-default GSettings values must survive, while the legacy `/opt` tree, AppArmor profile, alternative, and desktop entry are removed.

Before publishing, exercise fresh install, direct upgrade, extension shortcut registration, removal of legacy custom shortcuts, record/transcribe/paste, cancellation from every adapter, settings/preview launch, uninstall, and purge in clean Ubuntu 24.04/GNOME 46 and Ubuntu 26.04/GNOME 50 environments. Launch settings once from the desktop entry and once with `codex-voice --settings`; both must activate the same native window.

For each supported Ubuntu release, install the `.deb` without preconfiguring ydotool or adding the user to `input`, then log out and back in. Verify `test -r /dev/uinput && test -w /dev/uinput`; Ubuntu 24.04 must have an active `codex-voice-ydotoold.service` and `/tmp/.ydotool_socket`, while Ubuntu 26.04 must have an active `ydotool.service` and `$XDG_RUNTIME_DIR/.ydotool_socket`. Dictate into a graphical text control and a terminal. Then stop the user service, dictate again, and verify the transcript remains in History and on the clipboard, no key is emitted, and the GNOME notification identifies the socket plus the exact `systemctl --user restart` command.
