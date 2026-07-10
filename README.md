# codex-voice

Push-to-talk Linux dictation using `codex-asr` and an existing Codex login.

## Supported platforms

The release contract is Ubuntu 24.04 LTS with GNOME Shell 46 and Ubuntu 26.04 LTS with GNOME Shell 50, on either Wayland or X11. Other Ubuntu releases, GNOME 47–49, Ubuntu 22.04, and derivatives are not advertised as supported.

On supported GNOME versions, the Shell extension supplies the top-bar menu, global shortcut, and native Wayland/X11 pill. The CLI retains the GTK3/XWayland pill as an extension-free fallback.

## Install

Install everything—CLI, settings AppImage, schema, GTK fallback, desktop entry, and GNOME extension—with one command:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://raw.githubusercontent.com/ansanabria/codex-voice/master/install.sh | bash
```

From a checkout, run the same installer directly:

```bash
./install.sh
```

If a settings AppImage has already been built locally, the installer picks it up from `settings/dist/` or `settings/release/`. A specific artifact can be supplied with `CODEX_VOICE_SETTINGS_APPIMAGE=/path/to/file.AppImage ./install.sh`.

The installer builds the Rust CLI, compiles the private GSettings schema, installs a local AppImage or downloads a version-pinned one with SHA-256 verification, then enables the extension when the host is in the support contract. On a supported Ubuntu release with an unexpected Shell, use `CODEX_VOICE_EXTENSION_OVERRIDE=1` only if you explicitly accept extension risk. Unsupported hosts still receive the CLI with a warning.

The settings AppImage is installed at `~/.local/share/codex-voice/codex-voice-settings.AppImage`; its `~/.local/bin/codex-voice-settings` wrapper does not disable Electron's sandbox.

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
./scripts/uninstall.sh          # preserve saved preferences
./scripts/uninstall.sh --purge  # also reset saved preferences
```

Neither variant removes system packages or `codex-asr`.
