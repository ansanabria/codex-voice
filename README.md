# codex-voice

Push-to-talk voice dictation for Linux using your ChatGPT/OpenAI subscription.

codex-voice reuses the OAuth token from the [Codex CLI](https://github.com/openai/codex) to send audio to the same transcription endpoint that the Codex desktop app uses — no API key, no separate billing. Press a hotkey, speak, and the transcription is typed directly at your cursor.

Built for GNOME Wayland on Ubuntu. Inspired by [Handy](https://github.com/cjpais/Handy)'s recording overlay design.

## How it works

```
Ctrl+Super+Space → arecord captures mic audio
                  → overlay pill appears with animated waveform
Ctrl+Super+Space → audio uploaded to chatgpt.com/backend-api/transcribe
                  → overlay switches to "Transcribing…"
                  → transcription typed at cursor via ydotool
                  → text also copied to clipboard
```

Authentication is handled by [codex-asr](https://github.com/Wangnov/codex-asr), which reads `~/.codex/auth.json` (your Codex CLI login) and sends the audio with the correct OAuth bearer token and account ID headers.

## Requirements

- **ChatGPT Plus/Pro subscription** with [Codex CLI](https://github.com/openai/codex) installed and authenticated (`codex login`)
- **GNOME Wayland** (tested on Ubuntu 26.04, GNOME Shell 50)
- **Linux x86_64** (codex-asr provides prebuilt binaries)

### System packages

```bash
sudo apt install alsa-utils wl-clipboard ydotool python3-gi python3-gi-cairo libnotify-bin
```

| Package | Purpose |
|---|---|
| `alsa-utils` | `arecord` for microphone capture |
| `wl-clipboard` | `wl-copy` for clipboard support |
| `ydotool` | Types transcription at cursor (works on GNOME Wayland) |
| `python3-gi` | GTK3 Python bindings |
| `python3-gi-cairo` | Cairo bridge for transparent overlay window |
| `libnotify-bin` | `notify-send` (optional, for error notifications) |

### codex-asr

Installed automatically by the install script, or manually:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/wangnov/codex-asr/releases/latest/download/codex-asr-installer.sh \
  | sh
```

## Install

```bash
git clone https://github.com/andy-spike/codex-voice.git
cd codex-voice
./scripts/install.sh
```

The install script will:
1. Check all dependencies
2. Install codex-asr if missing
3. Copy the toggle script to `~/.local/bin/codex-voice`
4. Configure a GNOME custom shortcut (`Ctrl+Super+Space`)

To use a different hotkey:

```bash
./scripts/install.sh "<Super><Shift>D"
```

## Usage

Press `Ctrl+Super+Space`:
1. **First press** — recording starts, overlay pill appears at bottom-center with green mic icon and animated waveform bars
2. **Second press** — recording stops, pill switches to "Transcribing…", then transcription is typed at your cursor and copied to clipboard

### Configuration

| Environment variable | Default | Description |
|---|---|---|
| `CODEX_VOICE_LANG` | `en` | Language hint for transcription (e.g. `es`, `zh`) |

Add to `~/.zshrc` or `~/.bashrc`:
```bash
export CODEX_VOICE_LANG=es
```

## Uninstall

```bash
./scripts/uninstall.sh
```

Removes the script and GNOME shortcut. codex-asr and system packages are left untouched.

## Project structure

```
codex-voice/
├── src/
│   └── overlay.py        # GTK3 recording overlay (waveform pill)
├── scripts/
│   ├── codex-voice       # Toggle script (start/stop/transcribe/type)
│   ├── install.sh        # Dependency check + GNOME shortcut setup
│   └── uninstall.sh      # Remove scripts + shortcut
├── assets/               # Screenshots, demo gifs
├── LICENSE
└── README.md
```

## Caveats

- The `chatgpt.com/backend-api/transcribe` endpoint is undocumented and reverse-engineered from Codex Desktop behavior. It may stop working if OpenAI changes the backend.
- Cloudflare occasionally returns `403` challenges to non-browser requests. If transcription starts failing, run `codex login` to refresh your OAuth token.
- GNOME Wayland does not support arbitrary window positioning via `move()`. The overlay position may vary depending on compositor behavior.
- `ydotool` types via kernel uinput, which bypasses Wayland's virtual-keyboard restrictions but requires the `ydotoold` daemon to be running.

## Credits

- [codex-asr](https://github.com/Wangnov/codex-asr) — Rust CLI that reuses Codex ChatGPT auth for transcription
- [Handy](https://github.com/cjpais/Handy) — Overlay design inspiration (waveform bars, pill shape, recording states)
- [Codex CLI](https://github.com/openai/codex) — OAuth token source

## License

MIT
