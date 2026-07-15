mod product_package;
pub mod protocol;
mod runtime_state;
mod settings;
mod transcript_history;

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command as ProcessCommand, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const EXTENSION_UUID: &str = "codex-voice@andy-spike.github.io";

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static ACTIVE_CHILD: AtomicI32 = AtomicI32::new(0);

extern "C" fn handle_signal(_: libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
    let pid = ACTIVE_CHILD.load(Ordering::SeqCst);
    if pid > 0 {
        // SAFETY: kill is async-signal-safe and the PID is atomic.
        unsafe { libc::kill(pid, libc::SIGTERM) };
    }
}

#[derive(Clone)]
struct Paths {
    recorder_pid: PathBuf,
    wav: PathBuf,
    overlay_pid: PathBuf,
    preview_overlay_pid: PathBuf,
    transcriber_pid: PathBuf,
    transcript: PathBuf,
    cancel: PathBuf,
    typing_pid: PathBuf,
    session_owner_pid: PathBuf,
    runtime_state: PathBuf,
}

impl Paths {
    fn from_environment() -> Self {
        let state_dir = env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        let path = |name: &str| state_dir.join(name);
        Self {
            recorder_pid: path("codex-voice.pid"),
            wav: path("codex-voice.wav"),
            overlay_pid: path("codex-voice-overlay.pid"),
            preview_overlay_pid: path("codex-voice-preview-overlay.pid"),
            transcriber_pid: path("codex-voice-transcriber.pid"),
            transcript: path("codex-voice-transcript.txt"),
            cancel: path("codex-voice-cancelled"),
            typing_pid: path("codex-voice-typing.pid"),
            session_owner_pid: path("codex-voice-session-owner.pid"),
            runtime_state: path("codex-voice-state.json"),
        }
    }
}

struct SessionCleanup<'a> {
    paths: &'a Paths,
    owner_pid: u32,
}

impl Drop for SessionCleanup<'_> {
    fn drop(&mut self) {
        let active = ACTIVE_CHILD.swap(0, Ordering::SeqCst);
        if active > 0 && process_exists(active) {
            signal(active, libc::SIGTERM);
        }
        kill_from_file(&self.paths.overlay_pid, libc::SIGTERM);
        remove(&self.paths.overlay_pid);
        if read_pid(&self.paths.session_owner_pid) == Some(self.owner_pid as i32) {
            remove(&self.paths.session_owner_pid);
        }
        runtime_state::clear(self.paths);
        for path in [
            &self.paths.transcriber_pid,
            &self.paths.typing_pid,
            &self.paths.transcript,
            &self.paths.cancel,
            &self.paths.wav,
        ] {
            remove(path);
        }
    }
}

/// Every operation supported by the command-line desktop adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Start,
    Stop,
    Cancel,
    Status,
    LaunchSettings,
    Preview,
    ClosePreview,
    SettingsGet,
    SettingsReset,
    SettingsSet {
        key: String,
        value: String,
    },
    HistoryList {
        offset: usize,
        limit: usize,
        query: String,
    },
    HistoryDelete {
        id: i64,
    },
    HistoryClear,
    HistoryHasEntries,
    CopyLastTranscript,
    Version,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub exit_code: u8,
    pub stdout: Option<String>,
}

impl CommandOutput {
    fn exit(exit_code: u8) -> Self {
        Self {
            exit_code,
            stdout: None,
        }
    }
    fn text(stdout: String) -> Self {
        Self {
            exit_code: 0,
            stdout: Some(stdout),
        }
    }
}

/// Authoritative product behavior behind the thin CLI adapter.
pub struct Application {
    paths: Paths,
}

impl Application {
    pub fn from_environment() -> io::Result<Self> {
        install_signal_handlers()?;
        Ok(Self {
            paths: Paths::from_environment(),
        })
    }

