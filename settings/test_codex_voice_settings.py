#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import tempfile
import time
import unittest
import sys
from pathlib import Path
from typing import Any, Callable
from unittest import mock

import gi

gi.require_version("Gdk", "4.0")
gi.require_version("Gtk", "4.0")
sys.path.insert(0, str(Path(__file__).resolve().parent))
from gi.repository import Gdk, GLib

import codex_voice_settings as cv


def settings_json(**extra: Any) -> dict[str, Any]:
    document: dict[str, Any] = {
        "schemaVersion": 1,
        "enabled": True,
        "showTrayIcon": True,
        "keybinding": "<Control><Super>space",
        "language": "auto",
        "overrides": {"language": None},
    }
    document.update(extra)
    return document


def history_json(**extra: Any) -> dict[str, Any]:
    document: dict[str, Any] = {
        "schemaVersion": 1,
        "entries": [{"id": 1, "createdAt": 1_700_000_000_000, "text": "Transcript"}],
        "hasMore": False,
    }
    document.update(extra)
    return document


class ProtocolTests(unittest.TestCase):
    def test_settings_accepts_additive_v1_fields_at_every_object_level(self) -> None:
        document = settings_json(extra=True, overrides={"language": "fr", "future": {"nested": True}})
        parsed = cv.parse_settings(document)
        self.assertEqual(parsed.language_override, "fr")
        self.assertTrue(parsed.enabled)

    def test_settings_rejects_missing_or_wrong_required_fields(self) -> None:
        for document in (
            settings_json(enabled="true"),
            settings_json(overrides=[]),
            settings_json(schemaVersion=2),
            {"schemaVersion": 1},
        ):
            with self.assertRaises(cv.ProtocolError):
                cv.parse_settings(document)

    def test_status_validates_schema_and_state(self) -> None:
        valid = {"schemaVersion": 1, "state": "idle", "extensionActive": False, "ubuntu": "24.04", "gnomeShell": "46"}
        self.assertEqual(cv.parse_status({**valid, "future": "ok"}).state, "idle")
        for state in ("unknown", True):
            with self.assertRaises(cv.ProtocolError):
                cv.parse_status({**valid, "state": state})
        with self.assertRaises(cv.ProtocolError):
            cv.parse_status({**valid, "schemaVersion": 0})

    def test_history_validates_safe_integers_and_additive_fields(self) -> None:
        parsed = cv.parse_history(history_json(entries=[{"id": 1, "createdAt": 2, "text": "x", "future": 1}], future=True))
        self.assertEqual(parsed.entries[0].text, "x")
        for entry in (
            {"id": True, "createdAt": 1, "text": "x"},
            {"id": 1, "createdAt": True, "text": "x"},
            {"id": 2**60, "createdAt": 1, "text": "x"},
        ):
            with self.assertRaises(cv.ProtocolError):
                cv.parse_history(history_json(entries=[entry]))

    def test_languages_preserve_exact_model_and_unknown_current_value(self) -> None:
        self.assertEqual(len(cv.LANGUAGES), 26)
        self.assertEqual([code for code, _ in cv.LANGUAGES], [
            "auto", "en", "en-us", "en-gb", "es", "es-es", "es-mx", "fr", "fr-ca", "de", "it", "pt", "pt-br", "nl", "pl", "ru", "uk", "tr", "ar", "hi", "id", "ja", "ko", "zh", "zh-cn", "zh-tw",
        ])
        self.assertEqual(cv.language_options("x-private")[0], ("x-private", "Current value (x-private)"))
        self.assertEqual(cv.language_options("auto"), cv.LANGUAGES)

    def test_setting_value_requires_exact_types(self) -> None:
        self.assertEqual(cv.setting_value("enabled", True), "true")
        self.assertEqual(cv.setting_value("show-tray-icon", False), "false")
        self.assertEqual(cv.setting_value("language", "es-mx"), "es-mx")
        for key, value in (("enabled", 1), ("language", True), ("unknown", "value")):
            with self.assertRaises(ValueError):
                cv.setting_value(key, value)  # type: ignore[arg-type]


