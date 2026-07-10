#!/usr/bin/env python3
"""codex-voice overlay — recording pill with live waveform.

A transparent GTK3 window showing a Handy-style recording pill at the
bottom-center of the screen.  Displays animated waveform bars while
recording and switches to a pulsing "Transcribing…" label on SIGUSR1.
"""
import gi
gi.require_version('Gtk', '3.0')
gi.require_foreign('cairo')
from gi.repository import Gtk, Gdk, GLib
import cairo
import signal
import subprocess
import struct
import random
import os
import fcntl

signal.signal(signal.SIGINT, signal.SIG_DFL)
signal.signal(signal.SIGTERM, signal.SIG_DFL)

PILL_W = 172
PILL_H = 36
MARGIN_BOTTOM = 40
NUM_BARS = 9
BAR_W = 6
BAR_GAP = 3
BAR_MAX_H = 20
BAR_MIN_H = 4
LEVEL_POLL_MS = 50

COLOR_ICON = '#30D158'
COLOR_BARS = (0.82, 0.98, 0.88, 1.0)
COLOR_CANCEL = '#30D158'


class Waveform(Gtk.DrawingArea):
    """DrawingArea that renders animated waveform bars."""

    def __init__(self, num_bars, bar_w, bar_gap, max_h, min_h):
        super().__init__()
        self.num_bars = num_bars
        self.bar_w = bar_w
        self.bar_gap = bar_gap
        self.max_h = max_h
        self.min_h = min_h
        self.levels = [0.0] * num_bars
        self.set_size_request(
            num_bars * bar_w + (num_bars - 1) * bar_gap, max_h)
        self.set_valign(Gtk.Align.CENTER)
        self.set_halign(Gtk.Align.CENTER)
        self.set_hexpand(True)
        self.connect('draw', self._on_draw)

    def set_levels(self, levels):
        self.levels = levels
        self.queue_draw()

    def _on_draw(self, _w, cr):
        for i, level in enumerate(self.levels):
            h = self.min_h + level * (self.max_h - self.min_h)
            h = max(self.min_h, min(self.max_h, h))
            x = i * (self.bar_w + self.bar_gap)
            y = (self.max_h - h) / 2
            cr.set_source_rgba(*COLOR_BARS)
            cr.rectangle(x, y, self.bar_w, h)
            cr.fill()
        return False


