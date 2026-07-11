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
import time

import gi

gi.require_version('Gtk', '3.0')
gi.require_version('Gdk', '3.0')
gi.require_version('GLibUnix', '2.0')
gi.require_foreign('cairo')
from gi.repository import Gdk, GLib, GLibUnix, Gtk
import cairo


PILL_W = 176
PILL_H = 38
MARGIN_BOTTOM = 36
NUM_BARS = 11
BAR_W = 4
BAR_GAP = 3
BAR_MAX_H = 22
BAR_MIN_H = 4
LEVEL_POLL_MS = 50
WAV_HEADER_SIZE = 44
LEVEL_BYTES = 4096

COLOR_CANCEL = '#8E9692'


def _normalize_color(value, fallback):
    """Return a CSS #rrggbbaa color; invalid persisted values stay harmless."""
    value = (value or '').strip().lower()
    if value.startswith('#'):
        value = value[1:]
    if len(value) == 3 and all(char in '0123456789abcdef' for char in value):
        value = ''.join(char * 2 for char in value) + 'ff'
    elif len(value) == 4 and all(char in '0123456789abcdef' for char in value):
        value = ''.join(char * 2 for char in value)
    elif len(value) == 6 and all(char in '0123456789abcdef' for char in value):
        value += 'ff'
    elif len(value) != 8 or not all(char in '0123456789abcdef' for char in value):
        return fallback
    return '#' + value


def _hex_to_rgb(color):
    color = _normalize_color(color, '#10a37fff')[1:7]
    return tuple(int(color[index:index + 2], 16) / 255.0
                 for index in range(0, 6, 2))


