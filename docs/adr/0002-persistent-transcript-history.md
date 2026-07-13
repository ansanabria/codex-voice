# ADR 0002: Persist successful transcripts in a local SQLite history

## Status

Accepted

## Context

Typing automation cannot paste into a window when no editable control has focus. A successful transcript was previously held only in a runtime file that was deleted after transcription, so users could lose text despite successful ASR.

History must remain available indefinitely, support newest-first search and pagination, and allow deletion of one entry or all entries. The GNOME extension also needs to recover the newest transcript without opening Settings.

## Decision

Rust remains authoritative for transcript lifecycle and stores every successful, nonempty, non-placeholder transcript before clipboard and typing attempts. Each Transcript Entry contains an integer identifier, creation time in Unix epoch milliseconds, and text. Cancelled or failed transcription attempts are not entries.

Entries are stored in a per-user SQLite database under the XDG data directory. The CLI exposes bounded history queries and mutation commands to Desktop Adapters. The Settings adapter displays newest-first batches of 50 with literal substring search, Copy and immediate Delete actions, and a confirmed Clear History action. The extension keeps Copy Last Transcript visible, disables it when no entry exists, and briefly confirms success.

## Consequences

Transcript recovery no longer depends on focus or successful typing automation. History is private to the local user profile and has no automatic retention limit, so storage grows until the user deletes entries. SQLite adds a bundled native dependency but provides durable writes, ordering, concurrent readers, and efficient pagination without inventing a file format.
