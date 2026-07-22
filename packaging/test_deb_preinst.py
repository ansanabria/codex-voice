#!/usr/bin/env python3
from __future__ import annotations

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("deb-preinst.sh")
BOOT_TIME = 1_000
TICKS_PER_SECOND = os.sysconf("SC_CLK_TCK")


class DebPreinstTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.run_root = self.root / "run/user"
        self.tmp_dir = self.root / "tmp"
        self.proc_root = self.root / "proc"
        self.home = self.root / "home/tester"
        self.passwd = self.root / "passwd"
        self.uid = os.getuid()
        self.run_root.mkdir(parents=True)
        self.tmp_dir.mkdir()
        self.proc_root.mkdir()
        (self.proc_root / "stat").write_text(f"btime {BOOT_TIME}\n")
        self.passwd.write_text(
            f"tester:x:{self.uid}:{os.getgid()}::{self.home}:/bin/sh\n"
        )

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def process(self, pid: int, name: str, start_seconds: int = 10) -> int:
        process_dir = self.proc_root / str(pid)
        process_dir.mkdir()
        start_time = start_seconds * TICKS_PER_SECOND
        fields_before_start = ["S"] + ["0"] * 18
        process_dir.joinpath("stat").write_text(
            f"{pid} ({name}) {' '.join(fields_before_start)} {start_time} 0\n"
        )
        process_dir.joinpath("environ").write_bytes(b"")
        return start_time

    def record(self, directory: Path, name: str, contents: str, mtime: int = 2_000) -> Path:
        directory.mkdir(parents=True, exist_ok=True)
        path = directory / name
        path.write_text(contents)
        path.chmod(0o600)
        os.utime(path, (mtime, mtime))
        return path

    def scan(self, override: str = "") -> subprocess.CompletedProcess[str]:
        command = (
            'source "$1"; '
            + override
            + 'active_upgrade_pid "$2" "$3" "$4" "$5"'
        )
        return subprocess.run(
            [
                "bash",
                "-c",
                command,
                "preinst-test",
                str(SCRIPT),
                str(self.run_root),
                str(self.tmp_dir),
                str(self.proc_root),
                str(self.passwd),
            ],
            text=True,
            capture_output=True,
            check=False,
        )

    def test_detects_active_legacy_pid_record(self) -> None:
        pid = 4_201
        self.process(pid, "codex-voice")
        self.record(
            self.tmp_dir,
            "codex-voice-session-owner.pid",
            f"{pid}\n",
        )

        result = self.scan()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), str(pid))

    def test_ignores_missing_and_pid_reused_legacy_records(self) -> None:
        runtime = self.run_root / str(self.uid)
        self.record(runtime, "codex-voice.pid", "9999\n")
        pid = 4_202
        self.process(pid, "codex-voice", start_seconds=50)
        self.record(
            runtime,
            "codex-voice-session-owner.pid",
            f"{pid}\n",
            mtime=BOOT_TIME + 20,
        )

        result = self.scan()

        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(result.stdout, "")

    def test_ignores_malformed_records(self) -> None:
        runtime = self.run_root / str(self.uid)
        self.record(runtime, "codex-voice.pid", "not a pid")
        self.record(
            runtime,
            "codex-voice-session-owner.pid",
            '{"pid":4203,"startTime":"10"}',
        )

        result = self.scan()

        self.assertEqual(result.returncode, 1, result.stderr)

    def test_detects_current_json_identity_and_rejects_reused_pid(self) -> None:
        runtime = self.run_root / str(self.uid)
        stale_pid = 4_204
        stale_start = self.process(stale_pid, "arecord")
        self.record(
            runtime,
            "codex-voice.pid",
            json.dumps(
                {"pid": stale_pid, "startTime": stale_start - 1},
                separators=(",", ":"),
            ),
        )
        active_pid = 4_205
        active_start = self.process(active_pid, "codex-asr")
        self.record(
            runtime,
            "codex-voice-transcriber.pid",
            json.dumps(
                {"pid": active_pid, "startTime": active_start},
                separators=(",", ":"),
            ),
        )

        result = self.scan()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), str(active_pid))

    def test_scans_passwd_cache_fallback(self) -> None:
        pid = 4_206
        start_time = self.process(pid, "arecord")
        self.record(
            self.home / ".cache/codex-voice/runtime",
            "codex-voice.pid",
            f'{{"pid":{pid},"startTime":{start_time}}}',
        )

        result = self.scan()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), str(pid))

    def test_scans_custom_cache_fallback_from_process_environment(self) -> None:
        pid = 4_207
        start_time = self.process(pid, "codex-voice")
        custom_cache = self.root / "custom-cache"
        (self.proc_root / str(pid) / "environ").write_bytes(
            f"XDG_CACHE_HOME={custom_cache}\0HOME={self.home}\0".encode()
        )
        self.record(
            custom_cache / "codex-voice/runtime",
            "codex-voice-session-owner.pid",
            f'{{"pid":{pid},"startTime":{start_time}}}',
        )

        result = self.scan()

        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout.strip(), str(pid))

    def test_ignores_tmp_record_owned_by_unrelated_process_user(self) -> None:
        pid = 4_208
        start_time = self.process(pid, "codex-voice")
        self.record(
            self.tmp_dir,
            "codex-voice-session-owner.pid",
            f'{{"pid":{pid},"startTime":{start_time}}}',
        )
        override = f'process_uid() {{ printf "%s\\n" {self.uid + 1}; }}; '

        result = self.scan(override)

        self.assertEqual(result.returncode, 1, result.stderr)
        self.assertEqual(result.stdout, "")


if __name__ == "__main__":
    unittest.main()