    pub fn execute(&self, command: Command) -> io::Result<CommandOutput> {
        let output = match command {
            Command::Toggle => CommandOutput::exit(dictation_session::toggle(&self.paths)?),
            Command::Start => CommandOutput::exit(dictation_session::start(&self.paths)?),
            Command::Stop => CommandOutput::exit(dictation_session::stop(&self.paths)?),
            Command::Cancel => {
                dictation_session::cancel(&self.paths)?;
                CommandOutput::exit(0)
            }
            Command::Status => CommandOutput::text(dictation_session::status(&self.paths)?),
            Command::LaunchSettings => CommandOutput::exit(product_package::launch_settings()?),
            Command::Preview => CommandOutput::exit(dictation_session::preview(&self.paths)?),
            Command::ClosePreview => {
                CommandOutput::exit(dictation_session::close_preview(&self.paths)?)
            }
            Command::SettingsGet => CommandOutput::text(settings::json()?),
            Command::SettingsReset => {
                settings::reset()?;
                CommandOutput::text(settings::json()?)
            }
            Command::SettingsSet { key, value } => {
                settings::set(&key, &value)?;
                CommandOutput::text(settings::json()?)
            }
            Command::HistoryList {
                offset,
                limit,
                query,
            } => CommandOutput::text(transcript_history::list_json(offset, limit, &query)?),
            Command::HistoryDelete { id } => {
                transcript_history::delete(id)?;
                CommandOutput::exit(0)
            }
            Command::HistoryClear => {
                transcript_history::clear()?;
                CommandOutput::exit(0)
            }
            Command::HistoryHasEntries => {
                CommandOutput::exit(if transcript_history::last()?.is_some() {
                    0
                } else {
                    1
                })
            }
            Command::CopyLastTranscript => {
                let Some(entry) = transcript_history::last()? else {
                    return Ok(CommandOutput::exit(1));
                };
                copy_to_clipboard(&entry.text)?;
                CommandOutput::exit(0)
            }
            Command::Version => CommandOutput::text(env!("CARGO_PKG_VERSION").to_owned()),
        };
        Ok(output)
    }
}

mod dictation_session {
    use super::*;

