use serde::Serialize;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SCHEMA: &str = "io.github.andy_spike.CodexVoice";
const EXTENSION_UUID: &str = "codex-voice@andy-spike.github.io";
const DEFAULT_KEYBINDING: &str = "<Control><Super>space";
const DEFAULT_BACKGROUND: &str = "#0e1110eb";
const DEFAULT_ACCENT: &str = "#32d870";

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
            transcriber_pid: path("codex-voice-transcriber.pid"),
            transcript: path("codex-voice-transcript.txt"),
            cancel: path("codex-voice-cancelled"),
            typing_pid: path("codex-voice-typing.pid"),
            session_owner_pid: path("codex-voice-session-owner.pid"),
            runtime_state: path("codex-voice-state.json"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum RuntimeState {
    Recording,
    Transcribing,
    Typing,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimeStateDocument {
    schema_version: u8,
    state: RuntimeState,
    owner_pid: u32,
    started_at: u128,
}

#[derive(Debug, Clone)]
struct Settings {
    enabled: bool,
    keybinding: String,
    pill_background_color: String,
    pill_accent_color: String,
    language: String,
    language_override: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsDocument<'a> {
    schema_version: u8,
    enabled: bool,
    keybinding: &'a str,
    pill_background_color: &'a str,
    pill_accent_color: &'a str,
    language: &'a str,
    overrides: Overrides<'a>,
}

#[derive(Serialize)]
struct Overrides<'a> {
    language: Option<&'a str>,
}

impl Settings {
    fn document(&self) -> SettingsDocument<'_> {
        SettingsDocument {
            schema_version: 1,
            enabled: self.enabled,
            keybinding: &self.keybinding,
            pill_background_color: &self.pill_background_color,
            pill_accent_color: &self.pill_accent_color,
            language: &self.language,
            overrides: Overrides {
                language: self.language_override.as_deref(),
            },
        }
    }

    fn effective_language(&self) -> String {
        self.language_override
            .clone()
            .unwrap_or_else(|| self.language.clone())
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
        clear_runtime_state(&self.paths);
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

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("codex-voice: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<u8> {
    let mut args = env::args().skip(1);
    let first = args.next();
    let rest: Vec<String> = args.collect();
    if first.as_deref() == Some("--version") {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return Ok(0);
    }
    if first.as_deref() == Some("settings") {
        return run_settings(&rest);
    }

    let paths = Paths::from_environment();
    install_signal_handlers()?;
    match first.as_deref() {
        Some("--status") => {
            print_status(&paths)?;
            Ok(0)
        }
        Some("--settings") => launch_settings(),
        Some("--cancel") => {
            cancel_active_session(&paths)?;
            Ok(0)
        }
        Some("--start") => start_if_allowed(&paths),
        Some("--stop") => stop_if_recording(&paths),
        Some("--toggle") | None => toggle(&paths),
        Some(flag) => Err(invalid_input(format!("unknown argument `{flag}`"))),
    }
}

fn toggle(paths: &Paths) -> io::Result<u8> {
    if cancel_active_session(paths)? {
        return Ok(0);
    }
    if let Some(recorder_pid) = read_pid(&paths.recorder_pid).filter(|pid| process_exists(*pid)) {
        let codex_asr = find_command("codex-asr").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "codex-asr was not found in PATH or ~/.cargo/bin",
            )
        })?;
        stop_recording(paths, recorder_pid, &codex_asr)
    } else {
        start_if_allowed(paths)
    }
}

fn start_if_allowed(paths: &Paths) -> io::Result<u8> {
    if !load_settings()?.enabled {
        return Err(invalid_input(
            "dictation is paused; enable it in Settings first",
        ));
    }
    start_recording(paths)
}

fn stop_if_recording(paths: &Paths) -> io::Result<u8> {
    let recorder_pid = read_pid(&paths.recorder_pid)
        .filter(|pid| process_exists(*pid))
        .ok_or_else(|| invalid_input("dictation is not recording"))?;
    let codex_asr = find_command("codex-asr").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "codex-asr was not found in PATH or ~/.cargo/bin",
        )
    })?;
    stop_recording(paths, recorder_pid, &codex_asr)
}

