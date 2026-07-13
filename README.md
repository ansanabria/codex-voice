<p align="left">
  <img src="distribution/codex-voice.png" alt="Codex Voice" width="128" />
</p>

# Codex Voice

Push-to-talk voice dictation for Ubuntu 24.04 and 26.04 with GNOME Shell 46 or 50, powered by the same voice-to-text models the Codex App uses.

> **Unofficial project.** Codex Voice is not affiliated with, endorsed by, or supported by OpenAI or the Codex team. There is no warranty and no support channel. Use it at your own risk.

## Why I needed this

I've been using agents for quite a while, and recently I've grown to like using voice-to-text to communicate my ideas to agents. I used Handy for quite some time, but I didn't quite end up liking the local voice-to-text models. OpenAI's models, on the other hand, are excellent for voice-to-text and in particular the Codex App has this feature. Since on Linux there's still no Codex App (and I think there probably won't be for quite some time), I decided to implement the voice-to-text feature using an existing ChatGPT Plus/Pro subscription, meaning we can use the same models as the Codex App.

## What it does

- **Push-to-talk dictation** — press a global keyboard shortcut, speak, and the transcription is typed into whatever window has focus.
- **On-screen pill** — a small recording indicator appears while listening and disappears when done.
- **GNOME integration** — an optional top-bar indicator with start/stop/cancel controls and a settings window.
- **Language support** — automatic detection by default, or pick from common languages and regional variants.

## Supported platforms

| Ubuntu    | GNOME Shell | Wayland | X11 |
| --------- | ----------- | ------- | --- |
| 24.04 LTS | 46          | yes     | yes |
| 26.04 LTS | 50          | yes     | yes |

Other GNOME versions may work, but have not been tested and I can't support them. I try to support the latest stable versions because those are the ones most people use.

On supported GNOME versions, the Shell extension provides the top-bar menu and global shortcut. The CLI renders the pill with Python, GTK3, and XWayland so it positions consistently on both Wayland and X11.

## Install

### Prerequisites

- **Codex CLI** installed and logged in on the same machine. See the [Codex CLI install instructions](https://github.com/openai/codex#quickstart) if you don't have it yet.
- Ubuntu 24.04 or 26.04 with GNOME Shell 46 or 50.

### From a .deb

The `.deb` is the complete installer. It bundles the CLI, `codex-asr`, the GTK pill, the GSettings schema, the GNOME extension, shortcut setup, a desktop entry, an icon, and the settings app. `apt` pulls in the required runtime packages automatically.

```bash
sudo apt install ./dist/codex-voice-0.1.0-x86_64.deb
```

After install:

- On the next GNOME login, the extension is enabled automatically. Log out and log back in to apply it. If the shell is unsupported, a legacy global shortcut is configured instead.

### Build from source

```bash
cargo test
npm --prefix settings install
./build.sh
```

This produces the complete product `.deb` in `dist/`. The settings workspace's `npm run build` command builds only the Electron adapter; product orchestration belongs to the root `build.sh`.

## Usage

```text
codex-voice                 # toggle (default)
codex-voice --toggle
codex-voice --start         # begin recording
codex-voice --stop          # stop and transcribe
codex-voice --cancel        # discard the current recording
codex-voice --status        # print current state
codex-voice --settings      # open the settings window
codex-voice settings get    # print settings as JSON
codex-voice settings set language auto
```

Settings writes return the complete JSON document. The shared schema lives in `~/.local/share/codex-voice/schemas/`.

## Language

Language defaults to `auto`, which omits `--language` when calling `codex-asr` and lets the upstream service infer it. An explicit lower-cased code such as `en`, `es`, or `zh-hant` is passed as a hint.

`CODEX_VOICE_LANG` overrides the saved value. `CODEX_VOICE_LANG=auto` or an empty value selects automatic detection. The settings app shows the active override.

> Automatic detection relies on an undocumented upstream endpoint. Its accuracy and supported language set are not a stable API.

## Development

```bash
cargo test                    # Rust CLI tests
cd settings && npm install && npm test   # settings app tests
```

The Electron renderer is React + TypeScript + Vite + Tailwind CSS v4. Its preload bridge exposes only typed settings operations, and the main process invokes the Rust CLI with fixed argument arrays rather than a shell.

## Uninstall

```bash
sudo apt remove codex-voice           # remove the application
./scripts/uninstall.sh                # also clean legacy per-user files
./scripts/uninstall.sh --purge        # also reset saved preferences
```

## Disclaimer

Codex Voice is an independent, community project. It is **not** developed or supported by OpenAI. Issues, bugs, and feature requests should not be directed at OpenAI or the Codex team. There is no guarantee of continued maintenance or compatibility with future Codex changes.

In particular, things may change after the migration from the old Codex App to the new ChatGPT App that OpenAI is currently doing. If you catch any bugs, please open an issue.
