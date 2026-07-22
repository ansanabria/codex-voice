# Changelog

## 0.2.0 - 2026-07-21

- Ship Codex Voice as one Debian package with the Rust application, pinned
  `codex-asr`, GNOME Shell extension, GTK overlay, native settings window, and
  session integration.
- Add persistent transcript history backed by bundled SQLite, including search,
  copy, individual deletion, and clear-history controls.
- Paste completed transcripts into the focused application with ydotool while
  retaining clipboard and history recovery when automated paste is unavailable.
- Support GNOME 46 and GNOME 50 with package upgrade cleanup and reproducible
  release artifacts.