fn cancel_active_session(paths: &Paths) -> io::Result<bool> {
    let mut cancelled = false;
    if let Some(recorder) = read_pid(&paths.recorder_pid).filter(|pid| process_exists(*pid)) {
        File::create(&paths.cancel)?;
        signal(recorder, libc::SIGINT);
        remove(&paths.recorder_pid);
        remove(&paths.wav);
        cancelled = true;
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
    if cancelled {
        kill_from_file(&paths.overlay_pid, libc::SIGTERM);
        remove(&paths.overlay_pid);
        clear_runtime_state(paths);
    }
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

    publish_runtime_state(paths, RuntimeState::Transcribing, owner_pid)?;
    kill_from_file(&paths.overlay_pid, libc::SIGUSR1);
    if paths.cancel.exists() || INTERRUPTED.load(Ordering::SeqCst) {
        return Ok(0);
    }

    remove(&paths.transcript);
    remove(&paths.transcriber_pid);
    let transcript = File::create(&paths.transcript)?;
    let settings = load_settings()?;
    let language = settings.effective_language();
    let mut command = Command::new(codex_asr);
    command.arg(&paths.wav);
    command.args(transcriber_language_args(&language));
    let mut transcriber = command
        .stdout(Stdio::from(transcript))
        .stderr(Stdio::null())
        .spawn()?;
    publish_child(&mut transcriber, &paths.transcriber_pid)?;
    if paths.cancel.exists() {
        let _ = transcriber.kill();
    }
    wait_for_child(&mut transcriber);
    remove(&paths.transcriber_pid);
    remove(&paths.wav);

    if paths.cancel.exists() || INTERRUPTED.load(Ordering::SeqCst) {
        return Ok(0);
    }
    let mut text = String::new();
    File::open(&paths.transcript)?.read_to_string(&mut text)?;
    trim_trailing_newlines(&mut text);
    remove(&paths.transcript);
    kill_from_file(&paths.overlay_pid, libc::SIGTERM);
    remove(&paths.overlay_pid);
    if text.is_empty() || text == "..." {
        return Ok(1);
    }

    copy_to_clipboard(&text);
    if let Some(ydotool) = find_command("ydotool") {
        publish_runtime_state(paths, RuntimeState::Typing, owner_pid)?;
        thread::sleep(Duration::from_millis(200));
        if !paths.cancel.exists() && !INTERRUPTED.load(Ordering::SeqCst) {
            let mut typing = Command::new(ydotool)
                .args(["type", "--key-delay", "12", "--"])
                .arg(&text)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            publish_child(&mut typing, &paths.typing_pid)?;
            wait_for_child(&mut typing);
            remove(&paths.typing_pid);
        }
    }
    println!("{text}");
    Ok(0)
}

fn start_recording(paths: &Paths) -> io::Result<u8> {
    cleanup_stale_runtime_state(paths);
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
    let arecord = find_command("arecord")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "arecord was not found in PATH"))?;
    let recorder = Command::new(arecord)
        .args(["-q", "-f", "S16_LE", "-r", "48000", "-c", "1"])
        .arg(&paths.wav)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    write_pid_atomic(&paths.recorder_pid, recorder.id())?;
    publish_runtime_state(paths, RuntimeState::Recording, recorder.id())?;

    if !extension_is_active().unwrap_or(false) {
        if let Some(overlay) = find_overlay_script() {
            let settings = load_settings()?;
            let mut command = Command::new("python3");
            command
                .arg(overlay)
                .arg("--audio-file")
                .arg(&paths.wav)
                .arg("--recorder-pid-file")
                .arg(&paths.recorder_pid)
                .arg("--overlay-pid-file")
                .arg(&paths.overlay_pid)
                .arg("--transcriber-pid-file")
                .arg(&paths.transcriber_pid)
                .arg("--cancel-file")
                .arg(&paths.cancel)
                .arg("--background-color")
                .arg(settings.pill_background_color)
                .arg("--accent-color")
                .arg(settings.pill_accent_color)
                .arg("--state-file")
                .arg(&paths.runtime_state)
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if let Some(backend) = overlay_backend() {
                command.env("GDK_BACKEND", backend);
            }
            match command.spawn() {
                Ok(child) => write_pid_atomic(&paths.overlay_pid, child.id())?,
                Err(error) => eprintln!("codex-voice: could not start GTK fallback: {error}"),
            }
        }
    }
    Ok(0)
}