def _to_gtk_css_color(color, fallback):
    """Translate persisted #rrggbbaa values for GTK 3's CSS parser."""
    color = _normalize_color(color, fallback)[1:]
    red, green, blue, alpha = (int(color[index:index + 2], 16)
                               for index in range(0, 8, 2))
    return f'rgba({red}, {green}, {blue}, {alpha / 255.0:.3f})'


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
        width = num_bars * bar_w + (num_bars - 1) * bar_gap
        self.set_size_request(width, max_h)
        self.set_valign(Gtk.Align.CENTER)
        self.set_halign(Gtk.Align.CENTER)
        self.set_hexpand(True)
        self.connect('draw', self._on_draw)

    def set_levels(self, levels):
        self.levels = levels
        self.queue_draw()

    def _on_draw(self, _widget, cr):
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

    def __init__(self, audio_file=None, recorder_pid_file=None,
                 overlay_pid_file=None, transcriber_pid_file=None,
                 cancel_file=None, background_color='#0f0f0feb',
                 accent_color='#10a37fff', state_file=None):
        # The launcher uses XWayland because native Wayland clients cannot place
        # toplevels in global coordinates. Keep the overlay non-focusable so the
        # eventual transcription is typed into the window the user started in.
        super().__init__(type=Gtk.WindowType.TOPLEVEL)
        self.audio_file = audio_file
        self.recorder_pid_file = recorder_pid_file
        self.overlay_pid_file = overlay_pid_file
        self.transcriber_pid_file = transcriber_pid_file
        self.cancel_file = cancel_file
        self.state_file = state_file
        self.background_color = _normalize_color(background_color, '#0f0f0feb')
        self.accent_color = _normalize_color(accent_color, '#10a37fff')
        background_css = _to_gtk_css_color(self.background_color, '#0f0f0feb')
        accent_css = _to_gtk_css_color(self.accent_color, '#10a37fff')
        self.state = 'recording'
        self._level_history = [0.0] * ((NUM_BARS // 2) + 1)
        self._smoothed_level = 0.0
        self._pulse_up = False

        self.set_title('Codex Voice')
        self.set_decorated(False)
        self.set_app_paintable(True)
        self.set_resizable(False)
        self.set_skip_taskbar_hint(True)
        self.set_skip_pager_hint(True)
        self.set_type_hint(Gdk.WindowTypeHint.NOTIFICATION)
        self.set_keep_above(True)
        self.set_accept_focus(False)
        self.set_focus_on_map(False)
        self.set_can_focus(False)
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
            background-color: {background_css};
            border: 1px solid rgba(255, 255, 255, 0.10);
            border-radius: 19px;
            padding: 6px 8px;
        }}
        .mic-icon {{
            color: {accent_css};
        }}
        .transcribing-label {{
            color: #f4f7f5;
            font-size: 12px;
            font-family: -apple-system, "Segoe UI", "Ubuntu Sans", sans-serif;
        }}
        .cancel-btn {{
            min-width: 24px;
            min-height: 24px;
            padding: 0;
            margin: 0;
            border: none;
            border-radius: 12px;
            background: transparent;
            color: {COLOR_CANCEL};
            font-size: 14px;
        }}
        .cancel-btn:hover {{
            background: rgba(255, 255, 255, 0.10);
            color: #ffffff;
        }}
        """.encode()
        provider = Gtk.CssProvider()
        provider.load_from_data(css)
        Gtk.StyleContext.add_provider_for_screen(
            screen, provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)

        self._build_ui()
        self.connect('map-event', self._on_map)
        self.connect('destroy', self._on_destroy)

        self.show_all()
        self._position_window()

        GLibUnix.signal_add(GLib.PRIORITY_DEFAULT, signal.SIGUSR1,
                            self._on_transcribing_signal)
        GLibUnix.signal_add(GLib.PRIORITY_DEFAULT, signal.SIGTERM,
                            self._on_quit_signal)
        GLibUnix.signal_add(GLib.PRIORITY_DEFAULT, signal.SIGINT,
                            self._on_quit_signal)
        GLib.timeout_add(LEVEL_POLL_MS, self._poll_levels)

    def _build_ui(self):
        pill = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=7)
        pill.get_style_context().add_class('pill')
        pill.set_valign(Gtk.Align.CENTER)
        pill.set_size_request(PILL_W, PILL_H)

        self.mic_icon = Gtk.Image.new_from_icon_name(
            'audio-input-microphone-symbolic', Gtk.IconSize.SMALL_TOOLBAR)
        self.mic_icon.get_style_context().add_class('mic-icon')
        pill.pack_start(self.mic_icon, False, False, 0)

        self.waveform = Waveform(
            NUM_BARS, BAR_W, BAR_GAP, BAR_MAX_H, BAR_MIN_H,
            _hex_to_rgb(self.accent_color))
        pill.pack_start(self.waveform, True, True, 0)

        self.transcribing_label = Gtk.Label(label='Transcribing\u2026')
        self.transcribing_label.get_style_context().add_class(
            'transcribing-label')
        self.transcribing_label.set_no_show_all(True)
        pill.pack_start(self.transcribing_label, True, True, 0)

        cancel = Gtk.Button(label='\u00d7')
        cancel.set_tooltip_text('Cancel')
        cancel.get_style_context().add_class('cancel-btn')
        cancel.set_relief(Gtk.ReliefStyle.NONE)
        cancel.connect('clicked', self._cancel)
        pill.pack_start(cancel, False, False, 0)

        self.add(pill)

    def _on_map(self, _widget, _event):
        self._position_window()
        # Window managers may ignore the initial placement request while the
        # surface is being mapped, so repeat it after allocation settles.
        GLib.timeout_add(100, self._position_window)
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
        GLib.idle_add(self._switch_to_transcribing)
        return GLib.SOURCE_CONTINUE

    def _switch_to_transcribing(self):
        self.waveform.hide()
        self.mic_icon.hide()
        self.transcribing_label.set_opacity(1.0)
        self.transcribing_label.show()
        GLib.idle_add(self._position_window)
        GLib.timeout_add(60, self._pulse_label)
        return GLib.SOURCE_REMOVE

    def _pulse_label(self):
        if self.state != 'transcribing':
            return False
        opacity = self.transcribing_label.get_opacity()
        step = 0.035
        if self._pulse_up:
            opacity = min(1.0, opacity + step)
            if opacity >= 1.0:
                self._pulse_up = False
        else:
            opacity = max(0.55, opacity - step)
            if opacity <= 0.55:
                self._pulse_up = True
        self.transcribing_label.set_opacity(opacity)
        return True

    def _cancel(self, _button=None):
        if self.state == 'cancelled':
            return
        previous_state = self.state
        self.state = 'cancelled'
        self._mark_cancelled()
        self._unlink(self.state_file)

        recorder_pid = self._read_pid(self.recorder_pid_file)
        if recorder_pid:
            self._signal_process(recorder_pid, signal.SIGINT)

            # Keep the PID and WAV files authoritative until arecord has
            # actually released the microphone. If shutdown takes unusually
            # long, the next launcher invocation can still find and stop it.
            for _ in range(20):
                if not self._process_exists(recorder_pid):
                    self._unlink(self.recorder_pid_file)
                    self._unlink(self.audio_file)
                    break
                time.sleep(0.05)

        if previous_state == 'transcribing':
            transcriber_pid = self._read_pid(self.transcriber_pid_file)
            if transcriber_pid:
                self._signal_process(transcriber_pid, signal.SIGTERM)
            self._unlink(self.transcriber_pid_file)

        self.destroy()

    def _mark_cancelled(self):
        if not self.cancel_file:
            return
        try:
            # O_EXCL is unnecessary: the marker is deliberately idempotent.
            fd = os.open(self.cancel_file,
                         os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
            os.close(fd)
        except OSError:
            pass

    def _on_quit_signal(self):
        self.destroy()
        return GLib.SOURCE_REMOVE

    def _on_destroy(self, _widget):
        overlay_pid = self._read_pid(self.overlay_pid_file)
        if overlay_pid == os.getpid():
            self._unlink(self.overlay_pid_file)
        Gtk.main_quit()

    @staticmethod
    def _read_pid(path):
        if not path:
            return None
        try:
            with open(path, encoding='ascii') as pid_file:
                return int(pid_file.read().strip())
        except (FileNotFoundError, OSError, TypeError, ValueError):
            return None

    @staticmethod
    def _unlink(path):
        if not path:
            return
        try:
            os.unlink(path)
        except FileNotFoundError:
            pass

    @staticmethod
    def _signal_process(pid, sig):
        try:
            os.kill(pid, sig)
        except (ProcessLookupError, PermissionError):
            pass

    @staticmethod
    def _process_exists(pid):
        try:
            os.kill(pid, 0)
            return True
        except ProcessLookupError:
            return False
        except PermissionError:
            return True


def parse_args():
    parser = argparse.ArgumentParser(description='codex-voice recording overlay')
    parser.add_argument('--audio-file')
    parser.add_argument('--recorder-pid-file')
    parser.add_argument('--overlay-pid-file')
    parser.add_argument('--transcriber-pid-file')
    parser.add_argument('--cancel-file')
    parser.add_argument('--background-color', default='#0f0f0feb')
    parser.add_argument('--accent-color', default='#10a37fff')
    parser.add_argument('--state-file')
    return parser.parse_args()


if __name__ == '__main__':
    args = parse_args()
    Overlay(args.audio_file, args.recorder_pid_file, args.overlay_pid_file,
            args.transcriber_pid_file, args.cancel_file, args.background_color,
            args.accent_color, args.state_file)
    Gtk.main()
