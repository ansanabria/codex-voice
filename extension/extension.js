import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Compat from './compat.js';
import { parseRuntimeStateText, runtimeStateOwnerIsCurrent } from './protocol.js';

const UUID = 'codex-voice@andy-spike.github.io';
const XDG_RUNTIME_DIR = GLib.getenv('XDG_RUNTIME_DIR');
const RUNTIME_DIR = XDG_RUNTIME_DIR || GLib.build_filenamev([GLib.get_user_cache_dir(), 'codex-voice', 'runtime']);
const STATE_FILE = `${RUNTIME_DIR}/codex-voice-state.json`;
const KEYBINDING_NAME = 'keybinding';

function cliPath() {
    const candidates = ['/usr/bin/codex-voice', `${GLib.get_home_dir()}/.local/bin/codex-voice`];
    return candidates.find(path => GLib.file_test(path, GLib.FileTest.EXISTS)) || candidates[0];
}

function runCliAsync(args, callback) {
    try {
        const launcher = new Gio.SubprocessLauncher({
            flags: Gio.SubprocessFlags.STDOUT_PIPE | Gio.SubprocessFlags.STDERR_PIPE,
        });
        if (!XDG_RUNTIME_DIR) launcher.setenv('XDG_RUNTIME_DIR', RUNTIME_DIR, true);
        const process = launcher.spawnv([cliPath(), ...args]);
        process.communicate_utf8_async(null, null, (_process, result) => {
            let success;
            let detail;
            try {
                const [, , stderr] = process.communicate_utf8_finish(result);
                success = process.get_successful();
                detail = stderr?.trim() || '';
            } catch (error) {
                success = false;
                detail = error.message;
            }
            callback(success, detail);
        });
    } catch (error) {
        callback(false, error.message);
    }
}

export default class CodexVoiceExtension extends Extension {
    enable() {
        this._enabled = true;
        this._settings = this._loadSettings();
        this._escapeId = 0;
        this._stateDebounce = 0;
        this._statePoll = 0;
        this._actionInFlight = false;
        this._actionSupersededByCancel = false;
        this._cancelInFlight = false;
        this._copyInFlight = false;
        this._settingsInFlight = false;
        this._indicator = null;
        this._stateItem = null;
        this._actionItem = null;
        this._enabledItem = null;
        if (!XDG_RUNTIME_DIR) {
            if (GLib.mkdir_with_parents(RUNTIME_DIR, 0o700) !== 0 || GLib.chmod(RUNTIME_DIR, 0o700) !== 0)
                throw new Error(`Could not create private runtime directory ${RUNTIME_DIR}`);
        }
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
        this._statePoll = GLib.timeout_add_seconds(GLib.PRIORITY_DEFAULT, 2, () => {
            this._readState();
            return GLib.SOURCE_CONTINUE;
        });
        this._scheduleStateRead();
    }

