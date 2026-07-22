#!/usr/bin/env python3
"""Native libadwaita settings adapter for Codex Voice.

The Rust CLI remains the authority for settings validation and GNOME shortcut
synchronisation. This adapter only renders and coordinates that public CLI.
"""
from __future__ import annotations

import json
import os
import signal
import sys
from dataclasses import dataclass, replace
from datetime import datetime
from pathlib import Path
from typing import Any, Callable, Mapping, Sequence

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, Gdk, Gio, GLib, Gtk  # noqa: E402

SCHEMA_ID = "io.github.andy_spike.CodexVoice"
DEFAULT_KEYBINDING = "<Control><Super>space"
SETTINGS_KEYS = ("enabled", "show-tray-icon", "keybinding", "language")
BOOLEAN_SETTINGS_KEYS = frozenset(("enabled", "show-tray-icon"))
HISTORY_LIMIT = 50

LANGUAGES: tuple[tuple[str, str], ...] = (
    ("auto", "Automatic detection"), ("en", "English"), ("en-us", "English (United States)"),
    ("en-gb", "English (United Kingdom)"), ("es", "Spanish"), ("es-es", "Spanish (Spain)"),
    ("es-mx", "Spanish (Mexico)"), ("fr", "French"), ("fr-ca", "French (Canada)"),
    ("de", "German"), ("it", "Italian"), ("pt", "Portuguese"), ("pt-br", "Portuguese (Brazil)"),
    ("nl", "Dutch"), ("pl", "Polish"), ("ru", "Russian"), ("uk", "Ukrainian"),
    ("tr", "Turkish"), ("ar", "Arabic"), ("hi", "Hindi"), ("id", "Indonesian"),
    ("ja", "Japanese"), ("ko", "Korean"), ("zh", "Chinese"), ("zh-cn", "Chinese (Simplified)"),
    ("zh-tw", "Chinese (Traditional)"),
)


class ProtocolError(ValueError):
    """The CLI returned an incompatible Desktop Protocol document."""