class ShortcutTests(unittest.TestCase):
    def key(self, name: str, state: Gdk.ModifierType = Gdk.ModifierType(0)) -> tuple[str | None, str | None]:
        return cv.shortcut_from_key(Gdk.keyval_from_name(name), state)

    def test_generates_rust_supported_accelerators(self) -> None:
        self.assertEqual(self.key("space", Gdk.ModifierType.CONTROL_MASK | Gdk.ModifierType.SUPER_MASK)[0], "<Control><Super>space")
        self.assertEqual(self.key("o", Gdk.ModifierType.CONTROL_MASK | Gdk.ModifierType.ALT_MASK)[0], "<Control><Alt>o")
        self.assertEqual(self.key("o", Gdk.ModifierType.CONTROL_MASK | Gdk.ModifierType.SHIFT_MASK)[0], "<Control><Shift>o")
        self.assertEqual(self.key("F12")[0], "F12")

    def test_masks_lock_bits_and_waits_for_modifiers(self) -> None:
        state = Gdk.ModifierType.CONTROL_MASK | Gdk.ModifierType.LOCK_MASK
        self.assertEqual(self.key("o", state)[0], "<Control>o")
        self.assertEqual(self.key("Control_L"), (None, None))

    def test_special_capture_keys_and_unsupported_keys(self) -> None:
        self.assertEqual(self.key("Escape"), ("Escape", None))
        self.assertEqual(self.key("BackSpace"), ("BackSpace", None))
        self.assertEqual(self.key("Delete"), ("Delete", None))
        self.assertEqual(self.key("o"), (None, "Use Ctrl, Alt, Super, or a function key with a non-modifier key."))
        for name in ("Return", "KP_Enter", "comma"):
            accelerator, error = self.key(name, Gdk.ModifierType.CONTROL_MASK)
            self.assertIsNone(accelerator)
            self.assertEqual(error, "That key could not be identified. Try the shortcut again.")


class FakeClient:
    def __init__(self) -> None:
        self.get_callbacks: list[Callable[[cv.SettingsDocument | None, Exception | None], None]] = []
        self.set_callbacks: list[tuple[str, bool | str, Callable[[cv.SettingsDocument | None, Exception | None], None]]] = []
        self.reset_callbacks: list[Callable[[cv.SettingsDocument | None, Exception | None], None]] = []
        self.history_callbacks: list[Callable[[cv.HistoryPage | None, Exception | None], None]] = []

    def settings_get(self, callback: Callable[[cv.SettingsDocument | None, Exception | None], None]) -> None:
        self.get_callbacks.append(callback)

    def settings_set(self, key: str, value: bool | str, callback: Callable[[cv.SettingsDocument | None, Exception | None], None]) -> None:
        self.set_callbacks.append((key, value, callback))

    def settings_reset(self, callback: Callable[[cv.SettingsDocument | None, Exception | None], None]) -> None:
        self.reset_callbacks.append(callback)

    def load_history(self, offset: int, limit: int, query: str, callback: Callable[[cv.HistoryPage | None, Exception | None], None]) -> None:
        self.history_callbacks.append(callback)

    def delete_history(self, entry_id: int, callback: Callable[[None, Exception | None], None]) -> None:
        callback(None, None)

    def clear_history(self, callback: Callable[[None, Exception | None], None]) -> None:
        callback(None, None)


