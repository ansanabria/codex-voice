use crate::{remove, write_atomic};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProcessIdentity {
    pub(crate) pid: i32,
    pub(crate) start_time: u64,
}

impl ProcessIdentity {
    pub(crate) fn current() -> io::Result<Self> {
        Self::for_pid(std::process::id() as i32)
            .ok_or_else(|| io::Error::other("could not read current process identity from /proc"))
    }

    pub(crate) fn for_pid(pid: i32) -> Option<Self> {
        (pid > 0).then_some(Self {
            pid,
            start_time: proc_start_time(pid)?,
        })
    }

    pub(crate) fn is_alive(self) -> bool {
        Self::for_pid(self.pid) == Some(self)
    }
}

pub(crate) fn read_record(path: &Path) -> Option<ProcessIdentity> {
    let contents = fs::read(path).ok()?;
    let identity = serde_json::from_slice::<ProcessIdentity>(&contents).ok();
    if identity.is_none() {
        remove_if_contents(path, &contents);
    }
    identity
}

pub(crate) fn read_active_record(path: &Path) -> Option<ProcessIdentity> {
    let identity = read_record(path)?;
    if identity.is_alive() {
        Some(identity)
    } else {
        remove_record_if(path, identity);
        None
    }
}

pub(crate) fn write_record(path: &Path, identity: ProcessIdentity) -> io::Result<()> {
    let bytes = serde_json::to_vec(&identity).expect("process identity is serializable");
    write_atomic(path, &bytes)
}

pub(crate) fn remove_record_if(path: &Path, expected: ProcessIdentity) -> bool {
    if read_record(path) == Some(expected) {
        remove(path);
        true
    } else {
        false
    }
}

pub(crate) fn signal(identity: ProcessIdentity, signal_number: libc::c_int) -> io::Result<bool> {
    let Some(pidfd) = open_pidfd(identity)? else {
        return Ok(false);
    };
    let result = signal_pidfd(pidfd, signal_number);
    unsafe { libc::close(pidfd) };
    result
}

pub(crate) fn open_pidfd(identity: ProcessIdentity) -> io::Result<Option<libc::c_int>> {
    if !identity.is_alive() {
        return Ok(None);
    }
    let pidfd = unsafe { libc::syscall(libc::SYS_pidfd_open, identity.pid, 0) as libc::c_int };
    if pidfd < 0 {
        let error = io::Error::last_os_error();
        return if matches!(error.raw_os_error(), Some(libc::ESRCH)) {
            Ok(None)
        } else {
            Err(error)
        };
    }
    if identity.is_alive() {
        Ok(Some(pidfd))
    } else {
        unsafe { libc::close(pidfd) };
        Ok(None)
    }
}

pub(crate) fn wait_for_exit(identity: ProcessIdentity) -> io::Result<()> {
    let Some(pidfd) = open_pidfd(identity)? else {
        return Ok(());
    };
    let mut poll_fd = libc::pollfd {
        fd: pidfd,
        events: libc::POLLIN,
        revents: 0,
    };
    loop {
        let result = unsafe { libc::poll(&mut poll_fd, 1, -1) };
        if result >= 0 {
            unsafe { libc::close(pidfd) };
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            unsafe { libc::close(pidfd) };
            return Err(error);
        }
    }
}

pub(crate) fn signal_pidfd(pidfd: libc::c_int, signal_number: libc::c_int) -> io::Result<bool> {
    let result = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            pidfd,
            signal_number,
            std::ptr::null::<libc::siginfo_t>(),
            0,
        )
    };
    let error = (result < 0).then(io::Error::last_os_error);
    match error {
        Some(error) if matches!(error.raw_os_error(), Some(libc::ESRCH)) => Ok(false),
        Some(error) => Err(error),
        None => Ok(true),
    }
}

pub(crate) fn terminate(identity: ProcessIdentity, first_signal: libc::c_int) -> io::Result<bool> {
    if !signal(identity, first_signal)? {
        return Ok(true);
    }
    if wait_until_gone(identity, Duration::from_millis(750)) {
        return Ok(true);
    }
    let _ = signal(identity, libc::SIGKILL)?;
    Ok(wait_until_gone(identity, Duration::from_millis(750)))
}

fn wait_until_gone(identity: ProcessIdentity, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while identity.is_alive() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    !identity.is_alive()
}

fn remove_if_contents(path: &Path, expected: &[u8]) {
    if fs::read(path).is_ok_and(|contents| contents == expected) {
        remove(path);
    }
}

fn proc_start_time(pid: i32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let mut fields = stat.rsplit_once(") ")?.1.split_whitespace();
    fields.nth(19)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    fn temporary_path(name: &str) -> std::path::PathBuf {
        let temporary = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tmp");
        fs::create_dir_all(&temporary).unwrap();
        temporary.join(format!(
            "codex-voice-{name}-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
    }

    #[test]
    fn old_pid_only_record_is_stale() {
        let path = temporary_path("old-pid");
        fs::write(&path, b"123\n").unwrap();
        assert_eq!(read_record(&path), None);
        assert!(!path.exists());
    }

    #[test]
    fn identity_mismatch_is_never_signalled() {
        let mut child = Command::new("sleep")
            .arg("30")
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let actual = ProcessIdentity::for_pid(child.id() as i32).unwrap();
        let mismatched = ProcessIdentity {
            start_time: actual.start_time + 1,
            ..actual
        };
        assert!(!signal(mismatched, libc::SIGTERM).unwrap());
        assert!(child.try_wait().unwrap().is_none());
        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[test]
    fn owner_checked_removal_preserves_replacement() {
        let path = temporary_path("owned-remove");
        let first = ProcessIdentity {
            pid: 10,
            start_time: 20,
        };
        let replacement = ProcessIdentity {
            pid: 30,
            start_time: 40,
        };
        write_record(&path, first).unwrap();
        write_record(&path, replacement).unwrap();
        assert!(!remove_record_if(&path, first));
        assert_eq!(read_record(&path), Some(replacement));
        remove(&path);
    }
}