fn print_status(paths: &Paths) -> io::Result<()> {
    let state = read_runtime_state(paths).unwrap_or_else(|| "idle".into());
    let extension_active = extension_is_active().unwrap_or(false);
    let (ubuntu, gnome_shell) = platform_info();
    println!("{{\"schemaVersion\":1,\"state\":\"{state}\",\"extensionActive\":{extension_active},\"ubuntu\":{},\"gnomeShell\":{}}}", json_string(&ubuntu), json_string(&gnome_shell));
    Ok(())
}

fn launch_settings() -> io::Result<u8> {
    let binary = env::var_os("CODEX_VOICE_SETTINGS_BIN")
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|home| home.join(".local/bin/codex-voice-settings")));
    let Some(binary) = binary.filter(|path| path.is_file()) else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "codex-voice-settings is not installed",
        ));
    };
    Command::new(binary).spawn()?;
    Ok(0)
}

fn run_settings(args: &[String]) -> io::Result<u8> {
    match args {
        [command] if command == "get" => print_settings(),
        [command] if command == "reset" => {
            reset_settings()?;
            print_settings()
        }
        [command, key, value] if command == "set" => {
            set_setting(key, value)?;
            print_settings()
        }
        _ => Err(invalid_input(
            "usage: codex-voice settings get|reset|set <key> <value>",
        )),
    }
}

fn print_settings() -> io::Result<u8> {
    let json =
        serde_json::to_string(&load_settings()?.document()).expect("settings JSON is serializable");
    println!("{json}");
    Ok(0)
}

fn settings_schema_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".local/share/codex-voice/schemas"))
}

fn gsettings_command() -> Command {
    let mut command = Command::new("gsettings");
    if let Some(dir) = settings_schema_dir().filter(|dir| dir.is_dir()) {
        let old = env::var_os("GSETTINGS_SCHEMA_DIR");
        let value = old
            .map(|old| format!("{}:{}", dir.display(), PathBuf::from(old).display()))
            .unwrap_or_else(|| dir.display().to_string());
        command.env("GSETTINGS_SCHEMA_DIR", value);
    }
    command
}

fn gsettings(args: &[&str]) -> io::Result<String> {
    let output = gsettings_command().args(args).output()?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned());
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(io::Error::new(
        io::ErrorKind::Other,
        if message.is_empty() {
            "GSettings operation failed".into()
        } else {
            message
        },
    ))
}

fn load_settings() -> io::Result<Settings> {
    let enabled = gsettings(&["get", SCHEMA, "enabled"])
        .map(|v| v == "true")
        .unwrap_or(true);
    let keybinding = gsettings(&["get", SCHEMA, "keybinding"])
        .ok()
        .and_then(|v| parse_gvariant_string_array(&v).into_iter().next())
        .unwrap_or_else(|| DEFAULT_KEYBINDING.into());
    let background = gsettings(&["get", SCHEMA, "pill-background-color"])
        .ok()
        .and_then(|v| parse_gvariant_string(&v))
        .and_then(|v| normalize_color(&v))
        .unwrap_or_else(|| DEFAULT_BACKGROUND.into());
    let accent = gsettings(&["get", SCHEMA, "pill-accent-color"])
        .ok()
        .and_then(|v| parse_gvariant_string(&v))
        .and_then(|v| normalize_color(&v))
        .unwrap_or_else(|| DEFAULT_ACCENT.into());
    let language = gsettings(&["get", SCHEMA, "language"])
        .ok()
        .and_then(|v| parse_gvariant_string(&v))
        .and_then(|v| normalize_language(&v))
        .unwrap_or_else(|| "auto".into());
    // Presence of this variable overrides the stored setting, including an
    // empty value or `auto`, both of which deliberately select detection.
    let language_override = env::var("CODEX_VOICE_LANG")
        .ok()
        .and_then(|v| normalize_language(&v));
    Ok(Settings {
        enabled,
        keybinding,
        pill_background_color: background,
        pill_accent_color: accent,
        language,
        language_override,
    })
}

