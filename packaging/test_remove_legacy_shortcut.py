#!/usr/bin/env python3

import importlib.util
import pathlib
import unittest
from unittest import mock


SCRIPT = pathlib.Path(__file__).with_name("remove-legacy-shortcut.py")
SPEC = importlib.util.spec_from_file_location("remove_legacy_shortcut", SCRIPT)
shortcuts = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(shortcuts)


class LegacyShortcutRemovalTests(unittest.TestCase):
    def test_removes_owned_entries_and_preserves_unrelated_entries(self):
        numbered = f"{shortcuts.SHORTCUT_PREFIX}custom16/"
        unrelated = f"{shortcuts.SHORTCUT_PREFIX}custom17/"
        paths = [numbered, unrelated, shortcuts.OWNED_SLOT]
        metadata = {
            numbered: (shortcuts.OWNED_NAME, "/home/me/.local/bin/codex-voice"),
            unrelated: ("Launch notes", "/home/me/bin/notes"),
            shortcuts.OWNED_SLOT: ("", ""),
        }

        def read(path, key):
            return metadata[path][0 if key == "name" else 1]

        with mock.patch.object(shortcuts, "gsettings") as gs, mock.patch.object(
            shortcuts, "read_shortcut", side_effect=read
        ):
            gs.return_value = repr(paths)
            self.assertEqual(shortcuts.remove_owned(), 2)
            self.assertEqual(
                gs.call_args_list,
                [
                    mock.call("get", shortcuts.MEDIA_KEYS_SCHEMA, shortcuts.SHORTCUT_KEY),
                    mock.call(
                        "set",
                        shortcuts.MEDIA_KEYS_SCHEMA,
                        shortcuts.SHORTCUT_KEY,
                        repr([unrelated]),
                    ),
                    mock.call(
                        "reset-recursively",
                        f"{shortcuts.SHORTCUT_SCHEMA}:{numbered}",
                        check=False,
                    ),
                    mock.call(
                        "reset-recursively",
                        f"{shortcuts.SHORTCUT_SCHEMA}:{shortcuts.OWNED_SLOT}",
                        check=False,
                    ),
                ],
            )

    def test_does_not_claim_an_unrelated_command(self):
        self.assertFalse(shortcuts.is_owned(
            f"{shortcuts.SHORTCUT_PREFIX}custom7/",
            shortcuts.OWNED_NAME,
            "/home/me/bin/unrelated",
        ))


if __name__ == "__main__":
    unittest.main()
