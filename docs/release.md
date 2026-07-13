# Release

Run `cargo test`, `npm test` in `settings/`, `python3 scripts/test_gnome_custom_shortcuts.py`, and the JavaScript/Python syntax checks. Then run `./build.sh` from the repository root and inspect the result with `packaging/inspect-deb.sh dist/codex-voice-<version>-x86_64.deb`.

The release package is `codex-voice`; its settings executable remains `codex-voice-settings`. This package intentionally has no Debian `Provides` or `Replaces` relationship with the former `codex-voice-settings` package. Upgrade testing must remove the old package before installing the new package and confirm that GSettings values survive removal without purge.

Before publishing, exercise fresh install, clean-break upgrade, extension and fallback shortcut paths, record/transcribe/type, cancellation from every adapter, settings/preview launch, uninstall, and purge in a clean supported Ubuntu/GNOME environment.
