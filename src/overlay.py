#!/usr/bin/env python3
"""codex-voice overlay — recording pill with a live waveform.

The overlay reads levels from the WAV file that the main recorder is already
writing.  It never opens the microphone itself, so it cannot contend with the
actual recording process.
"""
import argparse
import math
import os
import signal
import struct
import subprocess
import time

# Cover the import/GTK-construction window before GLib owns this signal.
_EARLY_TRANSCRIBE = False
if __name__ == '__main__':
    def remember_transcribing(_signal, _frame):
        global _EARLY_TRANSCRIBE
        _EARLY_TRANSCRIBE = True

    signal.signal(signal.SIGUSR1, remember_transcribing)

import gi

gi.require_version('Gtk', '3.0')
gi.require_version('Gdk', '3.0')
gi.require_version('GLibUnix', '2.0')
gi.require_foreign('cairo')
from gi.repository import Gdk, GLib, GLibUnix, Gtk
import cairo


PILL_W = 108
PILL_H = 40
MARGIN_BOTTOM = 36
NUM_BARS = 9
BAR_W = 3
BAR_GAP = 3
BAR_MAX_H = 22
BAR_MIN_H = 5
LEVEL_POLL_MS = 50
WAV_HEADER_SIZE = 44
LEVEL_BYTES = 4096

PILL_BACKGROUND = '#0F0F0F'
PILL_FOREGROUND = (250 / 255.0, 250 / 255.0, 250 / 255.0)


class Waveform(Gtk.DrawingArea):
    """Render audio levels as a row of softly rounded capsules."""

    def __init__(self, num_bars, bar_w, bar_gap, max_h, min_h, color):
        super().__init__()
        self.num_bars = num_bars
        self.bar_w = bar_w
        self.bar_gap = bar_gap
        self.max_h = max_h
        self.min_h = min_h
        self.levels = [0.0] * num_bars
        self.color = color
        self.spinner_active = False
        self.spinner_angle = 0.0
        width = num_bars * bar_w + (num_bars - 1) * bar_gap
        self.set_size_request(width, max_h)
        self.set_valign(Gtk.Align.CENTER)
        self.set_halign(Gtk.Align.CENTER)
        self.set_hexpand(True)
        self.connect('draw', self._on_draw)

    def set_levels(self, levels):
        self.levels = levels
        self.queue_draw()

    def start_spinner(self):
        self.spinner_active = True
        self.levels = []
        GLib.timeout_add(50, self._animate_spinner)

    def _animate_spinner(self):
        if not self.spinner_active:
            return False
        self.spinner_angle = (self.spinner_angle + math.pi / 10.0) % (math.pi * 2.0)
        self.queue_draw()
        return True

    def _on_draw(self, _widget, cr):
        if self.spinner_active:
            center_x = self.get_allocated_width() / 2.0
            center_y = self.get_allocated_height() / 2.0
            cr.set_source_rgba(*self.color, 1.0)
            cr.set_line_cap(cairo.LINE_CAP_ROUND)
            cr.set_line_width(3.0)
            cr.arc(center_x, center_y, 7.0,
                   self.spinner_angle, self.spinner_angle + math.pi * 1.45)
            cr.stroke()
            return False

        center_y = self.get_allocated_height() / 2.0
        cr.set_line_cap(cairo.LINE_CAP_ROUND)
        cr.set_line_width(self.bar_w)

        for index, level in enumerate(self.levels):
            height = self.min_h + level * (self.max_h - self.min_h)
            height = max(self.min_h, min(self.max_h, height))
            x = index * (self.bar_w + self.bar_gap) + self.bar_w / 2.0
            half_line = max(0.0, (height - self.bar_w) / 2.0)
            alpha = 0.55 + level * 0.45
            cr.set_source_rgba(*self.color, alpha)
            cr.move_to(x, center_y - half_line)
            cr.line_to(x, center_y + half_line)
            cr.stroke()
        return False


