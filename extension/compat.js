import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import Config from 'resource:///org/gnome/shell/misc/config.js';
import Main from 'resource:///org/gnome/shell/ui/main.js';

const shellMajor = Number.parseInt(Config.PACKAGE_VERSION, 10);

export function getShellMajor() {
    return shellMajor;
}

export function addTopChrome(actor) {
    Main.layoutManager.addTopChrome(actor, { trackFullscreen: true });
}

export function removeChrome(actor) {
    Main.layoutManager.removeChrome(actor);
}

export function getPrimaryWorkArea() {
    const monitor = Main.layoutManager.primaryMonitor;
    return Main.layoutManager.getWorkAreaForMonitor(monitor.index);
}

export function watchWorkArea(callback) {
    return Main.layoutManager.connect('workareas-changed', callback);
}

export function unwatchWorkArea(id) {
    if (id) Main.layoutManager.disconnect(id);
}

export function addKeybinding(name, settings, handler) {
    Main.wm.addKeybinding(name, settings,
        Meta.KeyBindingFlags.NONE,
        Shell.ActionMode.NORMAL | Shell.ActionMode.OVERVIEW,
        handler);
}

export function removeKeybinding(name) {
    Main.wm.removeKeybinding(name);
}

export function connectEscape(actor, handler) {
    return global.stage.connect('captured-event', (_stage, event) => {
        if (event.type() === Clutter.EventType.KEY_PRESS &&
            event.get_key_symbol() === Clutter.KEY_Escape) {
            handler();
            return Clutter.EVENT_STOP;
        }
        return Clutter.EVENT_PROPAGATE;
    });
}

export function disconnectEscape(id) {
    if (id) global.stage.disconnect(id);
}