    pub(crate) fn toggle(paths: &Paths) -> io::Result<u8> {
        if let Some(recorder_pid) = read_pid(&paths.recorder_pid).filter(|pid| process_exists(*pid))
        {
            let codex_asr = product_package::command("codex-asr").ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "codex-asr was not found in PATH or ~/.cargo/bin",
                )
            })?;
            stop_recording(paths, recorder_pid, &codex_asr)
        } else if cancel(paths)? {
            Ok(0)
        } else {
            start(paths)
        }
    }

    pub(crate) fn start(paths: &Paths) -> io::Result<u8> {
        if !settings::load()?.enabled {
            return Err(invalid_input(
                "dictation is paused; enable it in Settings first",
            ));
        }
        start_recording(paths)
    }

    pub(crate) fn stop(paths: &Paths) -> io::Result<u8> {
        let recorder_pid = read_pid(&paths.recorder_pid)
            .filter(|pid| process_exists(*pid))
            .ok_or_else(|| invalid_input("dictation is not recording"))?;
        let codex_asr = product_package::command("codex-asr").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "codex-asr was not found in PATH or ~/.cargo/bin",
            )
        })?;
        stop_recording(paths, recorder_pid, &codex_asr)
    }

    pub(crate) fn cancel(paths: &Paths) -> io::Result<bool> {
        let mut cancelled = false;
        if let Some(recorder) = read_pid(&paths.recorder_pid).filter(|pid| process_exists(*pid)) {
            File::create(&paths.cancel)?;
            signal(recorder, libc::SIGINT);
            remove(&paths.recorder_pid);
            remove(&paths.wav);
            cancelled = true;
        } else {
            remove(&paths.recorder_pid);
            remove(&paths.wav);
        }
        if let Some(owner) = read_pid(&paths.session_owner_pid) {
            if process_exists(owner) {
                File::create(&paths.cancel)?;
                kill_from_file(&paths.transcriber_pid, libc::SIGTERM);
                kill_from_file(&paths.typing_pid, libc::SIGTERM);
                signal(owner, libc::SIGTERM);
                cancelled = true;
            } else {
                remove(&paths.session_owner_pid);
                remove(&paths.transcriber_pid);
                remove(&paths.typing_pid);
            }
        }
        // Idempotently repair protocol state even when a previous controller
        // already stopped the worker process.
        kill_from_file(&paths.overlay_pid, libc::SIGTERM);
        remove(&paths.overlay_pid);
        runtime_state::clear(paths);
        Ok(cancelled)
    }

    fn stop_recording(paths: &Paths, recorder_pid: i32, codex_asr: &Path) -> io::Result<u8> {
        let owner_pid = std::process::id();
        write_pid_atomic(&paths.session_owner_pid, owner_pid)?;
        let _cleanup = SessionCleanup { paths, owner_pid };

        signal(recorder_pid, libc::SIGINT);
        for _ in 0..20 {
            if !process_exists(recorder_pid) {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        remove(&paths.recorder_pid);
        if fs::metadata(&paths.wav).map_or(true, |metadata| metadata.len() == 0) {
            return Ok(1);
        }

        runtime_state::publish(paths, runtime_state::State::Transcribing, owner_pid)?;
        kill_from_file(&paths.overlay_pid, libc::SIGUSR1);
        if paths.cancel.exists() || INTERRUPTED.load(Ordering::SeqCst) {
            return Ok(0);
        }

        remove(&paths.transcript);
        remove(&paths.transcriber_pid);
        let transcript = File::create(&paths.transcript)?;
        let settings = settings::load()?;
        let language = settings.effective_language();
        let mut command = ProcessCommand::new(codex_asr);
        command.arg(&paths.wav);
        command.args(settings::transcriber_language_args(&language));
        let mut transcriber = command
            .stdout(Stdio::from(transcript))
            .stderr(Stdio::inherit())
            .spawn()?;
        publish_child(&mut transcriber, &paths.transcriber_pid)?;
        if paths.cancel.exists() {
            let _ = transcriber.kill();
        }
        let transcriber_status = wait_for_child(&mut transcriber)?;
        remove(&paths.transcriber_pid);
        remove(&paths.wav);

        if paths.cancel.exists() || INTERRUPTED.load(Ordering::SeqCst) {
            return Ok(0);
        }
        if !transcriber_status.success() {
            return Err(io::Error::other(format!(
                "codex-asr exited with {transcriber_status}"
            )));
        }
        let mut text = String::new();
        File::open(&paths.transcript)?.read_to_string(&mut text)?;
        trim_trailing_newlines(&mut text);
        remove(&paths.transcript);
        close_overlay_before_paste(paths);
        if text.is_empty() || text == "..." {
            return Ok(1);
        }

        transcript_history::add(&text)?;
        copy_to_clipboard(&text)?;
        if let Some(ydotool) = product_package::command("ydotool") {
            runtime_state::publish(paths, runtime_state::State::Typing, owner_pid)?;
            if !paths.cancel.exists() && !INTERRUPTED.load(Ordering::SeqCst) {
                // Shift+Insert is understood as paste by both common graphical
                // text controls and terminal emulators on Linux.
                let mut pasting = ProcessCommand::new(ydotool)
                    .args(["key", "42:1", "110:1", "110:0", "42:0"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()?;
                publish_child(&mut pasting, &paths.typing_pid)?;
                let status = wait_for_child(&mut pasting)?;
                remove(&paths.typing_pid);
                if !status.success() {
                    return Err(io::Error::other(format!(
                        "clipboard paste command exited with {status}"
                    )));
                }
            }
        }
        println!("{text}");
        Ok(0)
    }

    fn start_recording(paths: &Paths) -> io::Result<u8> {
        runtime_state::cleanup_stale(paths);
        for path in [
            &paths.wav,
            &paths.recorder_pid,
            &paths.transcriber_pid,
            &paths.transcript,
            &paths.cancel,
            &paths.typing_pid,
            &paths.session_owner_pid,
        ] {
            remove(path);
        }
        kill_from_file(&paths.overlay_pid, libc::SIGTERM);
        remove(&paths.overlay_pid);
        let arecord = product_package::command("arecord").ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "arecord was not found in PATH")
        })?;
        let mut recorder = ProcessCommand::new(arecord)
            .args(["-q", "-f", "S16_LE", "-r", "48000", "-c", "1"])
            .arg(&paths.wav)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        write_pid_atomic(&paths.recorder_pid, recorder.id())?;
        thread::sleep(Duration::from_millis(100));
        if let Some(status) = recorder.try_wait()? {
            let mut detail = String::new();
            if let Some(mut stderr) = recorder.stderr.take() {
                let _ = stderr.read_to_string(&mut detail);
            }
            remove(&paths.recorder_pid);
            remove(&paths.wav);
            return Err(io::Error::other(format!(
                "audio recorder exited during startup ({status}){}",
                if detail.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", detail.trim())
                }
            )));
        }
        runtime_state::publish(paths, runtime_state::State::Recording, recorder.id())?;

        if let Some(overlay) = product_package::overlay_script() {
            let control_command = env::current_exe()?;
            let mut command = ProcessCommand::new("python3");
            command
                .arg(overlay)
                .arg("--audio-file")
                .arg(&paths.wav)
                .arg("--control-command")
                .arg(control_command)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if let Some(backend) = product_package::overlay_backend() {
                command.env("GDK_BACKEND", backend);
            }
            match command.spawn() {
                Ok(child) => write_pid_atomic(&paths.overlay_pid, child.id())?,
                Err(error) => eprintln!("codex-voice: could not start GTK overlay: {error}"),
            }
        }
        Ok(0)
    }

    pub(crate) fn status(paths: &Paths) -> io::Result<String> {
        let state = runtime_state::read(paths).unwrap_or_else(|| "idle".into());
        let extension_active = settings::extension_is_active().unwrap_or(false);
        let (ubuntu, gnome_shell) = platform_info();
        Ok(format!("{{\"schemaVersion\":1,\"state\":\"{state}\",\"extensionActive\":{extension_active},\"ubuntu\":{},\"gnomeShell\":{}}}", json_string(&ubuntu), json_string(&gnome_shell)))
    }

    pub(crate) fn preview(paths: &Paths) -> io::Result<u8> {
        let overlay = product_package::overlay_script().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "codex-voice overlay is not installed",
            )
        })?;
        // The manual preview has a dedicated PID, so it cannot interfere with
        // the live dictation overlay and can be closed explicitly from Settings.
        kill_from_file(&paths.preview_overlay_pid, libc::SIGTERM);
        remove(&paths.preview_overlay_pid);

        let mut command = ProcessCommand::new("python3");
        command
            .arg(overlay)
            .arg("--control-command")
            .arg(env::current_exe()?)
            .arg("--preview")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(backend) = product_package::overlay_backend() {
            command.env("GDK_BACKEND", backend);
        }
        let mut child = command.spawn()?;
        write_pid_atomic(&paths.preview_overlay_pid, child.id())?;
        let status = child.wait()?;
        remove(&paths.preview_overlay_pid);
        Ok(status.code().unwrap_or(0) as u8)
    }

    pub(crate) fn close_preview(paths: &Paths) -> io::Result<u8> {
        kill_from_file(&paths.preview_overlay_pid, libc::SIGTERM);
        remove(&paths.preview_overlay_pid);
        Ok(0)
    }
}