class CoordinatorTests(unittest.TestCase):
    def test_pending_writes_merge_and_failed_write_rolls_back(self) -> None:
        client = FakeClient()
        rendered: list[cv.SettingsDocument] = []
        errors: list[str] = []
        coordinator = cv.SettingsCoordinator(client, None, rendered.append, errors.append)
        coordinator.load()
        client.get_callbacks.pop()(cv.parse_settings(settings_json()), None)
        coordinator.save("enabled", False)
        self.assertFalse(rendered[-1].enabled)
        client.set_callbacks.pop()[2](None, RuntimeError("save failed"))
        self.assertEqual(errors, ["save failed"])
        self.assertEqual(len(client.get_callbacks), 1)
        client.get_callbacks.pop()(cv.parse_settings(settings_json(enabled=True)), None)
        self.assertTrue(rendered[-1].enabled)

    def test_writes_are_serialized_and_same_key_queue_is_coalesced(self) -> None:
        client = FakeClient(); rendered: list[cv.SettingsDocument] = []
        coordinator = cv.SettingsCoordinator(client, None, rendered.append, lambda _: None)
        coordinator.load(); client.get_callbacks.pop()(cv.parse_settings(settings_json()), None)
        coordinator.save("language", "es")
        coordinator.save("language", "fr")
        coordinator.save("language", "de")
        self.assertEqual([(key, value) for key, value, _ in client.set_callbacks], [("language", "es")])
        first = client.set_callbacks.pop(0)[2]
        first(cv.parse_settings(settings_json(language="es")), None)
        self.assertEqual(rendered[-1].language, "de")
        self.assertEqual([(key, value) for key, value, _ in client.set_callbacks], [("language", "de")])
        client.set_callbacks.pop(0)[2](cv.parse_settings(settings_json(language="de")), None)
        self.assertEqual(rendered[-1].language, "de")

    def test_reverse_completion_order_is_prevented_across_keys(self) -> None:
        client = FakeClient(); rendered: list[cv.SettingsDocument] = []
        coordinator = cv.SettingsCoordinator(client, None, rendered.append, lambda _: None)
        coordinator.load(); client.get_callbacks.pop()(cv.parse_settings(settings_json()), None)
        coordinator.save("enabled", False)
        coordinator.save("language", "fr")
        self.assertEqual([(key, value) for key, value, _ in client.set_callbacks], [("enabled", False)])
        client.set_callbacks.pop()[2](cv.parse_settings(settings_json(enabled=False)), None)
        self.assertEqual([(key, value) for key, value, _ in client.set_callbacks], [("language", "fr")])
        client.set_callbacks.pop()[2](cv.parse_settings(settings_json(enabled=False, language="fr")), None)
        self.assertFalse(rendered[-1].enabled)
        self.assertEqual(rendered[-1].language, "fr")

    def test_reset_waits_for_active_write_and_precedes_later_intent(self) -> None:
        client = FakeClient(); rendered: list[cv.SettingsDocument] = []
        coordinator = cv.SettingsCoordinator(client, None, rendered.append, lambda _: None)
        coordinator.load(); client.get_callbacks.pop()(cv.parse_settings(settings_json()), None)
        coordinator.save("language", "es")
        coordinator.reset()
        self.assertEqual(rendered[-1].language, "auto")
        coordinator.save("enabled", False)
        self.assertEqual(len(client.set_callbacks), 1)
        self.assertEqual(client.reset_callbacks, [])
        client.set_callbacks.pop()[2](cv.parse_settings(settings_json(language="es")), None)
        self.assertEqual(len(client.reset_callbacks), 1)
        self.assertEqual(client.set_callbacks, [])
        client.reset_callbacks.pop()(cv.parse_settings(settings_json()), None)
        self.assertEqual([(key, value) for key, value, _ in client.set_callbacks], [("enabled", False)])
        self.assertFalse(rendered[-1].enabled)
        client.set_callbacks.pop()[2](cv.parse_settings(settings_json(enabled=False)), None)
        self.assertFalse(rendered[-1].enabled)
        self.assertEqual(rendered[-1].language, "auto")

    def test_stale_history_completion_is_ignored(self) -> None:
        client = FakeClient(); rendered: list[cv.HistoryPage] = []
        coordinator = cv.HistoryCoordinator(client, rendered.append, lambda _: None)
        coordinator.refresh(); coordinator.refresh()
        old, current = client.history_callbacks
        old(cv.parse_history(history_json(entries=[])), None)
        self.assertEqual(rendered, [])
        current(cv.parse_history(history_json(entries=[{"id": 2, "createdAt": 3, "text": "current"}])), None)
        self.assertEqual([entry.id for entry in rendered[-1].entries], [2])

    def test_filtered_empty_page_keeps_clear_history_available(self) -> None:
        client = FakeClient()
        coordinator = cv.HistoryCoordinator(client, lambda _: None, lambda _: None)
        coordinator.refresh()
        client.history_callbacks.pop()(cv.parse_history(history_json()), None)
        self.assertTrue(coordinator.history_exists)
        coordinator.query = "no match"
        coordinator.refresh()
        client.history_callbacks.pop()(cv.parse_history(history_json(entries=[])), None)
        self.assertTrue(coordinator.history_exists)
        coordinator.clear()
        self.assertFalse(coordinator.history_exists)

    def test_history_mutations_invalidate_older_loads(self) -> None:
        for mutate in (lambda coordinator: coordinator.delete(1), lambda coordinator: coordinator.clear()):
            with self.subTest(mutation=mutate):
                client = FakeClient(); rendered: list[cv.HistoryPage] = []
                coordinator = cv.HistoryCoordinator(client, rendered.append, lambda _: None)
                coordinator.entries = [cv.TranscriptEntry(1, 2, "existing")]
                coordinator.history_exists = True
                coordinator.refresh()
                stale = client.history_callbacks.pop()
                mutate(coordinator)
                stale(cv.parse_history(history_json(entries=[{"id": 2, "createdAt": 3, "text": "stale"}])), None)
                self.assertNotIn(2, [entry.id for page in rendered for entry in page.entries])