fn set_setting(key: &str, value: &str) -> io::Result<()> {
    match key {
        "enabled" => {
            if value != "true" && value != "false" {
                return Err(invalid_input("enabled must be true or false"));
            }
            gsettings(&["set", SCHEMA, key, value])?;
        }
        "keybinding" => {
            let accelerator = normalize_accelerator(value)
                .ok_or_else(|| invalid_input("invalid GNOME accelerator"))?;
            let escaped = accelerator.replace('\\', "\\\\").replace('\'', "\\'");
            gsettings(&["set", SCHEMA, key, &format!("['{escaped}']")])?;
        }
        "pill-background-color" | "pill-accent-color" => {
            let color = normalize_color(value)
                .ok_or_else(|| invalid_input("color must be #rgb, #rgba, #rrggbb, or #rrggbbaa"))?;
            gsettings(&["set", SCHEMA, key, &color])?;
        }
        "language" => {
            let language = normalize_language(value)
                .ok_or_else(|| invalid_input("language must be auto or a BCP-47-like code"))?;
            gsettings(&["set", SCHEMA, key, &language])?;
        }
        _ => return Err(invalid_input("unknown settings key")),
    }
    Ok(())
}

fn reset_settings() -> io::Result<u8> {
    gsettings(&["reset-recursively", SCHEMA]).map(|_| 0)
}

fn parse_gvariant_string(value: &str) -> Option<String> {
    value
        .trim()
        .strip_prefix('\'')
        .and_then(|v| v.strip_suffix('\''))
        .map(|v| v.replace("\\'", "'").replace("\\\\", "\\"))
}

fn parse_gvariant_string_array(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .unwrap_or("");
    inner.split(',').filter_map(parse_gvariant_string).collect()
}

fn normalize_color(value: &str) -> Option<String> {
    let hex = value.trim().strip_prefix('#')?;
    let expanded = match hex.len() {
        3 => {
            hex.chars()
                .flat_map(|character| [character, character])
                .collect::<String>()
                + "ff"
        }
        4 => hex
            .chars()
            .flat_map(|character| [character, character])
            .collect(),
        6 => format!("{hex}ff"),
        8 => hex.into(),
        _ => return None,
    };
    expanded
        .chars()
        .all(|c| c.is_ascii_hexdigit())
        .then(|| format!("#{}", expanded.to_ascii_lowercase()))
}

fn normalize_language(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.is_empty() || value == "auto" {
        return Some("auto".into());
    }
    let valid = value.len() <= 35
        && value.split('-').all(|part| {
            !part.is_empty() && part.len() <= 8 && part.chars().all(|c| c.is_ascii_alphanumeric())
        });
    valid.then_some(value)
}

fn transcriber_language_args(language: &str) -> Vec<String> {
    (language != "auto")
        .then(|| vec!["--language".into(), language.into()])
        .unwrap_or_default()
}

fn normalize_accelerator(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let mut rest = value;
    let mut modifiers = String::new();
    for (needle, canonical) in [
        ("<Primary>", "<Control>"),
        ("<Ctrl>", "<Control>"),
        ("<Control>", "<Control>"),
        ("<Alt>", "<Alt>"),
        ("<Super>", "<Super>"),
        ("<Shift>", "<Shift>"),
    ] {
        while let Some(after) = rest.strip_prefix(needle) {
            if !modifiers.contains(canonical) {
                modifiers.push_str(canonical);
            }
            rest = after;
        }
    }
    let key = rest.trim();
    let supported_function = key.len() >= 2
        && key.starts_with('F')
        && key[1..]
            .parse::<u8>()
            .map_or(false, |n| (1..=35).contains(&n));
    let normal_key =
        key.chars().count() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ');
    ((modifiers.contains("<Control>")
        || modifiers.contains("<Alt>")
        || modifiers.contains("<Super>")
        || supported_function)
        && (normal_key || supported_function || key == "space"))
        .then(|| format!("{modifiers}{}", if key == " " { "space" } else { key }))
}

fn publish_runtime_state(paths: &Paths, state: RuntimeState, owner_pid: u32) -> io::Result<()> {
    let document = RuntimeStateDocument {
        schema_version: 1,
        state,
        owner_pid,
        started_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    };
    let json = serde_json::to_vec(&document).expect("runtime state is serializable");
    write_atomic(&paths.runtime_state, &json)
}

fn read_runtime_state(paths: &Paths) -> Option<String> {
    let result = (|| {
        let contents = fs::read_to_string(&paths.runtime_state).ok()?;
        let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
        let schema = value.get("schemaVersion")?.as_u64()?;
        let state = value.get("state")?.as_str()?;
        let owner = value.get("ownerPid")?.as_i64()?;
        (schema == 1
            && matches!(state, "recording" | "transcribing" | "typing")
            && process_exists(owner as i32))
        .then(|| state.to_owned())
    })();
    if result.is_none() && paths.runtime_state.exists() {
        clear_runtime_state(paths);
    }
    result
}

