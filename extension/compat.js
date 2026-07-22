import Clutter from 'gi://Clutter';
import Meta from 'gi://Meta';
import Shell from 'gi://Shell';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';

export function addKeybinding(name, settings, handler) {
    return Main.wm.addKeybinding(name, settings,
        Meta.KeyBindingFlags.NONE,
        Shell.ActionMode.NORMAL,
        handler);
}

export function removeKeybinding(name) {
    Main.wm.removeKeybinding(name);
}

export function connectEscape(handler) {
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
