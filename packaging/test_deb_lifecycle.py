import configparser
import os
import pathlib
import shutil
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parent.parent
AUTOSTART = ROOT / "distribution/io.github.andy_spike.CodexVoice.SessionSetup.desktop"
SESSION_SETUP = ROOT / "packaging/codex-voice-session-setup.sh"


def read_entry(path: pathlib.Path) -> configparser.SectionProxy:
    parser = configparser.ConfigParser(interpolation=None)
    parser.optionxform = str
    with path.open(encoding="utf-8") as entry_file:
        parser.read_file(entry_file)
    return parser["Desktop Entry"]


class DebianAutostartLifecycleTests(unittest.TestCase):
    def test_normal_remove_leaves_guarded_conffile_and_purge_removes_it(self):
        source_entry = read_entry(AUTOSTART)
        self.assertEqual(source_entry["TryExec"], source_entry["Exec"])
        self.assertTrue(source_entry["TryExec"].startswith("/"))

        with tempfile.TemporaryDirectory() as temporary_directory:
            package_root = pathlib.Path(temporary_directory)
            conffile = package_root / "etc/xdg/autostart/io.github.andy_spike.CodexVoice.desktop"
            executable = package_root / source_entry["TryExec"].lstrip("/")
            conffile.parent.mkdir(parents=True)
            executable.parent.mkdir(parents=True)
            shutil.copy2(AUTOSTART, conffile)
            shutil.copy2(SESSION_SETUP, executable)
            executable.chmod(0o755)

            def should_autostart() -> bool:
                entry = read_entry(conffile)
                guarded_executable = package_root / entry["TryExec"].lstrip("/")
                return guarded_executable.is_file() and os.access(guarded_executable, os.X_OK)

            self.assertTrue(should_autostart())

            # dpkg removes package-owned executables but retains conffiles on remove.
            executable.unlink()
            self.assertTrue(conffile.is_file())
            self.assertFalse(should_autostart())

            # Purge remains responsible for deleting the conffile.
            conffile.unlink()
            self.assertFalse(conffile.exists())


if __name__ == "__main__":
    unittest.main()
