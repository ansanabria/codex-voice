#!/usr/bin/env python3
"""Manage the legacy GNOME custom shortcut owned by Codex Voice."""

from __future__ import annotations

import ast
import os
import shlex
import subprocess
import sys


MEDIA_KEYS_SCHEMA = "org.gnome.settings-daemon.plugins.media-keys"
SHORTCUT_SCHEMA = f"{MEDIA_KEYS_SCHEMA}.custom-keybinding"
SHORTCUT_KEY = "custom-keybindings"
SHORTCUT_PREFIX = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/"
OWNED_NAME = "Codex Voice Dictation"
OWNED_SLOT = f"{SHORTCUT_PREFIX}codex-voice/"


def parse_paths(value: str) -> list[str]:
    value = value.strip()
    if value == "@as []":
        return []
    parsed = ast.literal_eval(value)
    if not isinstance(parsed, list) or not all(isinstance(path, str) for path in parsed):
        raise ValueError("GNOME custom-keybindings is not a string array")
    return parsed


def is_owned(path: str, name: str, command: str) -> bool:
    """Require both our exact label and executable, except for our reserved slot."""
    if path == OWNED_SLOT:
        return True
    try:
        words = shlex.split(command)
    except ValueError:
        return False
    return name == OWNED_NAME and bool(words) and os.path.basename(words[0]) == "codex-voice"


def gsettings(*args: str) -> str:
    executable = os.environ.get("CODEX_VOICE_GSETTINGS", "gsettings")
    result = subprocess.run(
        [executable, *args], check=True, text=True, stdout=subprocess.PIPE
    )
    return result.stdout.strip()


def read_shortcut(path: str, key: str) -> str:
    value = gsettings("get", f"{SHORTCUT_SCHEMA}:{path}", key)
    parsed = ast.literal_eval(value)
    return parsed if isinstance(parsed, str) else ""


def remove_owned() -> int:
    paths = parse_paths(gsettings("get", MEDIA_KEYS_SCHEMA, SHORTCUT_KEY))
    kept: list[str] = []
    removed = 0
    for path in paths:
        if not path.startswith(SHORTCUT_PREFIX):
            kept.append(path)
            continue
        try:
            owned = is_owned(path, read_shortcut(path, "name"), read_shortcut(path, "command"))
        except (OSError, subprocess.CalledProcessError, SyntaxError, ValueError):
            owned = path == OWNED_SLOT
        if owned:
            removed += 1
        else:
            kept.append(path)
    if removed:
        # repr(list[str]) is valid GVariant array syntax.
        gsettings("set", MEDIA_KEYS_SCHEMA, SHORTCUT_KEY, repr(kept))
    return removed


def read_keybinding() -> str:
    try:
        value = gsettings("get", "io.github.andy_spike.CodexVoice", "keybinding")
        parsed = ast.literal_eval(value)
        if isinstance(parsed, list) and parsed and isinstance(parsed[0], str) and parsed[0]:
            return parsed[0]
    except (OSError, subprocess.CalledProcessError, SyntaxError, ValueError):
        pass
    return "<Control><Super>space"


def install_owned() -> int:
    paths = parse_paths(gsettings("get", MEDIA_KEYS_SCHEMA, SHORTCUT_KEY))
    if OWNED_SLOT not in paths:
        paths.append(OWNED_SLOT)
        gsettings("set", MEDIA_KEYS_SCHEMA, SHORTCUT_KEY, repr(paths))
    key = f"{SHORTCUT_SCHEMA}:{OWNED_SLOT}"
    gsettings("set", key, "name", OWNED_NAME)
    gsettings("set", key, "command", "/usr/bin/codex-voice")
    gsettings("set", key, "binding", read_keybinding())
    return 0


def main() -> int:
    if sys.argv[1:] == ["remove"]:
        remove_owned()
        return 0
    if sys.argv[1:] == ["install"]:
        return install_owned()
    if sys.argv[1:] != []:
        print(f"usage: {sys.argv[0]} install|remove", file=sys.stderr)
        return 2
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
