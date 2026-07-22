# Desktop Protocol version 1

Version 1 is the local contract between the Rust application and Desktop Adapters. Additive JSON fields are compatible within version 1. Removing or changing fields, meanings, states, filenames, commands, or exit behavior requires a version increment.

## CLI

`codex-voice` or `--toggle` toggles a session. `--start`, `--stop`, and `--cancel` perform the named lifecycle operation. `--status` prints status JSON. `--settings` launches `codex-voice-settings`. `--preview` and `--close-preview` control the isolated GTK preview. `--copy-last` copies the newest saved transcript and exits 1 when history is empty. `--version` prints the version. `settings get`, `settings reset`, and `settings set <key> <value>` print settings JSON after success.

`history list <offset> <limit> [query]` prints newest-first transcript history JSON. Limits are clamped to 1–100 and query performs a case-insensitive literal substring match. `history has` exits 0 when an entry exists and 1 otherwise. `history delete <id>` removes one entry and `history clear` removes all entries.

Success exits 0. Child adapters may return their own nonzero code. Invalid input and operational errors print `codex-voice: <message>` to stderr and exit 1. Commands produce no stdout unless stated above.

The executable also uses identity-scoped private controls internally: `--cancel-recording <recorderPid> <recorderStartTime>`, `--watch-session <overlayPid> <overlayStartTime> <recorderPid> <recorderStartTime>`, and `--supervise-owner <ownerPid> <ownerStartTime>`. Desktop Adapters do not invoke these as general lifecycle operations. The overlay uses the recorder-scoped cancellation control; detached watcher and supervisor processes use the other controls to clean up only the session whose full process identities still match.

## Runtime state

The runtime directory is `$XDG_RUNTIME_DIR` when set. Otherwise it is the private directory `$XDG_CACHE_HOME/codex-voice/runtime`, with `~/.cache/codex-voice/runtime` used when `XDG_CACHE_HOME` is also unset. The fallback directory is created with mode `0700`; runtime state does not fall back to `/tmp`.

`codex-voice-state.json` is written through a sibling temporary file and atomic rename. An active document requires `schemaVersion: 1`, a `state` of `recording`, `transcribing`, or `typing`, positive integers `ownerPid` and `ownerStartTime`, and a nonnegative integer `startedAt` containing Unix epoch milliseconds. The PID and Linux `/proc/<pid>/stat` start time together identify the owning process; readers must reject a document when either part is missing, invalid, dead, or no longer matches. `typing` is the legacy protocol name for the text-insertion stage, which pastes the completed transcript in one operation. The file is normally absent while idle and is removed by successful identity-scoped cleanup. Missing, malformed, stale, unknown-state, or unsupported-version documents are non-active and must never select a destructive action.

Process identity records are JSON objects containing positive integer `pid` and `startTime` fields. Despite their `.pid` suffixes, they are not PID-only text files. The runtime directory uses these identity-record filenames:

- `codex-voice.pid`: active audio recorder identity.
- `codex-voice-overlay.pid`: active dictation overlay identity.
- `codex-voice-preview-overlay.pid`: isolated Settings preview overlay identity.
- `codex-voice-transcriber.pid`: active `codex-asr` identity.
- `codex-voice-typing.pid`: active paste process identity.
- `codex-voice-session-owner.pid`: process identity owning the transcribing/typing phase.
- `codex-voice-session-recorder.pid`: originating recorder identity for the owned session, used to scope delayed overlay cancellation.
- `codex-voice-cancelled`: the recorder or session-owner identity whose work was cancelled.

The detached owner supervisor is identified by its private command arguments rather than by another runtime record. It waits for the exact owner identity and performs cleanup only while `codex-voice-session-owner.pid` still contains that identity, so a stale supervisor cannot clean up a newer session. Stable non-identity filenames include `codex-voice.lock`, `codex-voice.wav`, `codex-voice-transcript.txt`, and `codex-voice-state.json`.

## Status JSON

`--status` returns `schemaVersion: 1`, `state` (`idle` plus active states), boolean `extensionActive`, and strings `ubuntu` and `gnomeShell`. Readers reject unsupported versions and unknown states.

## Settings JSON and storage

Settings JSON contains `schemaVersion: 1`, booleans `enabled` and `showTrayIcon`, strings `keybinding` and `language`, and `overrides.language` (string or null). The GSettings schema is `io.github.andy_spike.CodexVoice`; keys are `enabled`, `show-tray-icon`, `keybinding`, and `language`.

The supported environment overrides are:

- `CODEX_VOICE_LANG` selects the effective transcription language.
- `CODEX_VOICE_BIN` selects the CLI invoked by the native GTK settings adapter.
- `CODEX_VOICE_SETTINGS_BIN` selects the Settings executable launched by Rust.
- `CODEX_VOICE_OVERLAY` selects the GTK overlay script.
- `CODEX_VOICE_OVERLAY_BACKEND` selects its GDK backend.

Resource path overrides must name existing files. `CODEX_VOICE_GDK_BACKEND` remains a deprecated alias for `CODEX_VOICE_OVERLAY_BACKEND` for compatibility. Without overrides, the Product Package resolver checks the canonical installed layout and explicit executable-relative, source-tree, and per-user development forms in resource-specific priority order. Rust and the GTK settings monitor preserve `GSETTINGS_SCHEMA_DIR` and add existing canonical system and per-user schema directories.

## Transcript history

Every nonempty, non-placeholder transcript is saved immediately after successful ASR and before clipboard or paste automation. Cancelled and failed sessions are not saved. History is stored indefinitely in `$XDG_DATA_HOME/codex-voice/transcripts.sqlite3`, falling back to `~/.local/share/codex-voice/transcripts.sqlite3`.

History JSON contains `schemaVersion: 1`, `entries`, and `hasMore`. Each entry has integer `id`, integer `createdAt` (Unix epoch milliseconds), and string `text`.
