#!/usr/bin/env python3

import importlib.util
import pathlib
import unittest
from unittest import mock


SCRIPT = pathlib.Path(__file__).with_name("gnome-custom-shortcuts.py")
SPEC = importlib.util.spec_from_file_location("shortcuts", SCRIPT)
shortcuts = importlib.util.module_from_spec(SPEC)
assert SPEC.loader
SPEC.loader.exec_module(shortcuts)


class ShortcutOwnershipTests(unittest.TestCase):
    def test_parses_empty_gvariant_array(self):
        self.assertEqual(shortcuts.parse_paths("@as []"), [])

    def test_numbered_legacy_entry_is_owned(self):
        self.assertTrue(shortcuts.is_owned(
            f"{shortcuts.SHORTCUT_PREFIX}custom16/",
            "Codex Voice Dictation",
            "/home/me/.local/bin/codex-voice",
        ))

    def test_reserved_slot_is_owned_even_when_stale(self):
        self.assertTrue(shortcuts.is_owned(shortcuts.OWNED_SLOT, "", ""))

    def test_same_name_with_another_command_is_preserved(self):
        self.assertFalse(shortcuts.is_owned(
            f"{shortcuts.SHORTCUT_PREFIX}custom7/",
            "Codex Voice Dictation",
            "/home/me/bin/unrelated",
        ))

    def test_same_executable_with_another_name_is_preserved(self):
        self.assertFalse(shortcuts.is_owned(
            f"{shortcuts.SHORTCUT_PREFIX}custom8/",
            "My personal dictation workflow",
            "/home/me/.local/bin/codex-voice",
        ))

    def test_shell_text_that_mentions_binary_is_preserved(self):
        self.assertFalse(shortcuts.is_owned(
            f"{shortcuts.SHORTCUT_PREFIX}custom9/",
            "Codex Voice Dictation",
            "sh -c 'codex-voice'",
        ))

    def test_cleanup_removes_all_owned_entries_and_keeps_unrelated_entries(self):
        owned_numbered = f"{shortcuts.SHORTCUT_PREFIX}custom16/"
        unrelated = f"{shortcuts.SHORTCUT_PREFIX}custom17/"
        paths = [owned_numbered, unrelated, shortcuts.OWNED_SLOT]
        metadata = {
            owned_numbered: (shortcuts.OWNED_NAME, "/home/me/.local/bin/codex-voice"),
            unrelated: ("Launch notes", "/home/me/bin/notes"),
            shortcuts.OWNED_SLOT: ("", ""),
        }

        def read(path, key):
            return metadata[path][0 if key == "name" else 1]

        with mock.patch.object(shortcuts, "gsettings") as gs, \
                mock.patch.object(shortcuts, "read_shortcut", side_effect=read):
            gs.side_effect = [repr(paths), ""]
            self.assertEqual(shortcuts.remove_owned(), 2)
            gs.assert_called_with(
                "set", shortcuts.MEDIA_KEYS_SCHEMA, shortcuts.SHORTCUT_KEY, repr([unrelated])
            )

    def test_remove_command_succeeds_even_when_it_removes_one_entry(self):
        with mock.patch.object(shortcuts.sys, "argv", [str(SCRIPT), "remove"]), \
                mock.patch.object(shortcuts, "remove_owned", return_value=1):
            self.assertEqual(shortcuts.main(), 0)

    def test_package_session_setup_prefers_extension_and_falls_back(self):
        setup = SCRIPT.parent.parent.joinpath("packaging/codex-voice-session-setup.sh").read_text()
        self.assertIn(
            'rm -f "$HOME/.config/autostart/io.github.andy_spike.CodexVoice.desktop"',
            setup,
        )
        self.assertIn(
            "dconf reset /io/github/andy_spike/CodexVoice/launch-on-startup",
            setup,
        )
        self.assertIn(
            "dconf reset /io/github/andy_spike/CodexVoice/start-hidden",
            setup,
        )
        enable = setup.index('gnome-extensions enable "$UUID"')
        fallback = setup.index('"$HELPER" install')
        cleanup = setup.index('"$HELPER" remove')
        self.assertLess(enable, cleanup)
        self.assertLess(enable, fallback)


if __name__ == "__main__":
    unittest.main()