def _object(value: Any, name: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ProtocolError(f"{name} must be an object")
    return value


def _string(value: Any, name: str) -> str:
    if not isinstance(value, str):
        raise ProtocolError(f"{name} must be a string")
    return value


def _bool(value: Any, name: str) -> bool:
    if type(value) is not bool:
        raise ProtocolError(f"{name} must be a boolean")
    return value


def _integer(value: Any, name: str, *, positive: bool = False) -> int:
    if type(value) is not int or not -(2**53 - 1) <= value <= 2**53 - 1:
        raise ProtocolError(f"{name} must be a safe integer")
    if positive and value <= 0:
        raise ProtocolError(f"{name} must be positive")
    return value


def _version(document: Mapping[str, Any]) -> int:
    version = _integer(document.get("schemaVersion"), "schemaVersion", positive=True)
    if version != 1:
        raise ProtocolError(f"Unsupported schema version {version}")
    return version


@dataclass(frozen=True)
class SettingsDocument:
    schema_version: int
    enabled: bool
    show_tray_icon: bool
    keybinding: str
    language: str
    language_override: str | None


@dataclass(frozen=True)
class StatusDocument:
    schema_version: int
    state: str
    extension_active: bool
    ubuntu: str
    gnome_shell: str


@dataclass(frozen=True)
class TranscriptEntry:
    id: int
    created_at: int
    text: str


@dataclass(frozen=True)
class HistoryPage:
    schema_version: int
    entries: list[TranscriptEntry]
    has_more: bool


def parse_settings(raw: str | Mapping[str, Any]) -> SettingsDocument:
    document = json.loads(raw) if isinstance(raw, str) else raw
    value = _object(document, "settings")
    overrides = _object(value.get("overrides"), "overrides")
    language_override = overrides.get("language")
    if language_override is not None:
        language_override = _string(language_override, "overrides.language")
    return SettingsDocument(
        _version(value), _bool(value.get("enabled"), "enabled"),
        _bool(value.get("showTrayIcon"), "showTrayIcon"),
        _string(value.get("keybinding"), "keybinding"), _string(value.get("language"), "language"),
        language_override,
    )


def parse_status(raw: str | Mapping[str, Any]) -> StatusDocument:
    document = json.loads(raw) if isinstance(raw, str) else raw
    value = _object(document, "status")
    state = _string(value.get("state"), "state")
    if state not in {"idle", "recording", "transcribing", "typing"}:
        raise ProtocolError(f"Unknown status state {state}")
    return StatusDocument(_version(value), state, _bool(value.get("extensionActive"), "extensionActive"),
                          _string(value.get("ubuntu"), "ubuntu"), _string(value.get("gnomeShell"), "gnomeShell"))


def parse_history(raw: str | Mapping[str, Any]) -> HistoryPage:
    document = json.loads(raw) if isinstance(raw, str) else raw
    value = _object(document, "history")
    entries = value.get("entries")
    if not isinstance(entries, list):
        raise ProtocolError("entries must be an array")
    parsed: list[TranscriptEntry] = []
    for index, entry in enumerate(entries):
        item = _object(entry, f"entries[{index}]")
        parsed.append(TranscriptEntry(_integer(item.get("id"), "id", positive=True),
                                      _integer(item.get("createdAt"), "createdAt"), _string(item.get("text"), "text")))
    return HistoryPage(_version(value), parsed, _bool(value.get("hasMore"), "hasMore"))


def language_options(current: str) -> tuple[tuple[str, str], ...]:
    if any(code == current for code, _ in LANGUAGES):
        return LANGUAGES
    return ((current, f"Current value ({current})"),) + LANGUAGES


def setting_value(key: str, value: bool | str) -> str:
    if key not in SETTINGS_KEYS:
        raise ValueError("Unsupported settings key")
    if key in BOOLEAN_SETTINGS_KEYS:
        if type(value) is not bool:
            raise ValueError(f"{key} must be boolean")
        return "true" if value else "false"
    if type(value) is not str:
        raise ValueError(f"{key} must be a string")
    return value


def document_with(document: SettingsDocument, key: str, value: bool | str) -> SettingsDocument:
    field = {"show-tray-icon": "show_tray_icon"}.get(key, key)
    return replace(document, **{field: value})


def shortcut_from_key(keyval: int, state: Gdk.ModifierType) -> tuple[str | None, str | None]:
    """Return (accelerator, error) without accepting keys Rust will reject."""
    name = Gdk.keyval_name(keyval)
    if name in {"Control_L", "Control_R", "Alt_L", "Alt_R", "Meta_L", "Meta_R", "Super_L", "Super_R", "Shift_L", "Shift_R"}:
        return None, None
    if name in {None, "Unidentified", "Dead"}:
        return None, "That key could not be identified. Try the shortcut again."
    if name in {"Escape", "BackSpace", "Delete"}:
        return name, None
    supported_function = name.startswith("F") and name[1:].isdigit() and 1 <= int(name[1:]) <= 35
    if name == "space":
        rust_key = "space"
    elif len(name) == 1 and (name.isascii() and (name.isalnum() or name == " ")):
        rust_key = name.lower()
    elif supported_function:
        rust_key = name
    else:
        return None, "That key could not be identified. Try the shortcut again."
    mask = Gdk.ModifierType.CONTROL_MASK | Gdk.ModifierType.ALT_MASK | Gdk.ModifierType.SUPER_MASK | Gdk.ModifierType.SHIFT_MASK
    masked = state & mask
    has_required_modifier = bool(masked & (Gdk.ModifierType.CONTROL_MASK | Gdk.ModifierType.ALT_MASK | Gdk.ModifierType.SUPER_MASK))
    if not has_required_modifier and not supported_function:
        return None, "Use Ctrl, Alt, Super, or a function key with a non-modifier key."
    if not Gtk.accelerator_name(keyval, masked):
        return None, "That key could not be identified. Try the shortcut again."
    modifiers = "".join(
        token for bit, token in (
            (Gdk.ModifierType.CONTROL_MASK, "<Control>"),
            (Gdk.ModifierType.ALT_MASK, "<Alt>"),
            (Gdk.ModifierType.SUPER_MASK, "<Super>"),
            (Gdk.ModifierType.SHIFT_MASK, "<Shift>"),
        ) if masked & bit
    )
    return f"{modifiers}{rust_key}", None


class CliClient:
    def __init__(self, executable: str | None = None, environment: Mapping[str, str] | None = None):
        self.executable = executable or os.environ.get("CODEX_VOICE_BIN", "codex-voice")
        self.environment = dict(environment) if environment is not None else None
        self.preview_process: Gio.Subprocess | None = None
        self._preview_closed: Callable[[], None] | None = None
        self._preview_waiters: list[Callable[[], None]] = []
        self._preview_pid: int | None = None
        self._preview_watch: int | None = None
        self._preview_start_source: int | None = None
        self._preview_start_callback: Callable[[None, Exception | None], None] | None = None

    def _launcher(self, flags: Gio.SubprocessFlags) -> Gio.SubprocessLauncher:
        launcher = Gio.SubprocessLauncher.new(flags)
        if self.environment is not None:
            launcher.set_environ([f"{key}={value}" for key, value in self.environment.items()])
        return launcher

    def _preview_exited(self, process: Gio.Subprocess) -> None:
        if self.preview_process is not process:
            return
        start_callback, self._preview_start_callback = self._preview_start_callback, None
        closed, self._preview_closed = self._preview_closed, None
        self.preview_process = None
        self._preview_pid = None
        if self._preview_watch is not None:
            GLib.source_remove(self._preview_watch)
            self._preview_watch = None
        if self._preview_start_source is not None:
            GLib.source_remove(self._preview_start_source)
            self._preview_start_source = None
        waiters, self._preview_waiters = self._preview_waiters, []
        if start_callback:
            start_callback(None, RuntimeError("Preview exited before it was ready"))
        if closed:
            closed()
        for waiter in waiters:
            waiter()

    @staticmethod
    def _preview_alive(pid: int) -> bool:
        try:
            return Path(f"/proc/{pid}/stat").read_text().split()[2] != "Z"
        except FileNotFoundError:
            return False
        except (OSError, IndexError):
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                return False
            except PermissionError:
                return True
            return True

    def _watch_preview(self) -> bool:
        process, pid = self.preview_process, self._preview_pid
        if process is None or pid is None:
            self._preview_watch = None
            return GLib.SOURCE_REMOVE
        if not self._preview_alive(pid):
            self._preview_watch = None
            self._preview_exited(process)
            return GLib.SOURCE_REMOVE
        return GLib.SOURCE_CONTINUE

    def _run(self, args: Sequence[str], parser: Callable[[str], Any] | None, callback: Callable[[Any | None, Exception | None], None]) -> None:
        completed = False
        consumer_exception: BaseException | None = None

        def deliver(value: Any | None, error: Exception | None) -> None:
            nonlocal completed, consumer_exception
            if completed:
                return
            completed = True
            try:
                callback(value, error)
            except BaseException as raised:
                consumer_exception = raised
                raise

        try:
            process = self._launcher(Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE).spawnv([self.executable, *args])
        except GLib.Error as error:
            deliver(None, RuntimeError(str(error)))
            return

        def finished(child: Gio.Subprocess, result: Gio.AsyncResult) -> None:
            try:
                _, stdout, stderr = child.communicate_utf8_finish(result)
                if not child.get_successful():
                    message = (stderr or "").strip() or f"codex-voice exited with status {child.get_exit_status()}"
                    raise RuntimeError(message)
                value = parser(stdout or "") if parser else None
            except (GLib.Error, ProtocolError, ValueError, RuntimeError, RecursionError) as error:
                deliver(None, RuntimeError(str(error)))
                return
            deliver(value, None)

        try:
            process.communicate_utf8_async(None, None, finished)
        except GLib.Error as error:
            if error is consumer_exception:
                raise
            deliver(None, RuntimeError(str(error)))

    def settings_get(self, callback: Callable[[SettingsDocument | None, Exception | None], None]) -> None:
        self._run(("settings", "get"), parse_settings, callback)

    def settings_set(self, key: str, value: bool | str, callback: Callable[[SettingsDocument | None, Exception | None], None]) -> None:
        try:
            encoded = setting_value(key, value)
        except ValueError as error:
            callback(None, error)
            return
        self._run(("settings", "set", key, encoded), parse_settings, callback)

    def settings_reset(self, callback: Callable[[SettingsDocument | None, Exception | None], None]) -> None:
        self._run(("settings", "reset"), parse_settings, callback)

    def load_history(self, offset: int, limit: int, query: str, callback: Callable[[HistoryPage | None, Exception | None], None]) -> None:
        if type(offset) is not int or type(limit) is not int or offset < 0 or not 1 <= limit <= 100 or type(query) is not str:
            callback(None, ValueError("Invalid history query"))
            return
        self._run(("history", "list", str(offset), str(limit), query), parse_history, callback)

    def delete_history(self, entry_id: int, callback: Callable[[None, Exception | None], None]) -> None:
        if type(entry_id) is not int or entry_id <= 0:
            callback(None, ValueError("Invalid transcript id"))
            return
        self._run(("history", "delete", str(entry_id)), None, callback)

    def clear_history(self, callback: Callable[[None, Exception | None], None]) -> None:
        self._run(("history", "clear"), None, callback)

    def version(self, callback: Callable[[str | None, Exception | None], None]) -> None:
        self._run(("--version",), lambda text: text.strip(), callback)

    def status(self, callback: Callable[[StatusDocument | None, Exception | None], None]) -> None:
        self._run(("--status",), parse_status, callback)

    def show_preview(self, callback: Callable[[None, Exception | None], None], closed: Callable[[], None]) -> None:
        if self.preview_process is not None:
            callback(None, RuntimeError("Preview is already open"))
            return
        try:
            child = self._launcher(Gio.SubprocessFlags.INHERIT_FDS).spawnv([self.executable, "--preview"])
        except GLib.Error as error:
            callback(None, RuntimeError(str(error)))
            return
        self.preview_process, self._preview_closed, self._preview_pid = child, closed, int(child.get_identifier())
        self._preview_start_callback = callback

        self._preview_watch = GLib.timeout_add(100, self._watch_preview)

        def confirm_started() -> bool:
            self._preview_start_source = None
            if self.preview_process is not child or self._preview_start_callback is not callback:
                return GLib.SOURCE_REMOVE
            if self._preview_pid is None or not self._preview_alive(self._preview_pid):
                self._preview_exited(child)
                return GLib.SOURCE_REMOVE
            self._preview_start_callback = None
            callback(None, None)
            return GLib.SOURCE_REMOVE

        self._preview_start_source = GLib.timeout_add(250, confirm_started)

    def close_preview(self, callback: Callable[[None, Exception | None], None]) -> None:
        owned = self.preview_process
        if owned is None:
            callback(None, None)
            return

        completed = False
        timeout_id: int | None = None
        force_id: int | None = None

        def finish(error: Exception | None = None) -> None:
            nonlocal completed
            if completed:
                return
            completed = True
            if timeout_id is not None:
                GLib.source_remove(timeout_id)
            if force_id is not None:
                GLib.source_remove(force_id)
            callback(None, error)

        self._preview_waiters.append(finish)

        def force_shutdown() -> bool:
            nonlocal force_id
            force_id = None
            if self.preview_process is owned:
                owned.force_exit()
            return GLib.SOURCE_REMOVE

        def request_wrapper_shutdown() -> bool:
            nonlocal force_id, timeout_id
            timeout_id = None
            if self.preview_process is not owned:
                finish()
                return GLib.SOURCE_REMOVE
            try:
                owned.send_signal(signal.SIGTERM)
            except GLib.Error:
                owned.force_exit()
            force_id = GLib.timeout_add(1000, force_shutdown)
            return GLib.SOURCE_REMOVE

        def after_close(_: Any | None, error: Exception | None) -> None:
            nonlocal timeout_id
            if completed:
                return
            if self.preview_process is not owned:
                finish()
                return
            if error:
                request_wrapper_shutdown()
                return
            timeout_id = GLib.timeout_add(5000, request_wrapper_shutdown)

        self._run(("--close-preview",), None, after_close)


class SettingsCoordinator:
    def __init__(self, client: CliClient, schema: Gio.Settings | None, on_document: Callable[[SettingsDocument], None], on_error: Callable[[str], None]):
        self.client, self.schema, self.on_document, self.on_error = client, schema, on_document, on_error
        self.document: SettingsDocument | None = None
        self.pending: dict[str, tuple[int, bool | str]] = {}
        self.write_queue: list[str] = []
        self.active_write: tuple[str, int, bool | str] | None = None
        self.next_token = 0
        self.reset_requested: int | None = None
        self.reset_running: int | None = None
        self.mutation_generation = 0
        self.reload_running = False
        self.reload_queued = False
        self.debounce_source: int | None = None
        if schema is not None:
            schema.connect("changed", self._schema_changed)

    def _merged(self, document: SettingsDocument) -> SettingsDocument:
        for key, (_, value) in self.pending.items():
            document = document_with(document, key, value)
        return document

    def _render(self) -> None:
        if self.document is not None:
            self.on_document(self._merged(self.document))

    def load(self) -> None:
        if self.active_write is not None or self.write_queue or self.reset_requested is not None or self.reset_running is not None:
            self.reload_queued = True
            return
        if self.reload_running:
            self.reload_queued = True
            return
        self.reload_running = True
        generation = self.mutation_generation
        def done(document: SettingsDocument | None, error: Exception | None) -> None:
            self.reload_running = False
            if generation != self.mutation_generation:
                pass
            elif error:
                self.on_error(str(error))
            elif document:
                self.document = document
                self._render()
            if self.reload_queued:
                self.reload_queued = False
                self.load()
        self.client.settings_get(done)

    def save(self, key: str, value: bool | str) -> None:
        if self.document is None:
            return
        try:
            setting_value(key, value)
        except ValueError as error:
            self.on_error(str(error))
            return
        self.next_token += 1
        token = self.next_token
        self.mutation_generation += 1
        self.pending[key] = (token, value)
        if key in self.write_queue:
            self.write_queue.remove(key)
        self.write_queue.append(key)
        self._render()

        self._advance()

    def _start_write(self, key: str, token: int, value: bool | str) -> None:
        self.active_write = (key, token, value)

        def done(document: SettingsDocument | None, error: Exception | None) -> None:
            if self.active_write != (key, token, value):
                return
            self.active_write = None
            latest = self.pending.get(key)
            if latest and latest[0] == token:
                del self.pending[key]
            reset_pending = self.reset_requested is not None or self.reset_running is not None
            if error and latest and latest[0] == token and not reset_pending:
                self.on_error(str(error))
                self.reload_queued = True
            elif document and not reset_pending:
                self.document = document
                self._render()
            self._advance()
        self.client.settings_set(key, value, done)

    def reset(self) -> None:
        self.next_token += 1
        self.mutation_generation += 1
        self.pending.clear()
        self.write_queue.clear()
        self.reset_requested = self.next_token
        self._render()
        self._advance()

    def _start_reset(self, token: int) -> None:
        self.reset_running = token

        def done(document: SettingsDocument | None, error: Exception | None) -> None:
            if self.reset_running != token:
                return
            self.reset_running = None
            superseded = self.reset_requested is not None
            if error and not superseded:
                self.on_error(str(error))
                self.reload_queued = True
            elif document and not superseded:
                self.document = document
                self._render()
            self._advance()
        self.client.settings_reset(done)

    def _advance(self) -> None:
        if self.active_write is not None or self.reset_running is not None:
            return
        if self.reset_requested is not None:
            token, self.reset_requested = self.reset_requested, None
            self._start_reset(token)
            return
        while self.write_queue:
            key = self.write_queue.pop(0)
            latest = self.pending.get(key)
            if latest is not None:
                self._start_write(key, latest[0], latest[1])
                return
        if self.reload_queued and not self.reload_running:
            self.reload_queued = False
            self.load()

    def _schema_changed(self, *_: object) -> None:
        if self.debounce_source is not None:
            GLib.source_remove(self.debounce_source)
        self.debounce_source = GLib.timeout_add(50, self._reload_after_change)

    def _reload_after_change(self) -> bool:
        self.debounce_source = None
        self.load()
        return GLib.SOURCE_REMOVE


def _schema_settings() -> Gio.Settings | None:
    parent = Gio.SettingsSchemaSource.get_default()
    candidates: list[str] = []
    candidates.extend(part for part in os.environ.get("GSETTINGS_SCHEMA_DIR", "").split(os.pathsep) if part)
    candidates.extend((str(Path.home() / ".local/share/codex-voice/schemas"), "/usr/share/glib-2.0/schemas"))
    seen: set[str] = set()
    for directory in reversed(candidates):
        path = Path(directory)
        normalized = str(path.resolve()) if path.exists() else str(path)
        if normalized in seen or not (path / "gschemas.compiled").exists():
            continue
        seen.add(normalized)
        try:
            parent = Gio.SettingsSchemaSource.new_from_directory(str(path), parent, False)
        except GLib.Error:
            continue
    if parent is None:
        return None
    schema = parent.lookup(SCHEMA_ID, True)
    return Gio.Settings.new_full(schema, None, None) if schema is not None else None


class HistoryCoordinator:
    def __init__(self, client: CliClient, on_page: Callable[[HistoryPage], None], on_error: Callable[[str], None]):
        self.client, self.on_page, self.on_error = client, on_page, on_error
        self.entries: list[TranscriptEntry] = []
        self.has_more = False
        self.history_exists = False
        self.query = ""
        self.request = 0
        self.debounce_source: int | None = None

    def search(self, query: str) -> None:
        self.query = query
        if self.debounce_source is not None:
            GLib.source_remove(self.debounce_source)
        self.debounce_source = GLib.timeout_add(250, self._debounced_search)

    def _debounced_search(self) -> bool:
        self.debounce_source = None
        self.refresh()
        return GLib.SOURCE_REMOVE

    def refresh(self) -> None:
        self._load(False)

    def load_more(self) -> None:
        if self.has_more:
            self._load(True)

    def _load(self, append: bool) -> None:
        self.request += 1
        request = self.request
        offset = len(self.entries) if append else 0
        def done(page: HistoryPage | None, error: Exception | None) -> None:
            if request != self.request:
                return
            if error:
                self.on_error(str(error))
                return
            assert page is not None
            self.entries = self.entries + page.entries if append else page.entries
            self.has_more = page.has_more
            if not append and not self.query:
                self.history_exists = bool(page.entries)
            self.on_page(HistoryPage(page.schema_version, list(self.entries), self.has_more))
        self.client.load_history(offset, HISTORY_LIMIT, self.query, done)

    def delete(self, entry_id: int) -> None:
        self.request += 1
        def done(_: None, error: Exception | None) -> None:
            if error:
                self.on_error(str(error))
                return
            self.entries = [entry for entry in self.entries if entry.id != entry_id]
            if not self.query and not self.entries and not self.has_more:
                self.history_exists = False
            self.on_page(HistoryPage(1, list(self.entries), self.has_more))
        self.client.delete_history(entry_id, done)

    def clear(self) -> None:
        self.request += 1
        def done(_: None, error: Exception | None) -> None:
            if error:
                self.on_error(str(error))
                return
            self.entries, self.has_more, self.history_exists = [], False, False
            self.on_page(HistoryPage(1, [], False))
        self.client.clear_history(done)


class SettingsWindow(Adw.ApplicationWindow):
    def __init__(self, application: Adw.Application):
        super().__init__(application=application, title="Codex Voice Settings", default_width=820, default_height=740)
        self.set_size_request(680, 560)
        self.client = CliClient()
        self.applying = False
        self.capturing = False
        self.preview_state = "closed"
        self.closing_window = False
        self.destroying = False
        self.banner = Adw.Banner.new("")
        self.banner.set_revealed(False)
        self.banner.set_button_label("Retry")
        self.banner.connect("button-clicked", lambda *_: self.retry())
        self.schema = _schema_settings()
        self.coordinator = SettingsCoordinator(self.client, self.schema, self.apply_document, self.show_error)
        self.history = HistoryCoordinator(self.client, self.apply_history, self.show_error)
        self._build()
        self._probe_info()
        self.coordinator.load()
        self.history.refresh()
        self.connect("close-request", self._close_request)

    def _build(self) -> None:
        header = Adw.HeaderBar.new()
        self.stack = Adw.ViewStack.new()
        self.stack.set_vexpand(True)
        self.overlay = Adw.ToastOverlay.new()

        switcher = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        switcher.add_css_class("linked")
        switcher.add_css_class("settings-switcher")
        self.page_buttons: dict[str, Gtk.ToggleButton] = {}
        previous: Gtk.ToggleButton | None = None
        for name, title in (("general", "General"), ("transcriptions", "Transcriptions")):
            button = Gtk.ToggleButton.new_with_label(title)
            if previous is not None:
                button.set_group(previous)
            button.connect("toggled", self._select_page, name)
            switcher.append(button)
            self.page_buttons[name] = button
            previous = button
        header.set_title_widget(switcher)

        self.stack.add_titled(self._general_page(), "general", "General")
        self.stack.add_titled(self._history_page(), "transcriptions", "Transcriptions")
        self.page_buttons["general"].set_active(True)
        self.stack.connect("notify::visible-child-name", self._page_changed)

        content = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        content.append(self.banner)
        content.append(self.stack)
        self.overlay.set_child(content)
        toolbar = Adw.ToolbarView.new()
        toolbar.add_top_bar(header)
        toolbar.set_content(self.overlay)
        self.set_content(toolbar)

    def _select_page(self, button: Gtk.ToggleButton, name: str) -> None:
        if button.get_active():
            self.stack.set_visible_child_name(name)

    def _general_page(self) -> Adw.PreferencesPage:
        page = Adw.PreferencesPage.new()

        behavior = Adw.PreferencesGroup.new()
        behavior.set_title("Behavior")
        page.add(behavior)
        self.enabled_row = Adw.SwitchRow.new()
        self.enabled_row.set_title("Dictation")
        self.enabled_row.set_subtitle("Listen for the global shortcut")
        self.enabled_row.connect("notify::active", self._switch_changed, "enabled")
        behavior.add(self.enabled_row)
        self.tray_row = Adw.SwitchRow.new()
        self.tray_row.set_title("Show top-bar icon")
        self.tray_row.set_subtitle("Show Codex Voice controls in the GNOME top bar")
        self.tray_row.connect("notify::active", self._switch_changed, "show-tray-icon")
        behavior.add(self.tray_row)

        input_group = Adw.PreferencesGroup.new()
        input_group.set_title("Input")
        page.add(input_group)
        shortcut = Adw.ActionRow.new()
        shortcut.set_title("Keyboard shortcut")
        shortcut.set_subtitle("Press Escape to cancel")
        self.shortcut_button = Gtk.Button.new()
        self.shortcut_button.set_valign(Gtk.Align.CENTER)
        self.shortcut_label = Gtk.ShortcutLabel.new("")
        self.shortcut_button.set_child(self.shortcut_label)
        self.shortcut_button.connect("clicked", self._begin_capture)
        key_controller = Gtk.EventControllerKey.new()
        key_controller.set_propagation_phase(Gtk.PropagationPhase.CAPTURE)
        key_controller.connect("key-pressed", self._shortcut_key_pressed)
        self.shortcut_button.add_controller(key_controller)
        shortcut.add_suffix(self.shortcut_button)
        input_group.add(shortcut)
        self.language_model = Gtk.StringList.new([])
        self.language_codes: list[str] = []
        self.language_row = Adw.ComboRow.new()
        self.language_row.set_title("Language")
        self.language_row.set_subtitle("Automatic works for most dictation")
        self.language_row.set_model(self.language_model)
        self.language_row.connect("notify::selected", self._language_changed)
        input_group.add(self.language_row)

        appearance = Adw.PreferencesGroup.new()
        appearance.set_title("Appearance")
        page.add(appearance)
        preview = Adw.ActionRow.new()
        preview.set_title("Recording pill")
        preview.set_subtitle("Shown while Codex Voice is listening")
        self.preview_button = Gtk.Button.new_with_label("Show live preview")
        self.preview_button.set_valign(Gtk.Align.CENTER)
        self.preview_button.connect("clicked", self._toggle_preview)
        preview.add_suffix(self.preview_button)
        appearance.add(preview)

        advanced_group = Adw.PreferencesGroup.new()
        advanced_group.set_description("Changes are saved automatically")
        page.add(advanced_group)
        self.advanced = Adw.ExpanderRow.new()
        self.advanced.set_title("Advanced")
        self.advanced.set_subtitle("Runtime information and reset controls")
        self.override_row = Adw.ActionRow.new()
        self.override_row.set_title("Language override")
        self.override_row.set_visible(False)
        self.runtime_row = Adw.ActionRow.new()
        self.runtime_row.set_title("Runtime")
        self.runtime_row.set_subtitle("CLI Unavailable · Ubuntu Unavailable · GNOME Unavailable")
        self.extension_row = Adw.ActionRow.new()
        self.extension_row.set_title("GNOME extension")
        self.extension_row.set_subtitle("Unavailable")
        reset_row = Adw.ActionRow.new()
        reset_row.set_title("Reset settings")
        reset_row.set_subtitle("Restore the default shortcut, language, and preferences")
        self.reset_button = Gtk.Button.new_with_label("Reset settings")
        self.reset_button.set_valign(Gtk.Align.CENTER)
        self.reset_button.add_css_class("destructive-action")
        self.reset_button.connect("clicked", self._confirm_reset)
        reset_row.add_suffix(self.reset_button)
        self.advanced.add_row(self.override_row)
        self.advanced.add_row(self.runtime_row)
        self.advanced.add_row(self.extension_row)
        self.advanced.add_row(reset_row)
        advanced_group.add(self.advanced)
        return page

    def _history_page(self) -> Gtk.ScrolledWindow:
        scroll = Gtk.ScrolledWindow.new()
        clamp = Adw.Clamp.new()
        clamp.set_maximum_size(680)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=16)
        box.set_margin_top(24)
        box.set_margin_bottom(24)
        box.set_margin_start(18)
        box.set_margin_end(18)

        title_box = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=12)
        heading = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=2)
        heading.set_hexpand(True)
        title = Gtk.Label.new("Transcript history")
        title.add_css_class("title-2")
        title.set_xalign(0)
        subtitle = Gtk.Label.new("Saved on this device until you delete it")
        subtitle.add_css_class("dim-label")
        subtitle.set_xalign(0)
        heading.append(title)
        heading.append(subtitle)
        self.clear_button = Gtk.Button.new_with_label("Clear history")
        self.clear_button.set_valign(Gtk.Align.CENTER)
        self.clear_button.add_css_class("destructive-action")
        self.clear_button.connect("clicked", self._confirm_clear)
        title_box.append(heading)
        title_box.append(self.clear_button)
        box.append(title_box)

        self.search_entry = Gtk.SearchEntry.new()
        self.search_entry.set_placeholder_text("Search transcripts")
        self.search_entry.connect("search-changed", lambda entry: self.history.search(entry.get_text()))
        box.append(self.search_entry)
        self.history_list = Gtk.ListBox.new()
        self.history_list.add_css_class("boxed-list")
        self.history_list.set_selection_mode(Gtk.SelectionMode.NONE)
        box.append(self.history_list)
        self.empty_label = Gtk.Label.new("No transcripts yet")
        self.empty_label.add_css_class("dim-label")
        self.empty_label.set_margin_top(36)
        self.empty_label.set_margin_bottom(36)
        self.empty_label.set_visible(False)
        box.append(self.empty_label)
        self.load_more = Gtk.Button.new_with_label("Load more")
        self.load_more.set_halign(Gtk.Align.CENTER)
        self.load_more.connect("clicked", lambda *_: self.history.load_more())
        self.load_more.set_visible(False)
        box.append(self.load_more)
        clamp.set_child(box)
        scroll.set_child(clamp)
        return scroll

    def retry(self) -> None:
        if self.schema is None:
            self.schema = _schema_settings()
            self.coordinator.schema = self.schema
            if self.schema:
                self.schema.connect("changed", self.coordinator._schema_changed)
        if self.schema is None:
            self.show_error("Codex Voice GSettings schema is unavailable")
            return
        self.banner.set_revealed(False)
        self.coordinator.load()

    def show_error(self, message: str) -> None:
        if self.destroying:
            return
        self.banner.set_title(message)
        self.banner.set_revealed(True)

    def _switch_changed(self, row: Adw.SwitchRow, _: GParamSpec, key: str) -> None:  # type: ignore[name-defined]
        if not self.applying:
            self.coordinator.save(key, row.get_active())

    def _language_changed(self, row: Adw.ComboRow, _: GParamSpec) -> None:  # type: ignore[name-defined]
        if self.applying or row.get_selected() == Gtk.INVALID_LIST_POSITION:
            return
        selected = row.get_selected()
        if selected < len(self.language_codes):
            self.coordinator.save("language", self.language_codes[selected])

    def apply_document(self, document: SettingsDocument) -> None:
        if self.destroying:
            return
        self.applying = True
        self.enabled_row.set_active(document.enabled)
        self.tray_row.set_active(document.show_tray_icon)
        self.shortcut_label.set_accelerator(document.keybinding)
        options = language_options(document.language)
        self.language_codes = [code for code, _ in options]
        self.language_model.splice(0, self.language_model.get_n_items(), [label for _, label in options])
        for position, (code, _) in enumerate(options):
            if code == document.language:
                self.language_row.set_selected(position)
                break
        override = document.language_override
        self.override_row.set_subtitle(f"CODEX_VOICE_LANG={override}" if override else "")
        self.override_row.set_visible(override is not None)
        self.applying = False
        self.banner.set_revealed(False)

    def _begin_capture(self, *_: object) -> None:
        self.capturing = True
        self.shortcut_button.set_label("Press keys…")
        self.shortcut_button.grab_focus()

    def _shortcut_key_pressed(self, controller: Gtk.EventControllerKey, keyval: int, keycode: int, state: Gdk.ModifierType) -> bool:
        if not self.capturing:
            return False
        accelerator, error = shortcut_from_key(keyval, state)
        if accelerator == "Escape":
            self.capturing = False
            self.shortcut_button.set_child(self.shortcut_label)
            return True
        if accelerator in {"BackSpace", "Delete"}:
            self.capturing = False
            self.shortcut_button.set_child(self.shortcut_label)
            self.coordinator.save("keybinding", DEFAULT_KEYBINDING)
            return True
        if error:
            self.show_error(error)
            return True
        if accelerator:
            self.capturing = False
            self.shortcut_button.set_child(self.shortcut_label)
            self.coordinator.save("keybinding", accelerator)
        return True

    def _toggle_preview(self, *_: object) -> None:
        if self.preview_state == "closed":
            self.preview_state = "opening"; self.preview_button.set_sensitive(False)
            self.client.show_preview(self._preview_opened, self._preview_closed)
        elif self.preview_state == "open":
            self.preview_state = "closing"; self.preview_button.set_sensitive(False)
            self.client.close_preview(self._preview_closed_request)

    def _preview_opened(self, _: None, error: Exception | None) -> None:
        if self.closing_window or self.destroying:
            return
        if error:
            self.preview_state = "closed"; self.show_error(str(error))
        else:
            self.preview_state = "open"; self.preview_button.set_label("Close preview")
        self.preview_button.set_sensitive(True)

    def _preview_closed(self) -> None:
        if self.closing_window or self.destroying:
            return
        self.preview_state = "closed"; self.preview_button.set_label("Show live preview"); self.preview_button.set_sensitive(True)

    def _preview_closed_request(self, _: None, error: Exception | None) -> None:
        if self.closing_window or self.destroying:
            return
        if error:
            self.preview_state = "open"; self.preview_button.set_label("Close preview"); self.show_error(str(error))
        else:
            self._preview_closed()
        self.preview_button.set_sensitive(True)

    def _probe_info(self) -> None:
        details: dict[str, Any] = {"version": "Unavailable", "status": None}
        def render() -> None:
            if self.destroying:
                return
            status = details["status"]
            ubuntu = status.ubuntu if status else "Unavailable"
            gnome = status.gnome_shell if status else "Unavailable"
            extension = "active" if status and status.extension_active else "inactive" if status else "Unavailable"
            self.runtime_row.set_subtitle(f"CLI {details['version']} · Ubuntu {ubuntu} · GNOME {gnome}")
            self.extension_row.set_subtitle(extension.capitalize())
        self.client.version(lambda value, error: (details.__setitem__("version", value if not error else "Unavailable"), render()))
        self.client.status(lambda value, error: (details.__setitem__("status", value if not error else None), render()))

    def _confirm_reset(self, *_: object) -> None:
        dialog = Adw.AlertDialog.new("Reset all settings?", "This restores startup, tray, shortcut, language, and enabled preferences.")
        dialog.add_response("cancel", "Cancel"); dialog.add_response("reset", "Reset settings")
        dialog.set_response_appearance("reset", Adw.ResponseAppearance.DESTRUCTIVE)
        dialog.set_default_response("cancel"); dialog.set_close_response("cancel")
        dialog.choose(self, None, lambda dialog, result: self.coordinator.reset() if dialog.choose_finish(result) == "reset" else None)

    def _confirm_clear(self, *_: object) -> None:
        dialog = Adw.AlertDialog.new("Clear transcript history?", "This permanently deletes every saved transcript. This action cannot be undone.")
        dialog.add_response("cancel", "Cancel"); dialog.add_response("clear", "Clear history")
        dialog.set_response_appearance("clear", Adw.ResponseAppearance.DESTRUCTIVE)
        dialog.set_default_response("cancel"); dialog.set_close_response("cancel")
        dialog.choose(self, None, lambda dialog, result: self.history.clear() if dialog.choose_finish(result) == "clear" else None)

    def _page_changed(self, stack: Adw.ViewStack, _: GParamSpec) -> None:  # type: ignore[name-defined]
        name = stack.get_visible_child_name()
        if name in self.page_buttons and not self.page_buttons[name].get_active():
            self.page_buttons[name].set_active(True)
        if name == "transcriptions":
            self.history.refresh()

    def apply_history(self, page: HistoryPage) -> None:
        if self.destroying:
            return
        while child := self.history_list.get_first_child():
            self.history_list.remove(child)
        for entry in page.entries:
            self.history_list.append(self._history_row(entry))
        empty = not page.entries
        self.history_list.set_visible(not empty)
        self.empty_label.set_label("No matching transcripts" if self.history.query else "No transcripts yet")
        self.empty_label.set_visible(empty)
        self.load_more.set_visible(page.has_more)
        self.clear_button.set_sensitive(self.history.history_exists)

    def _history_row(self, entry: TranscriptEntry) -> Gtk.ListBoxRow:
        row = Gtk.ListBoxRow.new()
        body = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=8)
        body.set_margin_top(12); body.set_margin_bottom(12); body.set_margin_start(12); body.set_margin_end(12)
        timestamp = Gtk.Label.new(datetime.fromtimestamp(entry.created_at / 1000).astimezone().strftime("%x %X"))
        timestamp.add_css_class("dim-label"); timestamp.set_xalign(0)
        transcript = Gtk.Label.new(entry.text)
        transcript.set_xalign(0); transcript.set_wrap(True); transcript.set_selectable(True); transcript.add_css_class("transcript-body")
        actions = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        actions.set_halign(Gtk.Align.END)
        copy = Gtk.Button.new_with_label("Copy")
        copy.connect("clicked", lambda *_: self._copy(entry.text))
        delete = Gtk.Button.new_with_label("Delete")
        delete.add_css_class("destructive-action")
        delete.connect("clicked", lambda *_: self.history.delete(entry.id))
        actions.append(copy)
        actions.append(delete)
        body.append(timestamp)
        body.append(transcript)
        body.append(actions)
        row.set_child(body)
        return row

    def _copy(self, text: str) -> None:
        clipboard = self.get_clipboard()
        clipboard.set(text)
        self.overlay.add_toast(Adw.Toast.new("Transcript copied"))

    def _close_request(self, *_: object) -> bool:
        if self.destroying:
            return False
        if self.client.preview_process is None:
            self.destroying = True
            return False
        if not self.closing_window:
            self.closing_window = True
            self.preview_button.set_sensitive(False)
            self.client.close_preview(self._preview_closed_for_window)
        return True

    def _preview_closed_for_window(self, _: None, error: Exception | None) -> None:
        if self.destroying or not self.closing_window:
            return
        if error:
            self.closing_window = False
            self.preview_button.set_sensitive(True)
            self.show_error(str(error))
            return
        self.destroying = True
        self.destroy()


class SettingsApplication(Adw.Application):
    def __init__(self) -> None:
        super().__init__(application_id="io.github.andy_spike.CodexVoice.Settings")
        self.window: SettingsWindow | None = None
        self.css_provider: Gtk.CssProvider | None = None

    def do_startup(self) -> None:
        Adw.Application.do_startup(self)
        display = Gdk.Display.get_default()
        if display is None:
            return
        provider = Gtk.CssProvider.new()
        provider.load_from_data(
            """
            @define-color accent_color #10A37F;
            @define-color accent_bg_color #10A37F;
            @define-color accent_fg_color #FFFFFF;

            .settings-switcher button {
              min-width: 108px;
            }
            """
        )
        Gtk.StyleContext.add_provider_for_display(
            display,
            provider,
            Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION,
        )
        self.css_provider = provider

    def do_activate(self) -> None:
        if self.window is None:
            self.window = SettingsWindow(self)
        self.window.present()


def main(argv: Sequence[str] | None = None) -> int:
    app = SettingsApplication()
    return app.run(list(argv or sys.argv))


if __name__ == "__main__":
    raise SystemExit(main())
