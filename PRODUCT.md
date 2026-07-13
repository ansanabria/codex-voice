# Product

## Register

product

## Platform

web

## Users

Developers and power users on Ubuntu 24.04/26.04 with GNOME Shell 46 or 50. They live in the terminal and value tools that integrate with their desktop environment rather than fighting it. They want push-to-talk dictation for writing commits, docs, and messages without reaching for a browser tab or a separate subscription. They are comfortable with CLI tools and system configuration, and they expect software that respects their attention.

## Product Purpose

codex-voice is push-to-talk voice dictation for Linux. It records audio on a global shortcut, transcribes it through codex-asr using an existing Codex login, and pastes the result into whatever has focus. The GNOME Shell extension provides a native top-bar menu and global shortcut. The CLI renders the recording pill with Python and GTK3 through XWayland for deterministic placement on Wayland and X11 sessions. The settings app (Electron + React) manages enabled state, keybinding, and language detection. Success is dictating a paragraph in one breath, seeing it appear in the focused window, and never thinking about the tool again.

## Positioning

The only Linux dictation tool that pairs an existing Codex login for high-quality ASR with first-class GNOME Shell integration — native pill, global shortcut, system settings — so it feels like part of the desktop, not an app bolted on top.

## Brand Personality

Quiet system tool. Minimal, restrained, invisible until needed. The overlay pill appears during recording and vanishes when done; the settings app is opened rarely and should feel like a system preference pane, not an application. Three words: calm, precise, native.

Reference feel: Raycast and Warp terminal — clean, dark, focused surfaces where every pixel earns its place. The confidence comes from craft and restraint, not from decoration.

## Anti-references

Bloated Electron settings panels (Discord, Spotify) with nested tabs, setup wizards, and unnecessary chrome. codex-voice should never feel like a cross-platform web app trapped in a window — it should feel native to GNOME.

Overly playful or cartoonish voice/AI apps with gradient mascots, bouncy animations, and rounded-everything. Voice is a serious input method for power users, not a toy.

Legacy dense settings dialogs — flat, gray-on-gray, no visual hierarchy. Functionality without craft is not enough.

## Design Principles

- **Invisible until needed.** The overlay appears during recording and disappears after. The settings app is a rare visit. Design for moments of attention, not constant interaction — every screen should open fast, communicate clearly, and get out of the way.
- **Native, not Electron-y.** Feel like it shipped with GNOME. Match system conventions for spacing, typography, and color. Use only the two top-level General and Transcriptions tabs; avoid deeper navigation, wizards, and app-store chrome. One window, clear sections, immediate apply.
- **Show, don't tell.** The fixed pill preview demonstrates the recording state. The waveform shows recording levels. Settings changes apply instantly with visible feedback. Demonstrate state visually rather than describing it in prose.
- **Every pixel earns its place.** No decorative borders, no gradient accents, no animation for its own sake. Structure comes from spacing and typography, not from cards-within-cards or visual chrome. If removing an element doesn't change comprehension, it shouldn't be there.
- **Quiet confidence.** The tool is confident in what it does. No onboarding tours, no empty-state illustrations, no "Welcome!" headers. Calm surfaces, precise labels, immediate function.

## Accessibility & Inclusion

Common-sense good practice: keyboard-navigable controls, readable contrast ratios (body text 4.5:1 minimum), clear focus indicators, and screen-reader-friendly labels. The waveform and pulse animations should respect `prefers-reduced-motion` in the settings app. No formal WCAG target, but the settings app should be usable with keyboard alone and legible at system font scales.
