#!/usr/bin/env python3
from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("codex-voice-session-setup.sh")


class SessionSetupTests(unittest.TestCase):
    def test_enables_once_and_preserves_later_user_disable(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            home = root / "home"
            binaries = root / "bin"
            home.mkdir()
            binaries.mkdir()
            log = root / "extension-calls"
            extension = binaries / "gnome-extensions"
            extension.write_text(
                "#!/bin/sh\n"
                "printf '%s\\n' \"$1\" >> \"$SETUP_TEST_LOG\"\n"
                "if [ \"$1\" = enable ]; then touch \"$SETUP_TEST_ENABLED\"; fi\n"
            )
            extension.chmod(0o755)
            dconf = binaries / "dconf"
            dconf.write_text("#!/bin/sh\nexit 0\n")
            dconf.chmod(0o755)
            gsettings = binaries / "gsettings"
            gsettings.write_text(
                "#!/bin/sh\n"
                "if [ \"$3\" = enabled-extensions ] && [ -e \"$SETUP_TEST_ENABLED\" ]; then\n"
                "  printf \"['codex-voice@andy-spike.github.io']\\n\"\n"
                "elif [ \"$3\" = disabled-extensions ] && [ \"${SETUP_TEST_DISABLED:-}\" = 1 ]; then\n"
                "  printf \"['codex-voice@andy-spike.github.io']\\n\"\n"
                "else\n"
                "  printf '@as []\\n'\n"
                "fi\n"
            )
            gsettings.chmod(0o755)
            enabled = root / "extension-enabled"
            environment = dict(
                os.environ,
                HOME=str(home),
                PATH=f"{binaries}:/usr/bin:/bin",
                SETUP_TEST_LOG=str(log),
                SETUP_TEST_ENABLED=str(enabled),
            )

            subprocess.run([SCRIPT], env=environment, check=True)
            subprocess.run([SCRIPT], env=environment, check=True)

            self.assertEqual(log.read_text().splitlines(), ["enable"])
            self.assertTrue(
                (home / ".config/codex-voice/extension-enabled-0.2.0").is_file()
            )

    def test_preserves_explicit_disable_without_enabling(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            home = root / "home"
            binaries = root / "bin"
            home.mkdir()
            binaries.mkdir()
            log = root / "extension-calls"
            extension = binaries / "gnome-extensions"
            extension.write_text(
                "#!/bin/sh\nprintf '%s\\n' \"$1\" >> \"$SETUP_TEST_LOG\"\n"
            )
            extension.chmod(0o755)
            dconf = binaries / "dconf"
            dconf.write_text("#!/bin/sh\nexit 0\n")
            dconf.chmod(0o755)
            gsettings = binaries / "gsettings"
            gsettings.write_text(
                "#!/bin/sh\n"
                "if [ \"$3\" = disabled-extensions ]; then\n"
                "  printf \"['codex-voice@andy-spike.github.io']\\n\"\n"
                "else\n"
                "  printf '@as []\\n'\n"
                "fi\n"
            )
            gsettings.chmod(0o755)
            environment = dict(
                os.environ,
                HOME=str(home),
                PATH=f"{binaries}:/usr/bin:/bin",
                SETUP_TEST_LOG=str(log),
            )

            subprocess.run([SCRIPT], env=environment, check=True)

            self.assertFalse(log.exists())
            self.assertTrue(
                (home / ".config/codex-voice/extension-enabled-0.2.0").is_file()
            )


if __name__ == "__main__":
    unittest.main()