class Overlay(Gtk.Window):
    """Recording overlay window with live waveform and transcribing state."""

    def __init__(self):
        super().__init__(type=Gtk.WindowType.TOPLEVEL)
        self.set_decorated(False)
        self.set_app_paintable(True)
        self.set_resizable(False)
        self.set_skip_taskbar_hint(True)
        self.set_skip_pager_hint(True)
        self.set_type_hint(Gdk.WindowTypeHint.NOTIFICATION)
        self.set_keep_above(True)
        self.set_accept_focus(False)
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
            background-color: rgba(0, 0, 0, 0.80);
            border-radius: 18px;
            padding: 6px;
        }}
        .mic-icon {{
            color: {COLOR_ICON};
        }}
        .transcribing-label {{
            color: #ffffff;
            font-size: 12px;
            font-family: -apple-system, "Segoe UI", "Ubuntu Sans", sans-serif;
        }}
        .cancel-btn {{
            min-width: 24px;
            min-height: 24px;
            padding: 0;
            margin: 0;
            border: none;
            border-radius: 50%;
            background: transparent;
            color: {COLOR_CANCEL};
            font-size: 16px;
        }}
        """.encode()
        provider = Gtk.CssProvider()
        provider.load_from_data(css)
        Gtk.StyleContext.add_provider_for_screen(
            screen, provider, Gtk.STYLE_PROVIDER_PRIORITY_USER)

        self.state = "recording"
        self._build_ui()

        self.connect('realize', self._on_realize)
        self.connect('destroy', Gtk.main_quit)

        self._smoothed = [0.0] * NUM_BARS
        self._arecord_proc = None
        self._pulse_up = False

        self.show_all()
        self._position_window()

        GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, signal.SIGUSR1,
                             self._on_transcribing_signal)

        self._start_level_monitor()
        GLib.timeout_add(LEVEL_POLL_MS, self._poll_levels)

    def _build_ui(self):
        pill = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL, spacing=6)
        pill.get_style_context().add_class('pill')
        pill.set_valign(Gtk.Align.CENTER)
        pill.set_size_request(PILL_W, PILL_H)

        self.mic_icon = Gtk.Image.new_from_icon_name(
            'audio-input-microphone-symbolic', Gtk.IconSize.SMALL_TOOLBAR)
        self.mic_icon.get_style_context().add_class('mic-icon')
        pill.pack_start(self.mic_icon, False, False, 0)

        self.waveform = Waveform(
            NUM_BARS, BAR_W, BAR_GAP, BAR_MAX_H, BAR_MIN_H)
        pill.pack_start(self.waveform, True, True, 0)

        self.transcribing_label = Gtk.Label(label="Transcribing\u2026")
        self.transcribing_label.get_style_context().add_class(
            'transcribing-label')
        self.transcribing_label.set_no_show_all(True)
        pill.pack_start(self.transcribing_label, True, True, 0)

        cancel = Gtk.Button(label="\u2715")
        cancel.get_style_context().add_class('cancel-btn')
        cancel.set_relief(Gtk.ReliefStyle.NONE)
        cancel.connect('clicked', lambda _: Gtk.main_quit())
        pill.pack_start(cancel, False, False, 0)

        self.add(pill)

    def _on_realize(self, _w):
        self._position_window()

    def _position_window(self):
        screen = self.get_screen()
        mon = screen.get_primary_monitor()
        if mon < 0:
            mon = 0
        geo = screen.get_monitor_geometry(mon)
        x = geo.x + (geo.width - PILL_W) // 2
        y = geo.y + geo.height - PILL_H - MARGIN_BOTTOM
        self.move(x, y)
        return False

    def _start_level_monitor(self):
        try:
            self._arecord_proc = subprocess.Popen(
                ["arecord", "-q", "-f", "S16_LE", "-r", "8000",
                 "-c", "1", "-t", "raw"],
                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
            fd = self._arecord_proc.stdout.fileno()
            flags = fcntl.fcntl(fd, fcntl.F_GETFL)
            fcntl.fcntl(fd, fcntl.F_SETFL, flags | os.O_NONBLOCK)
        except Exception:
            self._arecord_proc = None

    def _poll_levels(self):
        if self.state != "recording":
            return False

        peak = 0.0
        if self._arecord_proc and self._arecord_proc.stdout:
            try:
                data = os.read(self._arecord_proc.stdout.fileno(), 3200)
                if data:
                    n = len(data) // 2
                    vals = struct.unpack(f'<{n}h', data[:n * 2])
                    peak = max(abs(v) for v in vals) / 32768.0
            except (BlockingIOError, OSError):
                pass
            except Exception:
                pass

        for i in range(NUM_BARS):
            if peak > 0.005:
                center = (NUM_BARS - 1) / 2.0
                dist = abs(i - center) / center if center > 0 else 0
                decay = 1.0 - dist * 0.35
                target = peak * decay * (0.6 + random.random() * 0.4)
                target = min(1.0, target * 3.0)
            else:
                target = 0.0
            self._smoothed[i] = self._smoothed[i] * 0.6 + target * 0.4

        self.waveform.set_levels(self._smoothed)
        return True

    def _on_transcribing_signal(self):
        self.state = "transcribing"
        GLib.idle_add(self._switch_to_transcribing)
        return False

    def _switch_to_transcribing(self):
        self.waveform.hide()
        self.mic_icon.hide()
        self.transcribing_label.show()

        if self._arecord_proc:
            self._arecord_proc.terminate()
            try:
                self._arecord_proc.wait(timeout=1)
            except Exception:
                self._arecord_proc.kill()
            self._arecord_proc = None

        GLib.timeout_add(400, self._pulse_label)
        return False

    def _pulse_label(self):
        if self.state != "transcribing":
            return False
        op = self.transcribing_label.get_opacity()
        if self._pulse_up:
            op = min(1.0, op + 0.1)
            if op >= 1.0:
                self._pulse_up = False
        else:
            op = max(0.5, op - 0.1)
            if op <= 0.5:
                self._pulse_up = True
        self.transcribing_label.set_opacity(op)
        return True


if __name__ == '__main__':
    Overlay()
    Gtk.main()
