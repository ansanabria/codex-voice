# ADR-0001: One product with separate desktop adapters

- Status: Accepted
- Date: 2026-07-12

## Context

Codex Voice has one dictation lifecycle but several desktop entry points. Duplicating product behavior across those entry points makes cancellation, cleanup, and compatibility difficult to reason about.

## Decision

Ship one default Product Package, `codex-voice`, while retaining separately implemented CLI, GTK overlay, GNOME Shell, and GTK/libadwaita settings adapters. Rust is authoritative for Dictation Session and Settings behavior. The settings adapter invokes Rust through the versioned Desktop Protocol: CLI commands plus local JSON and GSettings contracts documented in `docs/desktop-protocol-v1.md`.

The package upgrades the former Electron settings payload in place. Its `preinst` removes only the legacy `/opt` launcher, AppArmor profile, alternative, and generated desktop entry; saved GSettings remain compatible unless purged.

## Consequences

Adapters remain independently deployable and testable but invoke Rust for behavior. Protocol readers validate versions and degrade safely. Incompatible protocol changes require a new protocol version.
