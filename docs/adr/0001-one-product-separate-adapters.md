# ADR-0001: One product with separate desktop adapters

- Status: Accepted
- Date: 2026-07-12

## Context

Codex Voice has one dictation lifecycle but several desktop entry points. Duplicating product behavior across those entry points makes cancellation, cleanup, and compatibility difficult to reason about.

## Decision

Ship one default Product Package, `codex-voice`, while retaining separately implemented CLI, GTK, GNOME Shell, and Electron Desktop Adapters. Rust is authoritative for Dictation Session and Settings behavior. The Desktop Protocol is the CLI plus versioned local JSON and GSettings contracts documented in `docs/desktop-protocol-v1.md`.

The Debian rename from `codex-voice-settings` is an intentional clean break. The new package does not declare `Provides`, `Replaces`, or an automatic package migration. Users remove the old package before installing the new one; saved GSettings remain compatible unless purged.

## Consequences

Adapters remain independently deployable and testable but invoke Rust for behavior. Protocol readers validate versions and degrade safely. Incompatible protocol changes require a new protocol version.
