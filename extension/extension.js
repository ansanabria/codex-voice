import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import St from 'gi://St';
import Main from 'resource:///org/gnome/shell/ui/main.js';
import PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Compat from './compat.js';

const UUID = 'codex-voice@andy-spike.github.io';
const STATE_FILE = `${GLib.getenv('XDG_RUNTIME_DIR') || '/tmp'}/codex-voice-state.json`;
const WAV_FILE = `${GLib.getenv('XDG_RUNTIME_DIR') || '/tmp'}/codex-voice.wav`;
const KEYBINDING_NAME = 'codex-voice-toggle';

function runCli(args) {
    try {
        new Gio.Subprocess({ argv: ['codex-voice', ...args], flags: Gio.SubprocessFlags.NONE }).init(null);
    } catch (error) {
        console.warn(`Codex Voice: could not run CLI: ${error.message}`);
    }
}

function color(value, fallback) {
    return /^#[0-9a-f]{8}$/i.test(value) ? value : fallback;
}

class Pill {
    constructor(settings) {
        this._settings = settings;
        this._actor = null;
        this._waveform = null;
        this._waveTimer = 0;
        this._reading = false;
        this._escapeId = 0;
        this._workareaId = 0;
        this._state = 'idle';
    }

    setState(state) {
        this._state = state;
        if (state === 'idle' || state === 'typing') {
            this.destroy();
            return;
        }
        if (!this._actor) this._create();
        this._label.text = state === 'transcribing' ? 'Transcribing…' : '';
        this._waveform.visible = state === 'recording';
        this._mic.visible = state === 'recording';
        this._reposition();
        if (state === 'recording' && !this._waveTimer)
            this._waveTimer = GLib.timeout_add(GLib.PRIORITY_DEFAULT, 50, () => this._readWaveform());
    }

    _create() {
        this._actor = new St.BoxLayout({ style_class: 'codex-voice-pill', reactive: true });
        this._actor.set_style(`background-color: ${color(this._settings.get_string('pill-background-color'), '#0e1110eb')};`);
        this._mic = new St.Icon({ icon_name: 'audio-input-microphone-symbolic', style_class: 'codex-voice-accent' });
        this._mic.set_style(`color: ${color(this._settings.get_string('pill-accent-color'), '#32d870')};`);
        this._waveform = new St.Label({ text: '▁▃▅▇▅▃▁', style_class: 'codex-voice-waveform' });
        this._waveform.set_style(`color: ${color(this._settings.get_string('pill-accent-color'), '#32d870')};`);
        this._label = new St.Label({ style_class: 'codex-voice-label' });
        const cancel = new St.Button({ label: '×', style_class: 'codex-voice-cancel', can_focus: false });
        cancel.connect('clicked', () => runCli(['--cancel']));
        this._actor.add_child(this._mic);
        this._actor.add_child(this._waveform);
        this._actor.add_child(this._label);
        this._actor.add_child(cancel);
        Compat.addTopChrome(this._actor);
        this._escapeId = Compat.connectEscape(this._actor, () => runCli(['--cancel']));
        this._workareaId = Compat.watchWorkArea(() => this._reposition());
        this._reposition();
    }

    _reposition() {
        if (!this._actor) return;
        const area = Compat.getPrimaryWorkArea();
        const [width, height] = this._actor.get_preferred_size().slice(2);
        this._actor.set_position(Math.round(area.x + (area.width - width) / 2), Math.round(area.y + area.height - height - 36));
    }

    _readWaveform() {
        if (this._state !== 'recording' || !this._actor) { this._waveTimer = 0; return GLib.SOURCE_REMOVE; }
        if (this._reading) return GLib.SOURCE_CONTINUE;
        this._reading = true;
        Gio.File.new_for_path(WAV_FILE).load_contents_async(null, (file, result) => {
            try {
                const [, bytes] = file.load_contents_finish(result);
                const tail = bytes.slice(Math.max(44, bytes.length - 4096));
                let sum = 0;
                for (let index = 0; index + 1 < tail.length; index += 2) {
                    const sample = tail[index] | (tail[index + 1] << 8);
                    sum += Math.abs(sample > 32767 ? sample - 65536 : sample);
                }
                const level = Math.min(1, sum / Math.max(1, tail.length / 2) / 12000);
                this._waveform.text = level > .55 ? '▂▄▇█▇▄▂' : level > .2 ? '▁▃▅▆▅▃▁' : '▁▂▃▂▃▂▁';
            } catch (_) { /* Recording can rotate or disappear between reads. */ }
            this._reading = false;
        });
        return GLib.SOURCE_CONTINUE;
    }

