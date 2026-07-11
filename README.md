# codex-voice

Push-to-talk Linux dictation using `codex-asr` and an existing Codex login.

## Supported platforms

The release contract is Ubuntu 24.04 LTS with GNOME Shell 46 and Ubuntu 26.04 LTS with GNOME Shell 50, on either Wayland or X11. Other Ubuntu releases, GNOME 47–49, Ubuntu 22.04, and derivatives are not advertised as supported.

On supported GNOME versions, the Shell extension supplies the top-bar menu and global shortcut. The CLI renders the pill with Python, GTK3, and XWayland so it can be positioned consistently on both Wayland and X11 sessions.

## Install

The `.deb` is the complete installation artifact. It contains the CLI, bundled `codex-asr`, GTK pill, GSettings schema, GNOME extension, shortcut setup, desktop entry, icon, and settings application. `apt` also installs the required Ubuntu runtime packages.

Install a local build with:

```bash
sudo apt install ./settings/dist/codex-voice-settings-0.1.0-x86_64.deb
```

The package registers the AppArmor profile needed by Chromium's renderer sandbox on Ubuntu 24.04 and newer. On the first GNOME session after installation, the package enables the extension for the logged-in user; unsupported shells receive the legacy shortcut fallback.

## Commands

```text
codex-voice                 # toggle (backward compatible)
codex-voice --toggle
codex-voice --start | --stop | --cancel | --status | --settings
codex-voice settings get
codex-voice settings set language auto
```

Settings writes return the complete JSON document. The shared schema is installed in `~/.local/share/codex-voice/schemas/`.

## Language detection

Language defaults to `auto`. In that mode `codex-voice` intentionally omits `--language` when running `codex-asr`, allowing the upstream service to infer it. An explicit, lower-cased BCP-47-like code such as `en`, `es`, or `zh-hant` is passed as a hint.

`CODEX_VOICE_LANG` overrides the saved value. `CODEX_VOICE_LANG=auto` and an empty value select automatic detection. The settings app displays the active override.

Automatic detection is provided by an undocumented upstream endpoint. Its accuracy and supported language set are not a stable API.

## Development

```bash
cargo test
cd settings && npm install && npm run build
```

The Electron renderer is React + TypeScript + Vite + Tailwind CSS v4. Its preload bridge exposes only typed settings operations, and the main process invokes the Rust CLI with fixed argument arrays rather than a shell.

## Uninstall

```bash
sudo apt remove codex-voice-settings  # remove the installed application
./scripts/uninstall.sh                # also clean legacy per-user files
./scripts/uninstall.sh --purge        # also reset saved preferences
```
