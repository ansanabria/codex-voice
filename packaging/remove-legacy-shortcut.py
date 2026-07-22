#!/usr/bin/env python3
"""Remove Codex Voice custom shortcuts left by pre-extension releases."""

from __future__ import annotations

import ast
import os
import shlex
import subprocess


MEDIA_KEYS_SCHEMA = "org.gnome.settings-daemon.plugins.media-keys"
SHORTCUT_SCHEMA = f"{MEDIA_KEYS_SCHEMA}.custom-keybinding"
SHORTCUT_KEY = "custom-keybindings"
SHORTCUT_PREFIX = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/"
OWNED_NAME = "Codex Voice Dictation"
OWNED_SLOT = f"{SHORTCUT_PREFIX}codex-voice/"


def gsettings(*args: str, check: bool = True) -> str:
    executable = os.environ.get("CODEX_VOICE_GSETTINGS", "gsettings")
    result = subprocess.run(
        [executable, *args],
        check=check,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    return result.stdout.strip()


def parse_paths(value: str) -> list[str]:
    if value.strip() == "@as []":
        return []
    paths = ast.literal_eval(value)
    if not isinstance(paths, list) or not all(isinstance(path, str) for path in paths):
        raise ValueError("GNOME custom-keybindings is not a string array")
    return paths


def read_shortcut(path: str, key: str) -> str:
    value = ast.literal_eval(gsettings("get", f"{SHORTCUT_SCHEMA}:{path}", key))
    return value if isinstance(value, str) else ""


def is_owned(path: str, name: str, command: str) -> bool:
    if path == OWNED_SLOT:
        return True
    try:
        words = shlex.split(command)
    except ValueError:
        return False
    return name == OWNED_NAME and bool(words) and os.path.basename(words[0]) == "codex-voice"


def remove_owned() -> int:
    paths = parse_paths(gsettings("get", MEDIA_KEYS_SCHEMA, SHORTCUT_KEY))
    kept: list[str] = []
    removed: list[str] = []
    for path in paths:
        if not path.startswith(SHORTCUT_PREFIX):
            kept.append(path)
            continue
        try:
            owned = is_owned(path, read_shortcut(path, "name"), read_shortcut(path, "command"))
        except (OSError, subprocess.CalledProcessError, SyntaxError, ValueError):
            owned = path == OWNED_SLOT
        (removed if owned else kept).append(path)

    if removed:
        gsettings("set", MEDIA_KEYS_SCHEMA, SHORTCUT_KEY, repr(kept))
        for path in removed:
            gsettings("reset-recursively", f"{SHORTCUT_SCHEMA}:{path}", check=False)
    return len(removed)


if __name__ == "__main__":
    raise SystemExit(0 if remove_owned() >= 0 else 1)
