<p align="left">
  <img src="distribution/codex-voice.png" alt="Codex Voice" width="128" />
</p>

# Codex Voice

Push-to-talk voice dictation for Ubuntu 24.04 and 26.04 with GNOME Shell 46 or 50, powered by the same voice-to-text models the Codex App uses.

> **Unofficial project.** Codex Voice is not affiliated with, endorsed by, or supported by OpenAI or the Codex team. There is no warranty and no support channel. Use it at your own risk.

## Why I needed this

I've been using agents for quite a while, and recently I've grown to like using voice-to-text to communicate my ideas to agents. I used [Handy](https://handy.computer/) for quite some time, but I didn't quite end up liking the local voice-to-text models. OpenAI's models, on the other hand, are excellent for voice-to-text and in particular the Codex App has this feature. Since on Linux there's still no Codex App (and I think there probably won't be for quite some time), I decided to implement the voice-to-text feature using an existing ChatGPT Plus/Pro subscription, meaning we can use the same models as the Codex App without having to pay API prices on the voice models.

## What it does

- **Push-to-talk dictation** — press a global keyboard shortcut, speak, and the complete transcription is pasted immediately into whatever window has focus.
- **On-screen pill** — a small recording indicator appears while listening and disappears when done.
- **GNOME integration** — an optional top-bar indicator with start/stop/cancel controls and a settings window.
- **Language support** — automatic detection by default, or pick from common languages and regional variants.

## Supported platforms

| Ubuntu    | GNOME Shell | Wayland | X11 |
| --------- | ----------- | ------- | --- |
| 24.04 LTS | 46          | yes     | yes |
| 26.04 LTS | 50          | yes     | yes |

Other GNOME versions may work, but have not been tested and I can't support them. I try to support the latest stable versions because those are the ones most people use. In theory, any GNOME version 46 or higher should work, but I can't guarantee it.

On supported GNOME versions, the Shell extension provides the top-bar menu and global shortcut. The CLI renders the pill with Python, GTK3, and XWayland so it positions consistently on both Wayland and X11.

## Install

### Prerequisites

- **Codex CLI** installed and logged in on the same machine. See the [Codex CLI install instructions](https://github.com/openai/codex#quickstart) if you don't have it yet.
- Ubuntu 24.04 or 26.04 with GNOME Shell 46 or 50.

### From a .deb

The `.deb` is the complete installer. It bundles the CLI, `codex-asr`, the GTK pill, the GSettings schema, the GNOME extension, session setup, a desktop entry, an icon, and the settings app. `apt` pulls in the required runtime packages automatically.

Download the `deb` package:

```bash
curl -L -o codex-voice-0.2.0-x86_64.deb https://github.com/ansanabria/codex-voice/releases/download/v0.2.0/codex-voice-0.2.0-x86_64.deb
```

Install the package:

```bash
sudo apt install ./codex-voice-0.2.0-x86_64.deb
```

After install:

- **Log out and log back in once.** The next GNOME login enables the extension, grants the active local session access to `/dev/uinput`, and starts the per-user `ydotoold` paste service.
- Codex Voice deliberately does not add your account to the `input` group. Its packaged udev rule grants only the active local desktop session access to the synthetic-input device; it does not grant persistent access to physical keyboard and pointer devices under `/dev/input`.

### Build from source

```bash
cargo test
python3 -m unittest settings/test_codex_voice_settings.py
./build.sh
```

This produces the complete product `.deb` in `dist/`. The settings adapter is Python with GTK 4/libadwaita; it invokes fixed Rust CLI argument arrays and never writes GSettings directly.

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
cargo test                                      # Rust CLI tests
python3 -m unittest settings/test_codex_voice_settings.py
```

The settings adapter uses native GTK 4/libadwaita widgets. Rust remains authoritative for settings validation, shortcut synchronisation, history, and preview lifecycle.

## Uninstall

```bash
sudo apt remove codex-voice           # remove the application
./scripts/uninstall.sh                # also clean legacy per-user files
./scripts/uninstall.sh --purge        # also reset saved preferences
```

## Troubleshooting

### Transcription is copied but not pasted

Codex Voice saves every successful transcript to History and copies it to the clipboard before checking paste automation. If `ydotoold` is unavailable, no paste keys are sent and the GNOME notification names the failing socket and a repair command.

After installation, first log out and back in. On Ubuntu 24.04 the package runs `codex-voice-ydotoold.service`; on Ubuntu 26.04 it uses Debian's `ydotool.service`. Check the applicable service with:

```bash
systemctl --user status codex-voice-ydotoold.service  # Ubuntu 24.04
systemctl --user status ydotool.service               # Ubuntu 26.04
```

If the service reports `/dev/uinput` permission errors after a full login restart, reapply the packaged active-session rule and restart the service:

```bash
sudo udevadm control --reload-rules
sudo udevadm trigger --action=change --name-match=/dev/uinput
systemctl --user restart codex-voice-ydotoold.service  # Ubuntu 24.04
# or: systemctl --user restart ydotool.service         # Ubuntu 26.04
```

### Transcription fails with an auth error

Codex Voice doesn't handle authentication itself — the bundled `codex-asr` reuses your Codex CLI login, stored at `~/.codex/auth.json` (or `$CODEX_HOME/auth.json` when that variable is set). If transcription fails with an HTTP 401/403 or "unauthorized" message, that login is missing or expired.

To fix it, log in with the Codex CLI on the same machine and user account that runs Codex Voice:

```bash
codex login
```

Then verify the auth file exists:

```bash
ls -l ~/.codex/auth.json
```

If you set `CODEX_HOME` to a non-default location, point it at the directory that contains `auth.json` and restart any running Codex Voice process. After a successful `codex login`, the next dictation should transcribe normally.

## Disclaimer

Codex Voice is an independent, community project. It is **not** developed or supported by OpenAI. Issues, bugs, and feature requests should not be directed at OpenAI or the Codex team. There is no guarantee of continued maintenance or compatibility with future Codex changes.

In particular, things may change after the migration from the old Codex App to the new ChatGPT App that OpenAI is currently doing. If you catch any bugs, please open an issue.