class Overlay(Gtk.Window):
    """Bottom-center recording overlay with recording/transcribing states."""

    def __init__(self, audio_file=None, control_command=None, preview=False,
                 recorder_pid=None, recorder_start_time=None):
        # The launcher uses XWayland because native Wayland clients cannot place
        # toplevels in global coordinates. The pill accepts focus while active so
        # Escape can cancel without relying on the Shell extension.
        super().__init__(type=Gtk.WindowType.TOPLEVEL)
        self.audio_file = audio_file
        self.control_command = control_command
        self.preview = preview
        self.recorder_pid = recorder_pid
        self.recorder_start_time = recorder_start_time
        self.state = 'recording'
        self._level_history = [0.0] * ((NUM_BARS // 2) + 1)
        self._smoothed_level = 0.0

        self.set_title('Codex Voice')
        self.set_decorated(False)
        self.set_app_paintable(True)
        self.set_resizable(False)
        self.set_skip_taskbar_hint(True)
        self.set_skip_pager_hint(True)
        # A notification-type window is deliberately denied keyboard focus by
        # GNOME even when accept-focus is enabled. UTILITY keeps the pill out of
        # normal app chrome (with the taskbar/pager hints below) while allowing
        # Escape to reach the active overlay.
        self.set_type_hint(Gdk.WindowTypeHint.UTILITY)
        self.set_keep_above(True)
        self.set_accept_focus(True)
        self.set_focus_on_map(True)
        self.set_can_focus(True)
        self.set_gravity(Gdk.Gravity.NORTH_WEST)
        self.stick()
        self.set_size_request(PILL_W, PILL_H)

        screen = self.get_screen()
        visual = screen.get_rgba_visual()
        if visual:
            self.set_visual(visual)

        css = f"""
        window {{
            background-color: transparent;
        }}
        .pill {{
            background-color: {PILL_BACKGROUND};
            border: 1px solid rgba(250, 250, 250, 0.32);
            border-radius: 20px;
            padding: 6px 8px;
        }}
        spinner {{
            color: #fafafa;
        }}
        .cancel-btn {{
            min-width: 18px;
            min-height: 18px;
            padding: 0;
            margin: 0;
            border: none;
            border-radius: 8px;
            background: transparent;
            color: #fafafa;
            font-size: 14px;
            font-weight: 400;
        }}
        .cancel-btn:hover {{
            background: rgba(250, 250, 250, 0.12);
        }}
        .cancel-btn:active {{
            background: rgba(250, 250, 250, 0.20);
        }}
        """.encode()
        provider = Gtk.CssProvider()
        provider.load_from_data(css)
        Gtk.StyleContext.add_provider_for_screen(
            screen, provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)

        self._build_ui()
        self.get_accessible().set_name('Codex Voice recording')
        self.connect('map-event', self._on_map)
        self.connect('key-press-event', self._on_key_press)
        self.connect('destroy', self._on_destroy)

        self.show_all()
        self.present()
        self._position_window()

        GLib.timeout_add(LEVEL_POLL_MS, self._poll_levels)

    def _build_ui(self):
        pill = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        pill.get_style_context().add_class('pill')
        pill.set_valign(Gtk.Align.CENTER)
        pill.set_size_request(PILL_W, PILL_H)

        content = Gtk.Overlay()
        recording = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        recording.set_valign(Gtk.Align.CENTER)
        self.waveform = Waveform(
            NUM_BARS, BAR_W, BAR_GAP, BAR_MAX_H, BAR_MIN_H,
            PILL_FOREGROUND)
        self.waveform.set_hexpand(False)
        # The pill supplies the first 8px of inset. This second 8px places the
        # waveform 16px from the leading edge, matching the settings preview.
        self.waveform.set_margin_start(8)

        recording.pack_start(self.waveform, False, False, 0)

        cancel = Gtk.Button(label='\u00d7')
        cancel.set_tooltip_text('Close preview' if self.preview else 'Cancel dictation')
        cancel.set_can_focus(True)
        cancel.set_focus_on_click(True)
        cancel.set_halign(Gtk.Align.END)
        cancel.set_valign(Gtk.Align.CENTER)
        cancel.get_style_context().add_class('cancel-btn')
        cancel.set_relief(Gtk.ReliefStyle.NONE)
        cancel.connect('clicked', self._cancel)
        cancel.get_accessible().set_name(
            'Close preview' if self.preview else 'Cancel dictation')
        self.cancel_button = cancel
        recording.pack_end(cancel, False, False, 0)

        # A normal row lets the remaining 13px become breathing room between
        # the 51px waveform and 18px cancel control instead of overlapping
        # independently-positioned children inside GTK's padded content box.
        content.add(recording)
        self.recording = recording

        pill.pack_start(content, True, True, 0)
        self.add(pill)

    def _on_map(self, _widget, _event):
        self._position_window()
        self.grab_focus()
        # Window managers may ignore the initial placement request while the
        # surface is being mapped, so repeat it after allocation settles.
        GLib.timeout_add(100, self._position_window)
        GLib.timeout_add(110, self._activate_window)
        return False

    def _activate_window(self):
        self.present()
        self.grab_focus()
        self.queue_draw()
        return GLib.SOURCE_REMOVE

    def _on_key_press(self, _widget, event):
        if event.keyval == Gdk.KEY_Escape:
            self._cancel()
            return True
        return False

    def _position_window(self):
        display = self.get_display()
        monitor = display.get_primary_monitor() or display.get_monitor(0)
        if monitor is None:
            return False
        workarea = monitor.get_workarea()
        width = max(PILL_W, self.get_allocated_width())
        height = max(PILL_H, self.get_allocated_height())
        x = workarea.x + (workarea.width - width) // 2
        y = workarea.y + workarea.height - height - MARGIN_BOTTOM
        self.move(x, y)
        return False

    def _read_audio_level(self):
        if not self.audio_file:
            return 0.0
        try:
            size = os.path.getsize(self.audio_file)
            if size <= WAV_HEADER_SIZE:
                return 0.0
            start = max(WAV_HEADER_SIZE, size - LEVEL_BYTES)
            if start % 2:
                start += 1
            with open(self.audio_file, 'rb', buffering=0) as stream:
                stream.seek(start)
                data = stream.read(size - start)
        except (FileNotFoundError, PermissionError, OSError):
            return 0.0

        sample_count = len(data) // 2
        if not sample_count:
            return 0.0
        samples = struct.unpack(f'<{sample_count}h', data[:sample_count * 2])
        rms = math.sqrt(sum(sample * sample for sample in samples) /
                        sample_count) / 32768.0
        if rms <= 0.0:
            return 0.0

        # Map roughly -52 dB (quiet room) through -12 dB (loud speech) to 0..1.
        decibels = 20.0 * math.log10(rms)
        return max(0.0, min(1.0, (decibels + 52.0) / 40.0))

    def _poll_levels(self):
        if self.state != 'recording':
            return False

        if self.preview:
            phase = time.monotonic() * 7.0
            levels = [
                0.20 + 0.62 * ((math.sin(phase + index * 0.85) + 1.0) / 2.0)
                for index in range(NUM_BARS)
            ]
            self.waveform.set_levels(levels)
            return True

        target = self._read_audio_level()
        attack = 0.55 if target > self._smoothed_level else 0.22
        self._smoothed_level += (target - self._smoothed_level) * attack
        self._level_history.insert(0, self._smoothed_level)
        self._level_history.pop()

        center = NUM_BARS // 2
        levels = []
        for index in range(NUM_BARS):
            distance = abs(index - center)
            trail = self._level_history[distance]
            levels.append(trail * (1.0 - distance * 0.035))
        self.waveform.set_levels(levels)
        return True

    def _on_transcribing_signal(self):
        if self.state != 'recording':
            return GLib.SOURCE_CONTINUE
        self.state = 'transcribing'
        self.get_accessible().set_name('Codex Voice transcribing')
        GLib.idle_add(self._switch_to_transcribing)
        return GLib.SOURCE_CONTINUE

    def _switch_to_transcribing(self):
        # Keep the waveform allocated as the fixed-width status slot so the
        # trailing cancel button cannot move or fall outside the pill. Clearing
        # its bars also avoids XWayland transparency artifacts caused by hiding
        # or changing the opacity of a child in an RGBA window.
        self.waveform.start_spinner()
        self.queue_draw()
        GLib.idle_add(self._position_window)
        return GLib.SOURCE_REMOVE

    def _cancel(self, _button=None):
        if self.state == 'cancelled':
            return
        previous_state = self.state
        self.state = 'cancelled'
        self.cancel_button.set_sensitive(False)
        action = ([self.control_command, '--close-preview'] if self.preview else
                  [self.control_command, '--cancel-recording',
                   str(self.recorder_pid), str(self.recorder_start_time)])
        try:
            process = subprocess.Popen(
                action,
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                close_fds=True)
        except OSError:
            self.state = previous_state
            self.cancel_button.set_sensitive(True)
            return

        def control_finished(_pid, status):
            self.control_process = None
            if os.waitstatus_to_exitcode(status) == 0:
                self.destroy()
                return
            self.state = previous_state
            self.cancel_button.set_sensitive(True)

        self.control_process = process
        GLib.child_watch_add(process.pid, control_finished)

    def _on_quit_signal(self):
        self.destroy()
        return GLib.SOURCE_REMOVE

    def _on_destroy(self, _widget):
        if not self.preview and self.state == 'recording':
            self.state = 'cancelled'
            try:
                subprocess.Popen(
                    [self.control_command, '--cancel-recording',
                     str(self.recorder_pid), str(self.recorder_start_time)],
                    stdin=subprocess.DEVNULL,
                    stdout=subprocess.DEVNULL,
                    stderr=subprocess.DEVNULL,
                    close_fds=True)
            except OSError:
                pass
        Gtk.main_quit()


def parse_args():
    parser = argparse.ArgumentParser(description='codex-voice recording overlay')
    parser.add_argument('--audio-file')
    parser.add_argument('--control-command', required=True)
    parser.add_argument('--preview', action='store_true')
    parser.add_argument('--recorder-pid', type=int)
    parser.add_argument('--recorder-start-time', type=int)
    args = parser.parse_args()
    if not args.preview and (args.recorder_pid is None or
                             args.recorder_start_time is None):
        parser.error('recording overlays require recorder identity')
    return args


if __name__ == '__main__':
    args = parse_args()
    overlay = [None]

    def transcribing_signal():
        return overlay[0]._on_transcribing_signal() if overlay[0] else GLib.SOURCE_CONTINUE

    def quit_signal():
        return overlay[0]._on_quit_signal() if overlay[0] else GLib.SOURCE_CONTINUE

    # Register before GTK construction so an immediate stop cannot terminate
    # the process through SIGUSR1's default action.
    GLibUnix.signal_add(GLib.PRIORITY_DEFAULT, signal.SIGUSR1, transcribing_signal)
    GLibUnix.signal_add(GLib.PRIORITY_DEFAULT, signal.SIGTERM, quit_signal)
    GLibUnix.signal_add(GLib.PRIORITY_DEFAULT, signal.SIGINT, quit_signal)
    overlay[0] = Overlay(
        args.audio_file, args.control_command, args.preview,
        args.recorder_pid, args.recorder_start_time)
    if _EARLY_TRANSCRIBE:
        overlay[0]._on_transcribing_signal()
    Gtk.main()