    _syncIndicatorVisibility() {
        if (!this._settings.get_boolean('show-tray-icon')) {
            this._indicator?.destroy();
            this._indicator = this._stateItem = this._actionItem = this._copyLastItem = this._enabledItem = null;
            return;
        }
        if (this._indicator) return;
        this._indicator = new PanelMenu.Button(0.0, 'Codex Voice');
        const icon = new Gio.FileIcon({
            file: this.dir.get_child('icons').get_child('codex-voice-panel.png'),
        });
        this._indicator.add_child(new St.Icon({ gicon: icon, style_class: 'system-status-icon' }));
        this._stateItem = new PopupMenu.PopupMenuItem('Idle', { reactive: false });
        this._actionItem = new PopupMenu.PopupMenuItem('Start Dictation');
        this._actionItem.connect('activate', () => {
            this._readState();
            const args = [this._state === 'recording' ? '--stop' : this._state === 'idle' ? '--start' : '--cancel'];
            this._runAction(args);
        });
        this._copyLastItem = new PopupMenu.PopupMenuItem('Copy last transcript');
        this._copyLastItem.setSensitive(false);
        this._copyLastItem.connect('activate', () => this._copyLastTranscript());
        this._enabledItem = new PopupMenu.PopupSwitchMenuItem('Dictation Enabled', this._settings.get_boolean('enabled'));
        this._enabledItem.connect('toggled', (_item, active) => this._settings.set_boolean('enabled', active));
        const settings = new PopupMenu.PopupMenuItem('Settings');
        settings.connect('activate', () => {
            if (this._settingsInFlight) return;
            this._settingsInFlight = true;
            runCliAsync(['--settings'], (success, detail) => {
                this._settingsInFlight = false;
                if (this._enabled && !success) this._notifyFailure('Could not open Settings', detail);
            });
        });
        this._indicator.menu.addMenuItem(this._stateItem);
        this._indicator.menu.addMenuItem(this._actionItem);
        this._indicator.menu.addMenuItem(this._copyLastItem);
        this._indicator.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._indicator.menu.addMenuItem(this._enabledItem);
        this._indicator.menu.addMenuItem(settings);
        Main.panel.addToStatusArea(UUID, this._indicator);
        this._indicator.menu.connect('open-state-changed', (_menu, open) => {
            if (open) this._refreshCopyLastAvailability();
        });
        this._readState();
        this._refreshCopyLastAvailability();
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
            () => this._runAction(['--toggle']));
    }

    _syncEnabledState() {
        const enabled = this._settings.get_boolean('enabled');
        this._enabledItem?.setToggleState(enabled);
        this._registerShortcut();
    }

    _refreshCopyLastAvailability() {
        runCliAsync(['history', 'has'], success => {
            if (!this._copyInFlight) this._copyLastItem?.setSensitive(success);
        });
    }

    _runAction(args) {
        if (this._actionInFlight || this._cancelInFlight) return;
        this._actionInFlight = true;
        this._actionSupersededByCancel = false;
        this._syncActionSensitivity();
        runCliAsync(args, (success, detail) => {
            const supersededByCancel = this._actionSupersededByCancel;
            this._actionInFlight = false;
            this._actionSupersededByCancel = false;
            this._syncActionSensitivity();
            if (!this._enabled) return;
            if (!success && !supersededByCancel)
                this._notifyFailure('Dictation action failed', detail);
            this._scheduleStateRead();
        });
    }

    _requestCancel() {
        if (this._cancelInFlight) return;
        if (this._actionInFlight) this._actionSupersededByCancel = true;
        this._cancelInFlight = true;
        this._syncActionSensitivity();
        runCliAsync(['--cancel'], (success, detail) => {
            this._cancelInFlight = false;
            this._syncActionSensitivity();
            if (!this._enabled) return;
            if (!success) this._notifyFailure('Could not cancel dictation', detail);
            this._scheduleStateRead();
        });
    }

    _copyLastTranscript() {
        if (this._copyInFlight) return;
        this._copyInFlight = true;
        this._copyLastItem?.setSensitive(false);
        runCliAsync(['--copy-last'], (success, detail) => {
            this._copyInFlight = false;
            if (!this._enabled) return;
            if (!success) {
                this._notifyFailure('Could not copy the last transcript', detail);
                this._refreshCopyLastAvailability();
                return;
            }
            if (this._copyLastItem) this._copyLastItem.label.text = 'Copied';
            GLib.timeout_add(GLib.PRIORITY_DEFAULT, 1500, () => {
                if (this._copyLastItem) this._copyLastItem.label.text = 'Copy last transcript';
                return GLib.SOURCE_REMOVE;
            });
            this._refreshCopyLastAvailability();
        });
    }

    _syncActionSensitivity() {
        this._actionItem?.setSensitive(!this._actionInFlight && !this._cancelInFlight);
    }

    _notifyFailure(message, detail) {
        const suffix = detail ? `: ${detail.split('\n', 1)[0]}` : '';
        console.warn(`Codex Voice: ${message}${suffix}`);
        Main.notify('Codex Voice', `${message}${suffix}`);
    }

    _scheduleStateRead() {
        if (this._stateDebounce) return;
        this._stateDebounce = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 80, () => { this._stateDebounce = 0; this._readState(); return GLib.SOURCE_REMOVE; });
    }

    _readState() {
        try {
            const [, bytes] = Gio.File.new_for_path(STATE_FILE).load_contents(null);
            const runtimeState = parseRuntimeStateText(new TextDecoder().decode(bytes));
            if (!runtimeState) throw new Error('Invalid runtime state');
            const [, statBytes] = Gio.File.new_for_path(`/proc/${runtimeState.ownerPid}/stat`).load_contents(null);
            if (!runtimeStateOwnerIsCurrent(runtimeState, new TextDecoder().decode(statBytes)))
                throw new Error('Stale runtime state');
            this._state = runtimeState.state;
        } catch (_) { this._state = 'idle'; }
        this._updateStatePresentation();
        if (this._state !== 'idle' && !this._escapeId)
            this._escapeId = Compat.connectEscape(() => this._requestCancel());
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
        this._enabled = false;
        if (this._stateDebounce) {
            GLib.Source.remove(this._stateDebounce);
            this._stateDebounce = 0;
        }
        Compat.removeKeybinding(KEYBINDING_NAME);
        for (const id of this._settingsSignals || []) this._settings.disconnect(id);
        this._monitor?.cancel();
        if (this._statePoll) {
            GLib.Source.remove(this._statePoll);
            this._statePoll = 0;
        }
        Compat.disconnectEscape(this._escapeId);
        this._indicator?.destroy();
        this._monitor = this._indicator = this._stateItem = this._actionItem = this._copyLastItem = this._enabledItem = null;
        this._escapeId = 0;
    }
}