class ImmediateProcess:
    def __init__(self, stdout: str, *, successful: bool = True, stderr: str = "") -> None:
        self.stdout = stdout
        self.successful = successful
        self.stderr = stderr

    def communicate_utf8_async(self, _stdin: None, _cancellable: None, callback: Callable[[Any, Any], None]) -> None:
        callback(self, object())

    def communicate_utf8_finish(self, _result: Any) -> tuple[bool, str, str]:
        return True, self.stdout, self.stderr

    def get_successful(self) -> bool:
        return self.successful

    def get_exit_status(self) -> int:
        return 1


class ImmediateLauncher:
    def __init__(self, process: ImmediateProcess) -> None:
        self.process = process

    def spawnv(self, _args: list[str]) -> ImmediateProcess:
        return self.process


class CliClientTests(unittest.TestCase):
    def client_for(self, stdout: str) -> cv.CliClient:
        client = cv.CliClient("test-client")
        launcher = ImmediateLauncher(ImmediateProcess(stdout))
        client._launcher = lambda _flags: launcher  # type: ignore[method-assign]
        return client

    def test_consumer_callback_exception_is_not_caught_or_repeated(self) -> None:
        client = self.client_for(json.dumps(settings_json()))
        calls = 0

        def callback(_value: Any | None, _error: Exception | None) -> None:
            nonlocal calls
            calls += 1
            raise RuntimeError("consumer failed")

        with self.assertRaisesRegex(RuntimeError, "consumer failed"):
            client.settings_get(callback)
        self.assertEqual(calls, 1)

    def test_deeply_nested_json_recursion_is_reported_once(self) -> None:
        client = self.client_for("[malformed deeply nested JSON]")
        results: list[tuple[Any | None, Exception | None]] = []
        with mock.patch.object(cv.json, "loads", side_effect=RecursionError("maximum recursion depth exceeded")):
            client.settings_get(lambda value, error: results.append((value, error)))
        self.assertEqual(len(results), 1)
        self.assertIsNone(results[0][0])
        self.assertIsInstance(results[0][1], RuntimeError)

    def test_preview_exit_during_startup_reports_failure_once(self) -> None:
        process = mock.Mock()
        process.get_identifier.return_value = "999999999"
        launcher = mock.Mock()
        launcher.spawnv.return_value = process
        client = cv.CliClient("test-client")
        client._launcher = lambda _flags: launcher  # type: ignore[method-assign]
        results: list[tuple[None, Exception | None]] = []
        closed: list[bool] = []
        client.show_preview(lambda value, error: results.append((value, error)), lambda: closed.append(True))
        client._preview_exited(process)
        self.assertEqual(len(results), 1)
        self.assertIsInstance(results[0][1], RuntimeError)
        self.assertEqual(closed, [True])

    def test_close_timeout_signals_wrapper_and_waits_for_actual_exit(self) -> None:
        process = mock.Mock()
        client = cv.CliClient("test-client")
        client.preview_process = process
        client._run = lambda _args, _parser, callback: callback(None, None)  # type: ignore[method-assign]
        timers: list[tuple[int, Callable[[], bool]]] = []
        results: list[tuple[None, Exception | None]] = []

        def timeout_add(delay: int, callback: Callable[[], bool]) -> int:
            timers.append((delay, callback))
            return len(timers)

        with mock.patch.object(cv.GLib, "timeout_add", side_effect=timeout_add), mock.patch.object(cv.GLib, "source_remove"):
            client.close_preview(lambda value, error: results.append((value, error)))
            self.assertEqual(timers[0][0], 5000)
            timers[0][1]()
            process.send_signal.assert_called_once_with(15)
            self.assertEqual(results, [])
            self.assertIs(client.preview_process, process)
            client._preview_exited(process)
        self.assertEqual(results, [(None, None)])


