# Codex Voice Domain Context

## Dictation Session

The product lifecycle that records audio, transcribes it, and pastes or copies the result. A session may be recording, transcribing, or inserting text and can be cancelled at every stage. The protocol retains the legacy `typing` state name for the insertion stage. Cleanup is idempotent and only removes state owned by that session.

## Desktop Protocol

The versioned local contract shared by the Rust application and desktop adapters: CLI commands and exit behavior, JSON settings/status/runtime-state documents, GSettings keys, environment overrides, and runtime filenames.

## Desktop Adapter

A separately implemented interface to the desktop. The CLI, GTK overlay, GNOME Shell extension, and Electron settings executable are adapters; none owns dictation behavior.

## Settings

Persistent user preferences stored under the `io.github.andy_spike.CodexVoice` GSettings schema. Environment overrides may change effective runtime behavior without changing saved preferences.

## Runtime State

Ephemeral, atomically replaced state describing an active Dictation Session. It is absent while idle and must be treated as untrusted input by every Desktop Adapter.

## Product Package

The single Debian package named `codex-voice`. It contains the Rust CLI/application, `codex-asr`, GTK overlay, schema, GNOME extension, shortcut helper, desktop integration files, and the `codex-voice-settings` Electron executable.