fn cleanup_stale_runtime_state(paths: &Paths) {
    let _ = read_runtime_state(paths);
}
fn clear_runtime_state(paths: &Paths) {
    remove(&paths.runtime_state);
}

fn extension_is_active() -> io::Result<bool> {
    let output = Command::new("gnome-extensions")
        .args(["info", EXTENSION_UUID])
        .output()?;
    if !output.status.success() {
        return Ok(false);
    }
    Ok(String::from_utf8_lossy(&output.stdout).lines().any(|line| {
        line.trim().starts_with("State:") && line.to_ascii_lowercase().contains("active")
    }))
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
    let gnome_shell = Command::new("gnome-shell")
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
fn wait_for_child(child: &mut Child) {
    loop {
        match child.wait() {
            Ok(_) => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    ACTIVE_CHILD.store(0, Ordering::SeqCst);
}
fn copy_to_clipboard(text: &str) {
    let Some(wl_copy) = find_command("wl-copy") else {
        return;
    };
    let Ok(mut child) = Command::new(wl_copy)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    let _ = child.wait();
}
fn find_overlay_script() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.extend([
                dir.join("../src/overlay.py"),
                dir.join("../share/codex-voice/overlay.py"),
                dir.join("../../../src/overlay.py"),
            ]);
        }
    }
    if let Ok(cwd) = env::current_dir() {
        candidates.push(cwd.join("src/overlay.py"));
    }
    if let Some(home) = home_dir() {
        candidates.push(home.join(".local/share/codex-voice/overlay.py"));
    }
    candidates.into_iter().find(|path| path.is_file())
}
fn overlay_backend() -> Option<String> {
    env::var("CODEX_VOICE_GDK_BACKEND")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            env::var_os("DISPLAY")
                .filter(|value| !value.is_empty())
                .map(|_| "x11".into())
        })
}
fn find_command(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let path = PathBuf::from(name);
        return path.is_file().then_some(path);
    }
    if let Some(paths) = env::var_os("PATH") {
        if let Some(found) = env::split_paths(&paths)
            .map(|dir| dir.join(name))
            .find(|path| path.is_file())
        {
            return Some(found);
        }
    }
    (name == "codex-asr")
        .then(home_dir)?
        .map(|home| home.join(".cargo/bin/codex-asr"))
        .filter(|path| path.is_file())
}
fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_colors() {
        assert_eq!(normalize_color("#ABC"), Some("#aabbccff".into()));
        assert_eq!(normalize_color("#11223344"), Some("#11223344".into()));
        assert_eq!(normalize_color("blue"), None);
    }
    #[test]
    fn normalizes_languages_and_legacy_empty_values() {
        assert_eq!(normalize_language(""), Some("auto".into()));
        assert_eq!(normalize_language("EN-US"), Some("en-us".into()));
        assert_eq!(normalize_language("en_US"), None);
    }
    #[test]
    fn validates_accelerators() {
        assert_eq!(
            normalize_accelerator("<Ctrl><Super>space"),
            Some("<Control><Super>space".into())
        );
        assert_eq!(normalize_accelerator("a"), None);
        assert_eq!(normalize_accelerator("F12"), Some("F12".into()));
    }
    #[test]
    fn auto_does_not_add_asr_language_argument() {
        let settings = Settings {
            enabled: true,
            keybinding: DEFAULT_KEYBINDING.into(),
            pill_background_color: DEFAULT_BACKGROUND.into(),
            pill_accent_color: DEFAULT_ACCENT.into(),
            language: "auto".into(),
            language_override: None,
        };
        assert_eq!(settings.effective_language(), "auto");
        assert!(transcriber_language_args(&settings.effective_language()).is_empty());
    }
    #[test]
    fn explicit_language_adds_asr_language_argument() {
        assert_eq!(transcriber_language_args("es"), vec!["--language", "es"]);
    }
    #[test]
    fn runtime_state_json_has_protocol_fields() {
        let doc = RuntimeStateDocument {
            schema_version: 1,
            state: RuntimeState::Recording,
            owner_pid: 42,
            started_at: 1,
        };
        let value = serde_json::to_value(doc).unwrap();
        assert_eq!(value["state"], "recording");
        assert_eq!(value["schemaVersion"], 1);
    }
}
