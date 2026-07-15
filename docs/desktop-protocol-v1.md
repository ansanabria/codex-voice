# Desktop Protocol version 1

Version 1 is the local contract between the Rust application and Desktop Adapters. Additive JSON fields are compatible within version 1. Removing or changing fields, meanings, states, filenames, commands, or exit behavior requires a version increment.

## CLI

`codex-voice` or `--toggle` toggles a session. `--start`, `--stop`, and `--cancel` perform the named lifecycle operation. `--status` prints status JSON. `--settings` launches `codex-voice-settings`. `--preview` and `--close-preview` control the isolated GTK preview. `--copy-last` copies the newest saved transcript and exits 1 when history is empty. `--version` prints the version. `settings get`, `settings reset`, and `settings set <key> <value>` print settings JSON after success.

`history list <offset> <limit> [query]` prints newest-first transcript history JSON. Limits are clamped to 1–100 and query performs a case-insensitive literal substring match. `history has` exits 0 when an entry exists and 1 otherwise. `history delete <id>` removes one entry and `history clear` removes all entries.

Success exits 0. Child adapters may return their own nonzero code. Invalid input and operational errors print `codex-voice: <message>` to stderr and exit 1. Commands produce no stdout unless stated above.

## Runtime state

The file is `$XDG_RUNTIME_DIR/codex-voice-state.json`, falling back to `/tmp/codex-voice-state.json`. Writers create a sibling temporary file and atomically rename it. Active documents contain `schemaVersion: 1`, a `state` of `recording`, `transcribing`, or `typing`, positive integer `ownerPid`, and integer `startedAt` (Unix epoch milliseconds). `typing` is the legacy protocol name for the text-insertion stage, which pastes the completed transcript in one operation. The file is absent while idle and is removed during idempotent cleanup. Missing, malformed, unknown-state, or unsupported-version documents are displayed as idle/unknown and must never select a destructive action.

Other runtime files retain their existing names: `codex-voice.pid`, `.wav`, `-overlay.pid`, `-preview-overlay.pid`, `-transcriber.pid`, `-transcript.txt`, `-cancelled`, `-typing.pid`, and `-session-owner.pid` in the same runtime directory.

## Status JSON

`--status` returns `schemaVersion: 1`, `state` (`idle` plus active states), boolean `extensionActive`, and strings `ubuntu` and `gnomeShell`. Readers reject unsupported versions and unknown states.

## Settings JSON and storage

Settings JSON contains `schemaVersion: 1`, booleans `enabled` and `showTrayIcon`, strings `keybinding` and `language`, and `overrides.language` (string or null). The GSettings schema is `io.github.andy_spike.CodexVoice`; keys are `enabled`, `show-tray-icon`, `keybinding`, and `language`.

The supported environment overrides are:

- `CODEX_VOICE_LANG` selects the effective transcription language.
- `CODEX_VOICE_BIN` selects the CLI invoked by the Electron adapter.
- `CODEX_VOICE_SETTINGS_BIN` selects the Settings executable launched by Rust.
- `CODEX_VOICE_OVERLAY` selects the GTK overlay script.
- `CODEX_VOICE_OVERLAY_BACKEND` selects its GDK backend.
- `CODEX_VOICE_SHORTCUT_HELPER` selects the GNOME fallback-shortcut helper.

Resource path overrides must name existing files. `CODEX_VOICE_GDK_BACKEND` remains a deprecated alias for `CODEX_VOICE_OVERLAY_BACKEND` for compatibility. Without overrides, the Product Package resolver checks the canonical installed layout and explicit executable-relative, source-tree, and per-user development forms in resource-specific priority order. Rust and the Electron GSettings monitor preserve `GSETTINGS_SCHEMA_DIR` and add existing canonical system and per-user schema directories.

## Transcript history

Every nonempty, non-placeholder transcript is saved immediately after successful ASR and before clipboard or paste automation. Cancelled and failed sessions are not saved. History is stored indefinitely in `$XDG_DATA_HOME/codex-voice/transcripts.sqlite3`, falling back to `~/.local/share/codex-voice/transcripts.sqlite3`.

History JSON contains `schemaVersion: 1`, `entries`, and `hasMore`. Each entry has integer `id`, integer `createdAt` (Unix epoch milliseconds), and string `text`.