class SettingsWindowLifecycleTests(unittest.TestCase):
    def test_close_waits_for_preview_once_before_destroying(self) -> None:
        class WindowHarness:
            _preview_closed_for_window = cv.SettingsWindow._preview_closed_for_window

        window = WindowHarness()
        window.destroying = False
        window.closing_window = False
        window.preview_button = mock.Mock()
        window.destroy = mock.Mock()
        window.show_error = mock.Mock()
        callbacks: list[Callable[[None, Exception | None], None]] = []
        window.client = mock.Mock()
        window.client.preview_process = object()
        window.client.close_preview.side_effect = callbacks.append

        self.assertTrue(cv.SettingsWindow._close_request(window))
        self.assertTrue(cv.SettingsWindow._close_request(window))
        window.client.close_preview.assert_called_once()
        window.destroy.assert_not_called()
        callbacks.pop()(None, None)
        window.destroy.assert_called_once()
        self.assertTrue(window.destroying)


class CliIntegrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.binary = os.environ.get("CODEX_VOICE_TEST_BIN")
        if not cls.binary:
            raise unittest.SkipTest("CODEX_VOICE_TEST_BIN is not set")
        if not Path(cls.binary).is_file():
            raise AssertionError(f"CODEX_VOICE_TEST_BIN is missing: {cls.binary}")
        cls.root = Path(tempfile.mkdtemp(prefix="gtk-settings-test-", dir=Path.cwd() / "tmp"))
        for directory in ("schemas", "config", "data", "runtime", "bin"):
            (cls.root / directory).mkdir()
        shutil.copy(Path.cwd() / "schemas/io.github.andy_spike.CodexVoice.gschema.xml", cls.root / "schemas/io.github.andy_spike.CodexVoice.gschema.xml")
        subprocess.run(["glib-compile-schemas", str(cls.root / "schemas")], check=True)
        cls._fake("arecord", "#!/usr/bin/env sh\nfor output; do :; done\nprintf x > \"$output\"\nwhile :; do sleep 1; done\n")
        cls._fake("codex-asr", "#!/usr/bin/env sh\nprintf '%s\\n' 'End to end dictated text'\n")
        cls._fake("wl-copy", "#!/usr/bin/env sh\ncat > \"$CODEX_VOICE_TEST_CLIPBOARD\"\n")
        cls._fake("xclip", "#!/usr/bin/env sh\ncat > \"$CODEX_VOICE_TEST_CLIPBOARD\"\n")
        cls._fake(
            "ydotool",
            "#!/usr/bin/env sh\n"
            "if [ \"${1:-}\" = --help ]; then\n"
            "  printf 'Available commands:\\n  click\\n  key\\n  debug\\n  bakers\\n'\n"
            "  exit 0\n"
            "fi\n"
            "printf '%s\\n' \"$*\" >> \"$CODEX_VOICE_TEST_YDOTOOL\"\n"
            "sleep 0.1\n",
        )
        cls.ydotool_socket = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
        cls.ydotool_socket.bind(str(cls.root / "runtime" / ".ydotool_socket"))
        cls.overlay = cls.root / "overlay.py"
        cls.overlay.write_text("#!/usr/bin/env python3\nimport time\ntime.sleep(60)\n")
        cls.overlay.chmod(0o755)
        cls.environment = dict(os.environ, PATH=f"{cls.root / 'bin'}:{os.environ.get('PATH', '')}", GSETTINGS_SCHEMA_DIR=str(cls.root / "schemas"), GSETTINGS_BACKEND="keyfile", XDG_CONFIG_HOME=str(cls.root / "config"), XDG_DATA_HOME=str(cls.root / "data"), XDG_RUNTIME_DIR=str(cls.root / "runtime"), CODEX_VOICE_OVERLAY=str(cls.overlay), CODEX_VOICE_TEST_CLIPBOARD=str(cls.root / "clipboard"), CODEX_VOICE_TEST_YDOTOOL=str(cls.root / "ydotool-calls"))

    @classmethod
    def _fake(cls, name: str, content: str) -> None:
        script = cls.root / "bin" / name
        script.write_text(content); script.chmod(0o755)

    @classmethod
    def tearDownClass(cls) -> None:
        if hasattr(cls, "root"):
            cls.ydotool_socket.close()
            shutil.rmtree(cls.root)

    def wait_for(self, start: Callable[[Callable[[Any | None, Exception | None], None]], None]) -> Any:
        results: list[tuple[Any | None, Exception | None]] = []
        start(lambda value, error: results.append((value, error)))
        deadline = time.monotonic() + 10
        context = GLib.MainContext.default()
        while not results and time.monotonic() < deadline:
            context.iteration(True)
        self.assertTrue(results, "asynchronous CLI request timed out")
        value, error = results[0]
        if error:
            self.fail(str(error))
        return value

    def command(self, *args: str) -> subprocess.CompletedProcess[str]:
        result = subprocess.run([self.binary, *args], env=self.environment, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result

    def test_real_cli_settings_status_preview_history_and_successful_paste(self) -> None:
        client = cv.CliClient(self.binary, self.environment)
        settings = self.wait_for(client.settings_get)
        self.assertTrue(settings.enabled)
        updated = self.wait_for(lambda done: client.settings_set("enabled", False, done))
        self.assertFalse(updated.enabled)
        updated = self.wait_for(lambda done: client.settings_set("language", "es-mx", done))
        self.assertEqual(updated.language, "es-mx")
        self.command("settings", "set", "enabled", "true")
        self.assertTrue(self.wait_for(client.settings_get).enabled)
        self.assertIsInstance(self.wait_for(client.version), str)
        self.assertEqual(self.wait_for(client.status).schema_version, 1)
        self.wait_for(lambda done: client.show_preview(done, lambda: None))
        self.wait_for(client.close_preview)
        self.command("--start")
        self.command("--stop")
        self.assertEqual(
            (self.root / "ydotool-calls").read_text().splitlines(),
            ["key 42:1 110:1 110:0 42:0"],
        )
        page = self.wait_for(lambda done: client.load_history(0, 50, "end TO END", done))
        self.assertEqual([entry.text for entry in page.entries], ["End to end dictated text"])
        self.wait_for(lambda done: client.delete_history(page.entries[0].id, done))
        self.assertEqual(self.wait_for(lambda done: client.load_history(0, 50, "", done)).entries, [])
        self.command("--start"); self.command("--stop")
        self.wait_for(client.clear_history)
        self.assertEqual(self.wait_for(lambda done: client.load_history(0, 50, "", done)).entries, [])
        reset = self.wait_for(client.settings_reset)
        self.assertEqual(reset.language, "auto")


if __name__ == "__main__":
    unittest.main()