    destroy() {
        if (this._waveTimer) GLib.Source.remove(this._waveTimer);
        this._waveTimer = 0;
        Compat.disconnectEscape(this._escapeId);
        this._escapeId = 0;
        Compat.unwatchWorkArea(this._workareaId);
        this._workareaId = 0;
        if (this._actor) {
            Compat.removeChrome(this._actor);
            this._actor.destroy();
            this._actor = null;
        }
    }
}

export default class CodexVoiceExtension extends Extension {
    enable() {
        this._settings = this._loadSettings();
        this._pill = new Pill(this._settings);
        this._indicator = new PanelMenu.Button(0.0, 'Codex Voice');
        this._indicator.add_child(new St.Icon({ icon_name: 'audio-input-microphone-symbolic', style_class: 'system-status-icon' }));
        this._stateItem = new PopupMenu.PopupMenuItem('Idle', { reactive: false });
        this._actionItem = new PopupMenu.PopupMenuItem('Start Dictation');
        this._actionItem.connect('activate', () => runCli([
            this._state === 'recording' ? '--stop' : this._state === 'idle' ? '--start' : '--cancel'
        ]));
        const enabled = new PopupMenu.PopupSwitchMenuItem('Dictation Enabled', this._settings.get_boolean('enabled'));
        enabled.connect('toggled', (_item, active) => this._settings.set_boolean('enabled', active));
        const settings = new PopupMenu.PopupMenuItem('Settings');
        settings.connect('activate', () => runCli(['--settings']));
        this._indicator.menu.addMenuItem(this._stateItem);
        this._indicator.menu.addMenuItem(this._actionItem);
        this._indicator.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._indicator.menu.addMenuItem(enabled);
        this._indicator.menu.addMenuItem(settings);
        Main.panel.addToStatusArea(UUID, this._indicator);
        this._settingsSignals = [
            this._settings.connect('changed::enabled', () => this._registerShortcut()),
            this._settings.connect('changed::keybinding', () => this._registerShortcut()),
        ];
        this._registerShortcut();
        this._monitor = Gio.File.new_for_path(STATE_FILE).monitor_file(Gio.FileMonitorFlags.NONE, null);
        this._monitor.connect('changed', () => this._scheduleStateRead());
        this._scheduleStateRead();
    }

    _loadSettings() {
        const directory = GLib.build_filenamev([GLib.get_home_dir(), '.local', 'share', 'codex-voice', 'schemas']);
        const source = Gio.SettingsSchemaSource.new_from_directory(directory, Gio.SettingsSchemaSource.get_default(), false);
        const schema = source.lookup('io.github.andy_spike.CodexVoice', true);
        if (!schema) throw new Error('Codex Voice GSettings schema is missing');
        return new Gio.Settings({ settings_schema: schema });
    }

    _registerShortcut() {
        Compat.removeKeybinding(KEYBINDING_NAME);
        if (this._settings.get_boolean('enabled'))
            Compat.addKeybinding(KEYBINDING_NAME, this._settings, () => runCli(['--toggle']));
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
        this._stateItem.label.text = this._state[0].toUpperCase() + this._state.slice(1);
        this._actionItem.label.text = this._state === 'recording' ? 'Stop and Transcribe' : this._state === 'idle' ? 'Start Dictation' : 'Cancel';
        this._pill.setState(this._state);
    }

    disable() {
        if (this._stateDebounce) GLib.Source.remove(this._stateDebounce);
        Compat.removeKeybinding(KEYBINDING_NAME);
        for (const id of this._settingsSignals || []) this._settings.disconnect(id);
        this._monitor?.cancel();
        this._pill?.destroy();
        this._indicator?.destroy();
        this._monitor = this._pill = this._indicator = null;
    }
}
