import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Compat from './compat.js';

const UUID = 'codex-voice@andy-spike.github.io';
const RUNTIME_DIR = GLib.getenv('XDG_RUNTIME_DIR') || '/tmp';
const STATE_FILE = `${RUNTIME_DIR}/codex-voice-state.json`;
const KEYBINDING_NAME = 'keybinding';

function cliPath() {
    const candidates = ['/usr/bin/codex-voice', `${GLib.get_home_dir()}/.local/bin/codex-voice`];
    return candidates.find(path => GLib.file_test(path, GLib.FileTest.EXISTS)) || candidates[0];
}

function runCli(args) {
    try {
        new Gio.Subprocess({ argv: [cliPath(), ...args], flags: Gio.SubprocessFlags.NONE }).init(null);
    } catch (error) {
        console.warn(`Codex Voice: could not run CLI: ${error.message}`);
    }
}

export default class CodexVoiceExtension extends Extension {
    enable() {
        this._settings = this._loadSettings();
        this._escapeId = 0;
        this._indicator = null;
        this._stateItem = null;
        this._actionItem = null;
        this._enabledItem = null;
        this._syncIndicatorVisibility();
        this._settingsSignals = [
            this._settings.connect('changed::enabled', () => this._syncEnabledState()),
            this._settings.connect('changed::keybinding', () => this._registerShortcut()),
            this._settings.connect('changed::show-tray-icon', () => this._syncIndicatorVisibility()),
        ];
        this._registerShortcut();
        // The CLI publishes state with an atomic rename. Monitoring the file
        // itself follows the old inode and can miss later replacements.
        this._monitor = Gio.File.new_for_path(RUNTIME_DIR).monitor_directory(Gio.FileMonitorFlags.NONE, null);
        this._monitor.connect('changed', (_monitor, file, otherFile) => {
            if (file?.get_path() === STATE_FILE || otherFile?.get_path() === STATE_FILE)
                this._scheduleStateRead();
        });
        this._scheduleStateRead();
    }

    _syncIndicatorVisibility() {
        if (!this._settings.get_boolean('show-tray-icon')) {
            this._indicator?.destroy();
            this._indicator = this._stateItem = this._actionItem = this._enabledItem = null;
            return;
        }
        if (this._indicator) return;
        this._indicator = new PanelMenu.Button(0.0, 'Codex Voice');
        this._indicator.add_child(new St.Icon({ icon_name: 'audio-input-microphone-symbolic', style_class: 'system-status-icon' }));
        this._stateItem = new PopupMenu.PopupMenuItem('Idle', { reactive: false });
        this._actionItem = new PopupMenu.PopupMenuItem('Start Dictation');
        this._actionItem.connect('activate', () => {
            this._readState();
            runCli([this._state === 'recording' ? '--stop' : this._state === 'idle' ? '--start' : '--cancel']);
        });
        this._enabledItem = new PopupMenu.PopupSwitchMenuItem('Dictation Enabled', this._settings.get_boolean('enabled'));
        this._enabledItem.connect('toggled', (_item, active) => this._settings.set_boolean('enabled', active));
        const settings = new PopupMenu.PopupMenuItem('Settings');
        settings.connect('activate', () => runCli(['--settings']));
        this._indicator.menu.addMenuItem(this._stateItem);
        this._indicator.menu.addMenuItem(this._actionItem);
        this._indicator.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._indicator.menu.addMenuItem(this._enabledItem);
        this._indicator.menu.addMenuItem(settings);
        Main.panel.addToStatusArea(UUID, this._indicator);
        this._readState();
    }

    _loadSettings() {
        const defaultSource = Gio.SettingsSchemaSource.get_default();
        let schema = defaultSource.lookup('io.github.andy_spike.CodexVoice', true);
        if (!schema) {
            const directory = GLib.build_filenamev([GLib.get_home_dir(), '.local', 'share', 'codex-voice', 'schemas']);
            const source = Gio.SettingsSchemaSource.new_from_directory(directory, defaultSource, false);
            schema = source.lookup('io.github.andy_spike.CodexVoice', true);
        }
        if (!schema) throw new Error('Codex Voice GSettings schema is missing');
        return new Gio.Settings({ settings_schema: schema });
    }

    _registerShortcut() {
        Compat.removeKeybinding(KEYBINDING_NAME);
        if (!this._settings.get_boolean('enabled'))
            return;
        Compat.addKeybinding(
            KEYBINDING_NAME,
            this._settings,
            () => runCli(['--toggle']));
    }

    _syncEnabledState() {
        const enabled = this._settings.get_boolean('enabled');
        this._enabledItem?.setToggleState(enabled);
        this._registerShortcut();
    }

    _scheduleStateRead() {
        if (this._stateDebounce) return;
        this._stateDebounce = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 80, () => { this._stateDebounce = 0; this._readState(); return GLib.SOURCE_REMOVE; });
    }

    _readState() {
        try {
            const [, bytes] = Gio.File.new_for_path(STATE_FILE).load_contents(null);
            const state = JSON.parse(new TextDecoder().decode(bytes)).state;
            this._state = ['recording', 'transcribing', 'typing'].includes(state) ? state : 'idle';
        } catch (_) { this._state = 'idle'; }
        this._updateStatePresentation();
        if (this._state !== 'idle' && !this._escapeId)
            this._escapeId = Compat.connectEscape(() => {
                runCli(['--cancel']);
                Compat.disconnectEscape(this._escapeId);
                this._escapeId = 0;
                this._state = 'idle';
                this._updateStatePresentation();
            });
        else if (this._state === 'idle' && this._escapeId) {
            Compat.disconnectEscape(this._escapeId);
            this._escapeId = 0;
        }
    }

    _updateStatePresentation() {
        if (this._stateItem)
            this._stateItem.label.text = this._state[0].toUpperCase() + this._state.slice(1);
        if (this._actionItem)
            this._actionItem.label.text = this._state === 'recording' ? 'Stop and Transcribe' : this._state === 'idle' ? 'Start Dictation' : 'Cancel';
    }

    disable() {
        if (this._stateDebounce) GLib.Source.remove(this._stateDebounce);
        Compat.removeKeybinding(KEYBINDING_NAME);
        for (const id of this._settingsSignals || []) this._settings.disconnect(id);
        this._monitor?.cancel();
        Compat.disconnectEscape(this._escapeId);
        this._indicator?.destroy();
        this._monitor = this._indicator = this._stateItem = this._actionItem = this._enabledItem = null;
        this._escapeId = 0;
    }
}