fn platform_info() -> (String, String) {
    let ubuntu = fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                line.strip_prefix("VERSION_ID=")
                    .map(|v| v.trim_matches('\"').into())
            })
        })
        .unwrap_or_else(|| "unknown".into());
    let gnome_shell = ProcessCommand::new("gnome-shell")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|out| out.split_whitespace().last().map(Into::into))
        .unwrap_or_else(|| "unknown".into());
    (ubuntu, gnome_shell)
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"unknown\"".into())
}

fn publish_child(child: &mut Child, pid_file: &Path) -> io::Result<()> {
    ACTIVE_CHILD.store(child.id() as i32, Ordering::SeqCst);
    write_pid_atomic(pid_file, child.id())
}
fn wait_for_child(child: &mut Child) -> io::Result<ExitStatus> {
    loop {
        match child.wait() {
            Ok(status) => {
                ACTIVE_CHILD.store(0, Ordering::SeqCst);
                return Ok(status);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                ACTIVE_CHILD.store(0, Ordering::SeqCst);
                return Err(error);
            }
        }
    }
}
fn copy_to_clipboard(text: &str) -> io::Result<()> {
    let mut last_error = None;
    if let Some(wl_copy) = product_package::command("wl-copy") {
        match write_to_clipboard_command(ProcessCommand::new(wl_copy), text) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
    }
    if let Some(xclip) = product_package::command("xclip") {
        let mut command = ProcessCommand::new(xclip);
        command.args(["-selection", "clipboard", "-in"]);
        match write_to_clipboard_command(command, text) {
            Ok(()) => return Ok(()),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "neither wl-copy nor xclip was found",
        )
    }))
}
fn write_to_clipboard_command(mut command: ProcessCommand, text: &str) -> io::Result<()> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "clipboard command exited with {status}"
        )));
    }
    Ok(())
}
fn read_pid(path: &Path) -> Option<i32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}
fn write_pid_atomic(path: &Path, pid: u32) -> io::Result<()> {
    write_atomic(path, format!("{pid}\n").as_bytes())
}
fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temporary, path)
}
fn process_exists(pid: i32) -> bool {
    pid > 0 && unsafe { libc::kill(pid, 0) == 0 }
}
fn signal(pid: i32, signal: libc::c_int) {
    if pid > 0 {
        unsafe { libc::kill(pid, signal) };
    }
}
fn kill_from_file(path: &Path, signal_number: libc::c_int) {
    if let Some(pid) = read_pid(path).filter(|pid| process_exists(*pid)) {
        signal(pid, signal_number);
    }
}
fn close_overlay_before_paste(paths: &Paths) {
    if let Some(pid) = read_pid(&paths.overlay_pid).filter(|pid| process_exists(*pid)) {
        signal(pid, libc::SIGTERM);
        let deadline = Instant::now() + Duration::from_millis(500);
        while process_exists(pid) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        // GNOME restores focus only after processing the destroyed window.
        thread::sleep(Duration::from_millis(50));
    }
    remove(&paths.overlay_pid);
}
fn remove(path: &Path) {
    let _ = fs::remove_file(path);
}
fn trim_trailing_newlines(text: &mut String) {
    while text.ends_with(['\n', '\r']) {
        text.pop();
    }
}
fn install_signal_handlers() -> io::Result<()> {
    unsafe {
        if libc::signal(libc::SIGINT, handle_signal as *const () as usize) == libc::SIG_ERR {
            return Err(io::Error::last_os_error());
        }
        if libc::signal(libc::SIGTERM, handle_signal as *const () as usize) == libc::SIG_ERR {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}
fn invalid_input(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}
