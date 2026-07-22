use std::env;
use std::fs::OpenOptions;
use std::io;
use std::os::unix::net::{UnixDatagram, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) const INSPECTION_OUTPUT_LIMIT: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Protocol {
    Legacy,
    Modern,
}

pub(crate) struct PasteCommand {
    args: &'static [&'static str],
}

impl PasteCommand {
    pub(crate) fn args(&self) -> &'static [&'static str] {
        self.args
    }
}

pub(crate) fn inspection_command(ydotool: &Path) -> Command {
    let mut command = Command::new(ydotool);
    command.arg("--help");
    unsafe {
        command.pre_exec(|| {
            let limit = libc::rlimit {
                rlim_cur: INSPECTION_OUTPUT_LIMIT as libc::rlim_t,
                rlim_max: INSPECTION_OUTPUT_LIMIT as libc::rlim_t,
            };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
}

pub(crate) fn prepare(help: &[u8]) -> io::Result<PasteCommand> {
    let protocol = classify_help(&String::from_utf8_lossy(help)).ok_or_else(|| {
        unavailable(
            "the installed ydotool version is unsupported; run `sudo apt install --reinstall codex-voice`, then log out and back in",
        )
    })?;
    ensure_ready(protocol)?;
    Ok(PasteCommand {
        args: match protocol {
            Protocol::Legacy => &["key", "Shift+Insert"],
            Protocol::Modern => &["key", "42:1", "110:1", "110:0", "42:0"],
        },
    })
}

pub(crate) fn missing() -> io::Error {
    unavailable(
        "ydotool is not installed; run `sudo apt install --reinstall codex-voice`, then log out and back in",
    )
}

pub(crate) fn inspection_failed(detail: impl AsRef<str>) -> io::Error {
    unavailable(detail)
}

fn classify_help(help: &str) -> Option<Protocol> {
    if !help.contains("Available commands:") {
        return None;
    }
    if help.lines().any(|line| line.trim() == "recorder") {
        return Some(Protocol::Legacy);
    }
    if help.lines().any(|line| line.trim() == "debug")
        && help.lines().any(|line| line.trim() == "bakers")
    {
        return Some(Protocol::Modern);
    }
    None
}

fn ensure_ready(protocol: Protocol) -> io::Result<()> {
    let socket = match protocol {
        Protocol::Legacy => PathBuf::from("/tmp/.ydotool_socket"),
        Protocol::Modern => env::var_os("YDOTOOL_SOCKET")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                env::var_os("XDG_RUNTIME_DIR")
                    .filter(|value| !value.is_empty())
                    .map(|runtime| PathBuf::from(runtime).join(".ydotool_socket"))
            })
            .unwrap_or_else(|| PathBuf::from("/tmp/.ydotool_socket")),
    };
    check_ready(protocol, &socket, Path::new("/dev/uinput"))
}

fn check_ready(protocol: Protocol, socket: &Path, uinput: &Path) -> io::Result<()> {
    let connection = match protocol {
        Protocol::Legacy => UnixStream::connect(socket).map(drop),
        Protocol::Modern => UnixDatagram::unbound().and_then(|client| client.connect(socket)),
    };
    let Err(socket_error) = connection else {
        return Ok(());
    };

    let service = match protocol {
        Protocol::Legacy => "codex-voice-ydotoold.service",
        Protocol::Modern => "ydotool.service",
    };
    let restart = format!("`systemctl --user restart {service}`");
    let detail = match OpenOptions::new().read(true).write(true).open(uinput) {
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => format!(
            "this GNOME session cannot access {} ({error}); log out and back in after installing Codex Voice; if already restarted, run `sudo udevadm control --reload-rules && sudo udevadm trigger --action=change --name-match=/dev/uinput`, then {restart}",
            uinput.display()
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => format!(
            "{} is unavailable ({error}); run `sudo modprobe uinput`, then {restart}",
            uinput.display()
        ),
        _ => format!(
            "ydotoold is not accepting connections at {} ({socket_error}); run {restart}; if it fails, inspect `systemctl --user status {service}`",
            socket.display()
        ),
    };
    Err(unavailable(detail))
}

fn unavailable(detail: impl AsRef<str>) -> io::Error {
    io::Error::other(format!(
        "automatic paste was not attempted: {}. The transcript remains in History and on the clipboard",
        detail.as_ref()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::os::unix::net::UnixListener;

    fn test_directory(name: &str) -> PathBuf {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tmp")
            .join(format!("paste-automation-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn recognizes_supported_ubuntu_protocols() {
        let legacy = "Available commands:\n  type\n  recorder\n  mousemove\n  key\n  click\n";
        let modern = "Available commands:\n  click\n  key\n  debug\n  bakers\n";
        assert_eq!(classify_help(legacy), Some(Protocol::Legacy));
        assert_eq!(classify_help(modern), Some(Protocol::Modern));
        assert_eq!(classify_help("Available commands:\n  key\n"), None);
    }

    #[test]
    fn accepts_legacy_stream_and_modern_datagram_sockets() {
        let directory = test_directory("sockets");
        let legacy_socket = directory.join("legacy.socket");
        let modern_socket = directory.join("modern.socket");
        let _legacy_listener = UnixListener::bind(&legacy_socket).unwrap();
        let _modern_listener = UnixDatagram::bind(&modern_socket).unwrap();

        check_ready(Protocol::Legacy, &legacy_socket, Path::new("/missing")).unwrap();
        check_ready(Protocol::Modern, &modern_socket, Path::new("/missing")).unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn unavailable_daemon_diagnostic_is_actionable_and_preserves_transcript() {
        let directory = test_directory("diagnostic");
        let uinput = directory.join("uinput");
        File::create(&uinput).unwrap();
        let error = check_ready(Protocol::Modern, &directory.join("missing.socket"), &uinput)
            .unwrap_err()
            .to_string();

        assert!(error.contains("automatic paste was not attempted"));
        assert!(error.contains("systemctl --user restart ydotool.service"));
        assert!(error.contains("History and on the clipboard"));
        fs::remove_dir_all(directory).unwrap();
    }
}
