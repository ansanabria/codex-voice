mod paste_automation;
mod process_identity;
mod product_package;
pub mod protocol;
mod runtime_state;
mod settings;
mod transcript_history;

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command as ProcessCommand, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use process_identity::ProcessIdentity;

const EXTENSION_UUID: &str = "codex-voice@andy-spike.github.io";

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static ACTIVE_PIDFD: AtomicI32 = AtomicI32::new(-1);

extern "C" fn handle_signal(_: libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
    let pidfd = ACTIVE_PIDFD.load(Ordering::SeqCst);
    if pidfd >= 0 {
        unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                pidfd,
                libc::SIGTERM,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
    }
}

#[derive(Clone)]
struct Paths {
    lock: PathBuf,
    recorder_pid: PathBuf,
    wav: PathBuf,
    overlay_pid: PathBuf,
    preview_overlay_pid: PathBuf,
    transcriber_pid: PathBuf,
    transcript: PathBuf,
    cancel: PathBuf,
    typing_pid: PathBuf,
    session_owner_pid: PathBuf,
    session_recorder_pid: PathBuf,
    runtime_state: PathBuf,
}

impl Paths {
    fn from_environment() -> io::Result<Self> {
        let state_dir = if let Some(runtime) = env::var_os("XDG_RUNTIME_DIR") {
            PathBuf::from(runtime)
        } else {
            env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?
                .join("codex-voice/runtime")
        };
        fs::create_dir_all(&state_dir)?;
        fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o700))?;
        Ok(Self::in_directory(state_dir))
    }

    fn in_directory(state_dir: PathBuf) -> Self {
        let path = |name: &str| state_dir.join(name);
        Self {
            lock: path("codex-voice.lock"),
            recorder_pid: path("codex-voice.pid"),
            wav: path("codex-voice.wav"),
            overlay_pid: path("codex-voice-overlay.pid"),
            preview_overlay_pid: path("codex-voice-preview-overlay.pid"),
            transcriber_pid: path("codex-voice-transcriber.pid"),
            transcript: path("codex-voice-transcript.txt"),
            cancel: path("codex-voice-cancelled"),
            typing_pid: path("codex-voice-typing.pid"),
            session_owner_pid: path("codex-voice-session-owner.pid"),
            session_recorder_pid: path("codex-voice-session-recorder.pid"),
            runtime_state: path("codex-voice-state.json"),
        }
    }
}

struct StateLock(File);

impl StateLock {
    fn acquire(paths: &Paths) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(&paths.lock)?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self(file))
    }
}

impl Drop for StateLock {
    fn drop(&mut self) {
        unsafe { libc::flock(self.0.as_raw_fd(), libc::LOCK_UN) };
    }
}

struct SessionCleanup<'a> {
    paths: &'a Paths,
    owner: ProcessIdentity,
    overlay: Option<ProcessIdentity>,
    transcriber: Option<ProcessIdentity>,
    typing: Option<ProcessIdentity>,
    recovery: Option<RecoveryPaths>,
    preserve_source_audio: bool,
    preserve_recovery: bool,
}

impl Drop for SessionCleanup<'_> {
    fn drop(&mut self) {
        clear_active_child();
        if let Some(transcriber) = self.transcriber {
            let _ = terminate_owned_record(
                self.paths,
                &self.paths.transcriber_pid,
                transcriber,
                libc::SIGTERM,
            );
        }
        if let Some(typing) = self.typing {
            let _ =
                terminate_owned_record(self.paths, &self.paths.typing_pid, typing, libc::SIGTERM);
        }
        if let Some(overlay) = self.overlay {
            let _ =
                terminate_owned_record(self.paths, &self.paths.overlay_pid, overlay, libc::SIGTERM);
        }
        if let Ok(_lock) = StateLock::acquire(self.paths) {
            let owns_session =
                process_identity::read_record(&self.paths.session_owner_pid) == Some(self.owner);
            if owns_session {
                process_identity::remove_record_if(&self.paths.session_owner_pid, self.owner);
                remove(&self.paths.session_recorder_pid);
                runtime_state::clear_if_owner(self.paths, self.owner);
                process_identity::remove_record_if(&self.paths.cancel, self.owner);
                if !self.preserve_source_audio {
                    remove(&self.paths.wav);
                }
                remove(&self.paths.transcript);
                if !self.preserve_recovery {
                    if let Some(recovery) = &self.recovery {
                        recovery.remove();
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
struct RecoveryPaths {
    audio: PathBuf,
    transcript: PathBuf,
}

impl RecoveryPaths {
    fn for_session(paths: &Paths, owner: ProcessIdentity) -> Self {
        Self {
            audio: paths.wav.with_file_name(format!(
                "codex-voice-audio-recovery-{}-{}.wav",
                owner.pid, owner.start_time
            )),
            transcript: paths.transcript.with_file_name(format!(
                "codex-voice-transcript-recovery-{}-{}.txt",
                owner.pid, owner.start_time
            )),
        }
    }

    fn remove(&self) {
        remove(&self.audio);
        remove(&self.transcript);
    }
}

/// Every operation supported by the command-line desktop adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Toggle,
    Start,
    Stop,
    Cancel,
    CancelRecording {
        recorder_pid: i32,
        recorder_start_time: u64,
    },
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
    WatchSession {
        overlay_pid: i32,
        overlay_start_time: u64,
        recorder_pid: i32,
        recorder_start_time: u64,
    },
    SuperviseOwner {
        owner_pid: i32,
        owner_start_time: u64,
    },
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
            paths: Paths::from_environment()?,
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
            Command::CancelRecording {
                recorder_pid,
                recorder_start_time,
            } => CommandOutput::exit(
                if dictation_session::cancel_if_recorder_matches(
                    &self.paths,
                    ProcessIdentity {
                        pid: recorder_pid,
                        start_time: recorder_start_time,
                    },
                )? {
                    0
                } else {
                    1
                },
            ),
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
            Command::WatchSession {
                overlay_pid,
                overlay_start_time,
                recorder_pid,
                recorder_start_time,
            } => CommandOutput::exit(dictation_session::watch_session(
                &self.paths,
                ProcessIdentity {
                    pid: overlay_pid,
                    start_time: overlay_start_time,
                },
                ProcessIdentity {
                    pid: recorder_pid,
                    start_time: recorder_start_time,
                },
            )?),
            Command::SuperviseOwner {
                owner_pid,
                owner_start_time,
            } => CommandOutput::exit(dictation_session::supervise_owner(
                &self.paths,
                ProcessIdentity {
                    pid: owner_pid,
                    start_time: owner_start_time,
                },
            )?),
            Command::Version => CommandOutput::text(env!("CARGO_PKG_VERSION").to_owned()),
        };
        Ok(output)
    }
}

mod dictation_session {
    use super::*;

    pub(crate) fn toggle(paths: &Paths) -> io::Result<u8> {
        let lock = StateLock::acquire(paths)?;
        repair_orphans_locked(paths)?;
        if let Some(recorder) = process_identity::read_active_record(&paths.recorder_pid) {
            let codex_asr = product_package::command("codex-asr").ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    "codex-asr was not found in PATH or ~/.cargo/bin",
                )
            })?;
            let context = claim_stop_locked(paths, recorder)?;
            drop(lock);
            stop_recording(paths, context, &codex_asr)
        } else if let Some(owner) = process_identity::read_active_record(&paths.session_owner_pid) {
            process_identity::write_record(&paths.cancel, owner)?;
            let targets = cancellation_targets_locked(paths, Some(owner));
            drop(lock);
            terminate_cancellation_targets(paths, targets)?;
            Ok(0)
        } else {
            if !settings::load()?.enabled {
                return Err(invalid_input(
                    "dictation is paused; enable it in Settings first",
                ));
            }
            start_recording_locked(paths)
        }
    }

    pub(crate) fn start(paths: &Paths) -> io::Result<u8> {
        if !settings::load()?.enabled {
            return Err(invalid_input(
                "dictation is paused; enable it in Settings first",
            ));
        }
        let _lock = StateLock::acquire(paths)?;
        repair_orphans_locked(paths)?;
        if process_identity::read_active_record(&paths.recorder_pid).is_some()
            || process_identity::read_active_record(&paths.session_owner_pid).is_some()
        {
            return Err(invalid_input("dictation is already active"));
        }
        start_recording_locked(paths)
    }

    pub(crate) fn stop(paths: &Paths) -> io::Result<u8> {
        let codex_asr = product_package::command("codex-asr").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "codex-asr was not found in PATH or ~/.cargo/bin",
            )
        })?;
        let lock = StateLock::acquire(paths)?;
        repair_orphans_locked(paths)?;
        let recorder = process_identity::read_active_record(&paths.recorder_pid)
            .ok_or_else(|| invalid_input("dictation is not recording"))?;
        let context = claim_stop_locked(paths, recorder)?;
        drop(lock);
        stop_recording(paths, context, &codex_asr)
    }

    pub(crate) fn cancel(paths: &Paths) -> io::Result<bool> {
        let lock = StateLock::acquire(paths)?;
        repair_orphans_locked(paths)?;
        let owner = process_identity::read_active_record(&paths.session_owner_pid);
        let recorder = process_identity::read_active_record(&paths.recorder_pid);
        let target = owner.or(recorder);
        if let Some(target) = target {
            process_identity::write_record(&paths.cancel, target)?;
        }
        let targets = cancellation_targets_locked(paths, owner);
        drop(lock);
        terminate_cancellation_targets(paths, targets)?;
        let cancelled = target.is_some();
        Ok(cancelled)
    }

    struct StopContext {
        owner: ProcessIdentity,
        recorder: ProcessIdentity,
        overlay: Option<ProcessIdentity>,
    }

    fn claim_stop_locked(paths: &Paths, recorder: ProcessIdentity) -> io::Result<StopContext> {
        if process_identity::read_active_record(&paths.session_owner_pid).is_some() {
            return Err(invalid_input("dictation is already transcribing"));
        }
        let owner = ProcessIdentity::current()?;
        spawn_owner_supervisor(owner)?;
        process_identity::write_record(&paths.session_owner_pid, owner)?;
        if let Err(error) = process_identity::write_record(&paths.session_recorder_pid, recorder) {
            process_identity::remove_record_if(&paths.session_owner_pid, owner);
            return Err(error);
        }
        if is_cancelled(paths, owner) {
            process_identity::remove_record_if(&paths.session_owner_pid, owner);
            remove(&paths.session_recorder_pid);
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "dictation cancelled",
            ));
        }
        if let Err(error) = runtime_state::publish(paths, runtime_state::State::Transcribing, owner)
        {
            process_identity::remove_record_if(&paths.session_owner_pid, owner);
            remove(&paths.session_recorder_pid);
            return Err(error);
        }
        Ok(StopContext {
            owner,
            recorder,
            overlay: process_identity::read_active_record(&paths.overlay_pid),
        })
    }

    fn stop_recording(paths: &Paths, context: StopContext, codex_asr: &Path) -> io::Result<u8> {
        stop_recording_with_options(paths, context, codex_asr, None, Duration::from_secs(600))
    }

    fn stop_recording_with_options(
        paths: &Paths,
        context: StopContext,
        codex_asr: &Path,
        language_args: Option<&[String]>,
        asr_timeout: Duration,
    ) -> io::Result<u8> {
        stop_recording_with_hooks(
            paths,
            context,
            codex_asr,
            language_args,
            asr_timeout,
            StopRecordingHooks::production(),
        )
    }

    #[derive(Clone, Copy)]
    struct StopRecordingHooks {
        metadata: fn(&Path) -> io::Result<fs::Metadata>,
        open_transcript: fn(&Path) -> io::Result<File>,
        set_audio_private: fn(&Path) -> io::Result<()>,
        rename_audio: fn(&Path, &Path) -> io::Result<()>,
    }

    impl StopRecordingHooks {
        fn production() -> Self {
            Self {
                metadata: source_audio_metadata,
                open_transcript: open_private_recovery_transcript,
                set_audio_private: make_audio_private,
                rename_audio: rename_recovery_audio,
            }
        }
    }

    fn stop_recording_with_hooks(
        paths: &Paths,
        context: StopContext,
        codex_asr: &Path,
        language_args: Option<&[String]>,
        asr_timeout: Duration,
        hooks: StopRecordingHooks,
    ) -> io::Result<u8> {
        let recovery = RecoveryPaths::for_session(paths, context.owner);
        let mut cleanup = SessionCleanup {
            paths,
            owner: context.owner,
            overlay: context.overlay,
            transcriber: None,
            typing: None,
            recovery: None,
            preserve_source_audio: true,
            preserve_recovery: false,
        };

        let recorder_terminated = match process_identity::terminate(context.recorder, libc::SIGINT)
        {
            Ok(terminated) => terminated,
            Err(error) => {
                return source_audio_failure(
                    paths,
                    context.owner,
                    &mut cleanup,
                    &recovery,
                    "could not stop audio recorder",
                    error,
                );
            }
        };
        if !recorder_terminated {
            let lock = match StateLock::acquire(paths) {
                Ok(lock) => lock,
                Err(error) => {
                    return source_audio_failure(
                        paths,
                        context.owner,
                        &mut cleanup,
                        &recovery,
                        "could not lock session after audio recorder termination failure",
                        error,
                    );
                }
            };
            if let Err(error) =
                runtime_state::publish(paths, runtime_state::State::Recording, context.recorder)
            {
                drop(lock);
                return source_audio_failure(
                    paths,
                    context.owner,
                    &mut cleanup,
                    &recovery,
                    "could not restore recording state",
                    error,
                );
            }
            process_identity::remove_record_if(&paths.session_owner_pid, context.owner);
            remove(&paths.session_recorder_pid);
            return Err(asr_source_recovery_error(
                "audio recorder did not terminate after SIGKILL; tracking was preserved",
                io::Error::other("recorder remains active"),
                &paths.wav,
                &recovery.transcript,
            ));
        }
        {
            let _lock = match StateLock::acquire(paths) {
                Ok(lock) => lock,
                Err(error) => {
                    return source_audio_failure(
                        paths,
                        context.owner,
                        &mut cleanup,
                        &recovery,
                        "could not lock session after stopping audio recorder",
                        error,
                    );
                }
            };
            process_identity::remove_record_if(&paths.recorder_pid, context.recorder);
        }
        match (hooks.metadata)(&paths.wav) {
            Ok(metadata) if metadata.len() == 0 => {
                cleanup.preserve_source_audio = false;
                return Ok(1);
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                cleanup.preserve_source_audio = false;
                return Ok(1);
            }
            Err(error) => {
                return source_audio_failure(
                    paths,
                    context.owner,
                    &mut cleanup,
                    &recovery,
                    "could not inspect recorded source audio",
                    error,
                );
            }
        }

        if let Some(overlay) = context.overlay {
            let _ = process_identity::signal(overlay, libc::SIGUSR1);
        }
        match cancelled(paths, context.owner) {
            Ok(true) => {
                cleanup.preserve_source_audio = false;
                return Ok(0);
            }
            Ok(false) => {}
            Err(error) => {
                return Err(asr_source_recovery_error(
                    "could not check cancellation before ASR recovery setup",
                    error,
                    &paths.wav,
                    &recovery.transcript,
                ));
            }
        }

        recovery.remove();
        if let Err(error) = (hooks.set_audio_private)(&paths.wav) {
            if cancelled(paths, context.owner).unwrap_or(false) {
                cleanup.preserve_source_audio = false;
                return Ok(0);
            }
            let _ = make_audio_private(&paths.wav);
            return Err(asr_source_recovery_error(
                "could not secure source audio for ASR recovery",
                error,
                &paths.wav,
                &recovery.transcript,
            ));
        }
        if let Err(error) = (hooks.rename_audio)(&paths.wav, &recovery.audio) {
            if cancelled(paths, context.owner).unwrap_or(false) {
                cleanup.preserve_source_audio = false;
                return Ok(0);
            }
            return Err(asr_source_recovery_error(
                "could not move source audio into ASR recovery",
                error,
                &paths.wav,
                &recovery.transcript,
            ));
        }
        cleanup.preserve_source_audio = false;
        cleanup.recovery = Some(recovery.clone());
        cleanup.preserve_recovery = true;
        if let Err(error) = (hooks.set_audio_private)(&recovery.audio) {
            if cancelled(paths, context.owner).unwrap_or(false) {
                cleanup.preserve_recovery = false;
                return Ok(0);
            }
            return Err(asr_recovery_error(
                "could not secure ASR recovery audio",
                error,
                &recovery,
            ));
        }
        let transcript = match (hooks.open_transcript)(&recovery.transcript) {
            Ok(transcript) => transcript,
            Err(error) => {
                if cancelled(paths, context.owner).unwrap_or(false) {
                    cleanup.preserve_recovery = false;
                    return Ok(0);
                }
                return Err(asr_recovery_error(
                    "could not create ASR recovery transcript",
                    error,
                    &recovery,
                ));
            }
        };
        let language_args = match language_args {
            Some(args) => args.to_vec(),
            None => {
                let settings = settings::load().map_err(|error| {
                    asr_recovery_error("could not load ASR settings", error, &recovery)
                })?;
                settings::transcriber_language_args(&settings.effective_language())
            }
        };
        let mut command = ProcessCommand::new(codex_asr);
        command.arg(&recovery.audio);
        command.args(language_args);
        kill_session_child_if_owner_dies(&mut command, context.owner);
        let mut transcriber = command
            .stdout(Stdio::from(transcript))
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| asr_recovery_error("could not start codex-asr", error, &recovery))?;
        let transcriber_identity =
            identify_child(&mut transcriber, "codex-asr").map_err(|error| {
                asr_recovery_error("codex-asr process identification failed", error, &recovery)
            })?;
        if let Err(error) = register_session_child(
            paths,
            context.owner,
            &mut transcriber,
            &paths.transcriber_pid,
        ) {
            terminate_and_reap_child(&mut transcriber, transcriber_identity, libc::SIGTERM);
            if error.kind() == io::ErrorKind::Interrupted {
                cleanup.preserve_recovery = false;
                return Ok(0);
            }
            return Err(asr_recovery_error(
                "could not register codex-asr",
                error,
                &recovery,
            ));
        }
        cleanup.transcriber = Some(transcriber_identity);
        let transcriber_status =
            match wait_for_child(&mut transcriber, Some((paths, context.owner)), asr_timeout) {
                Ok(status) => status,
                Err(error) => {
                    if cancelled(paths, context.owner).unwrap_or(false) {
                        cleanup.preserve_recovery = false;
                        return Ok(0);
                    }
                    return Err(asr_recovery_error(
                        "codex-asr wait failed",
                        error,
                        &recovery,
                    ));
                }
            };
        {
            let _lock = StateLock::acquire(paths)?;
            process_identity::remove_record_if(&paths.transcriber_pid, transcriber_identity);
        }
        cleanup.transcriber = None;

        if cancelled(paths, context.owner)? {
            cleanup.preserve_recovery = false;
            return Ok(0);
        }
        if !transcriber_status.success() {
            return Err(asr_recovery_error(
                "codex-asr failed",
                io::Error::other(format!("exited with {transcriber_status}")),
                &recovery,
            ));
        }
        let mut text = String::new();
        File::open(&recovery.transcript)?.read_to_string(&mut text)?;
        trim_trailing_newlines(&mut text);
        if text.is_empty() || text == "..." {
            cleanup.preserve_recovery = false;
            close_overlay_before_paste(paths, context.overlay)?;
            return Ok(1);
        }

        close_overlay_before_paste(paths, context.overlay)?;

        let publication_lock = StateLock::acquire(paths)?;
        if is_cancelled(paths, context.owner) || INTERRUPTED.load(Ordering::SeqCst) {
            cleanup.preserve_recovery = false;
            return Ok(0);
        }
        let inserted_history = match transcript_history::add(&text) {
            Ok(inserted) => inserted,
            Err(error) => {
                drop(publication_lock);
                return Err(asr_recovery_error(
                    "could not persist transcript history",
                    error,
                    &recovery,
                ));
            }
        };
        drop(publication_lock);
        recovery.remove();
        cleanup.preserve_recovery = false;
        let delivery = (|| -> io::Result<HistoryDelivery> {
            let clipboard_lock = StateLock::acquire(paths)?;
            if is_cancelled(paths, context.owner) || INTERRUPTED.load(Ordering::SeqCst) {
                return Ok(HistoryDelivery::Cancelled);
            }
            copy_to_clipboard(&text)?;
            drop(clipboard_lock);
            let ydotool =
                product_package::command("ydotool").ok_or_else(paste_automation::missing)?;
            let paste = match prepare_paste_automation(paths, context.owner, &ydotool) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {
                    return Ok(HistoryDelivery::Cancelled)
                }
                result => result?,
            };
            let lock = StateLock::acquire(paths)?;
            if is_cancelled(paths, context.owner) || INTERRUPTED.load(Ordering::SeqCst) {
                return Ok(HistoryDelivery::Cancelled);
            }
            runtime_state::publish(paths, runtime_state::State::Typing, context.owner)?;
            // Shift+Insert is understood as paste by both common graphical text
            // controls and terminal emulators on Linux.
            let mut command = ProcessCommand::new(ydotool);
            command
                .args(paste.args())
                .stdout(Stdio::null())
                .stderr(Stdio::inherit());
            kill_session_child_if_owner_dies(&mut command, context.owner);
            let mut pasting = command.spawn()?;
            let pasting_identity = identify_child(&mut pasting, "clipboard paste")?;
            if let Err(error) = register_child_locked(&mut pasting, &paths.typing_pid) {
                drop(lock);
                terminate_and_reap_child(&mut pasting, pasting_identity, libc::SIGTERM);
                return Err(error);
            }
            cleanup.typing = Some(pasting_identity);
            drop(lock);
            let status = wait_for_child(
                &mut pasting,
                Some((paths, context.owner)),
                Duration::from_secs(10),
            )?;
            let _lock = StateLock::acquire(paths)?;
            process_identity::remove_record_if(&paths.typing_pid, pasting_identity);
            cleanup.typing = None;
            if is_cancelled(paths, context.owner) || INTERRUPTED.load(Ordering::SeqCst) {
                return Ok(HistoryDelivery::Cancelled);
            }
            if !status.success() {
                return Err(io::Error::other(format!(
                    "clipboard paste command exited with {status}"
                )));
            }
            complete_session_locked(paths, context.owner);
            Ok(HistoryDelivery::Completed)
        })();
        match delivery {
            Ok(HistoryDelivery::Cancelled) => {
                let _lock = StateLock::acquire(paths)?;
                inserted_history.rollback()?;
                Ok(0)
            }
            Err(error) => {
                history_error_or_cancelled(paths, context.owner, &inserted_history, error)
            }
            Ok(HistoryDelivery::Completed) => {
                println!("{text}");
                Ok(0)
            }
        }
    }

    enum HistoryDelivery {
        Completed,
        Cancelled,
    }

    fn history_error_or_cancelled(
        paths: &Paths,
        owner: ProcessIdentity,
        inserted_history: &transcript_history::InsertedTranscript,
        error: io::Error,
    ) -> io::Result<u8> {
        let _lock = StateLock::acquire(paths)?;
        if rollback_history_if_cancelled_locked(paths, owner, inserted_history)? {
            Ok(0)
        } else {
            Err(error)
        }
    }

    fn rollback_history_if_cancelled_locked(
        paths: &Paths,
        owner: ProcessIdentity,
        inserted_history: &transcript_history::InsertedTranscript,
    ) -> io::Result<bool> {
        if !is_cancelled(paths, owner) && !INTERRUPTED.load(Ordering::SeqCst) {
            return Ok(false);
        }
        inserted_history.rollback()?;
        Ok(true)
    }

    fn complete_session_locked(paths: &Paths, owner: ProcessIdentity) {
        if process_identity::remove_record_if(&paths.session_owner_pid, owner) {
            remove(&paths.session_recorder_pid);
            process_identity::remove_record_if(&paths.cancel, owner);
            runtime_state::clear_if_owner(paths, owner);
        }
    }

    fn start_recording_locked(paths: &Paths) -> io::Result<u8> {
        prepare_new_recording_locked(paths)?;
        let arecord = product_package::command("arecord").ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "arecord was not found in PATH")
        })?;
        let mut recorder = ProcessCommand::new(arecord)
            .args(["-q", "-f", "S16_LE", "-r", "48000", "-c", "1"])
            .arg(&paths.wav)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        let recorder_identity = identify_child(&mut recorder, "audio recorder")?;
        if let Err(error) = process_identity::write_record(&paths.recorder_pid, recorder_identity) {
            terminate_and_reap_child(&mut recorder, recorder_identity, libc::SIGTERM);
            return Err(error);
        }
        thread::sleep(Duration::from_millis(100));
        let startup_status = match recorder.try_wait() {
            Ok(status) => status,
            Err(error) => {
                terminate_and_reap_child(&mut recorder, recorder_identity, libc::SIGTERM);
                process_identity::remove_record_if(&paths.recorder_pid, recorder_identity);
                remove(&paths.wav);
                return Err(error);
            }
        };
        if let Some(status) = startup_status {
            let mut detail = String::new();
            if let Some(mut stderr) = recorder.stderr.take() {
                let _ = stderr.read_to_string(&mut detail);
            }
            process_identity::remove_record_if(&paths.recorder_pid, recorder_identity);
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
        if let Err(error) =
            runtime_state::publish(paths, runtime_state::State::Recording, recorder_identity)
        {
            terminate_and_reap_child(&mut recorder, recorder_identity, libc::SIGTERM);
            process_identity::remove_record_if(&paths.recorder_pid, recorder_identity);
            remove(&paths.wav);
            return Err(error);
        }

        let Some(overlay) = product_package::overlay_script() else {
            terminate_and_reap_child(&mut recorder, recorder_identity, libc::SIGTERM);
            process_identity::remove_record_if(&paths.recorder_pid, recorder_identity);
            runtime_state::clear_if_owner(paths, recorder_identity);
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "codex-voice overlay is not installed",
            ));
        };
        let control_command = match env::current_exe() {
            Ok(command) => command,
            Err(error) => {
                terminate_and_reap_child(&mut recorder, recorder_identity, libc::SIGTERM);
                process_identity::remove_record_if(&paths.recorder_pid, recorder_identity);
                runtime_state::clear_if_owner(paths, recorder_identity);
                return Err(error);
            }
        };
        let mut command = ProcessCommand::new("python3");
        command
            .arg(overlay)
            .arg("--audio-file")
            .arg(&paths.wav)
            .arg("--control-command")
            .arg(control_command)
            .arg("--recorder-pid")
            .arg(recorder_identity.pid.to_string())
            .arg("--recorder-start-time")
            .arg(recorder_identity.start_time.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        if let Some(backend) = product_package::overlay_backend() {
            command.env("GDK_BACKEND", backend);
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                terminate_and_reap_child(&mut recorder, recorder_identity, libc::SIGTERM);
                process_identity::remove_record_if(&paths.recorder_pid, recorder_identity);
                runtime_state::clear_if_owner(paths, recorder_identity);
                return Err(error);
            }
        };
        let identity = match identify_child(&mut child, "GTK overlay") {
            Ok(identity) => identity,
            Err(error) => {
                terminate_and_reap_child(&mut recorder, recorder_identity, libc::SIGTERM);
                process_identity::remove_record_if(&paths.recorder_pid, recorder_identity);
                runtime_state::clear_if_owner(paths, recorder_identity);
                return Err(error);
            }
        };
        if let Err(error) = process_identity::write_record(&paths.overlay_pid, identity) {
            terminate_and_reap_child(&mut child, identity, libc::SIGTERM);
            terminate_and_reap_child(&mut recorder, recorder_identity, libc::SIGTERM);
            process_identity::remove_record_if(&paths.recorder_pid, recorder_identity);
            runtime_state::clear_if_owner(paths, recorder_identity);
            return Err(error);
        }
        thread::sleep(Duration::from_millis(100));
        let overlay_status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                terminate_and_reap_child(&mut child, identity, libc::SIGTERM);
                process_identity::remove_record_if(&paths.overlay_pid, identity);
                terminate_and_reap_child(&mut recorder, recorder_identity, libc::SIGTERM);
                process_identity::remove_record_if(&paths.recorder_pid, recorder_identity);
                runtime_state::clear_if_owner(paths, recorder_identity);
                return Err(error);
            }
        };
        if let Some(status) = overlay_status {
            let mut detail = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut detail);
            }
            process_identity::remove_record_if(&paths.overlay_pid, identity);
            terminate_and_reap_child(&mut recorder, recorder_identity, libc::SIGTERM);
            process_identity::remove_record_if(&paths.recorder_pid, recorder_identity);
            runtime_state::clear_if_owner(paths, recorder_identity);
            return Err(io::Error::other(format!(
                "GTK overlay exited during startup ({status}){}",
                if detail.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", detail.trim())
                }
            )));
        }
        let watchdog = env::current_exe().and_then(|executable| {
            ProcessCommand::new(executable)
                .args([
                    "--watch-session".to_owned(),
                    identity.pid.to_string(),
                    identity.start_time.to_string(),
                    recorder_identity.pid.to_string(),
                    recorder_identity.start_time.to_string(),
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        });
        if let Err(error) = watchdog {
            terminate_and_reap_child(&mut child, identity, libc::SIGTERM);
            process_identity::remove_record_if(&paths.overlay_pid, identity);
            terminate_and_reap_child(&mut recorder, recorder_identity, libc::SIGTERM);
            process_identity::remove_record_if(&paths.recorder_pid, recorder_identity);
            runtime_state::clear_if_owner(paths, recorder_identity);
            return Err(error);
        }
        Ok(0)
    }

    fn prepare_new_recording_locked(paths: &Paths) -> io::Result<()> {
        runtime_state::cleanup_stale(paths);
        for path in [&paths.wav, &paths.transcript] {
            remove(path);
        }
        terminate_record_locked(&paths.overlay_pid, libc::SIGTERM)?;
        Ok(())
    }

    pub(crate) fn watch_session(
        paths: &Paths,
        overlay: ProcessIdentity,
        recorder: ProcessIdentity,
    ) -> io::Result<u8> {
        while overlay.is_alive() && recorder.is_alive() {
            thread::sleep(Duration::from_millis(100));
        }
        if !overlay.is_alive() && recorder.is_alive() {
            cancel_if_recorder_matches(paths, recorder)?;
        }
        Ok(0)
    }

    fn spawn_owner_supervisor(owner: ProcessIdentity) -> io::Result<()> {
        ProcessCommand::new(env::current_exe()?)
            .args([
                "--supervise-owner".to_owned(),
                owner.pid.to_string(),
                owner.start_time.to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    }

    pub(crate) fn supervise_owner(paths: &Paths, owner: ProcessIdentity) -> io::Result<u8> {
        process_identity::wait_for_exit(owner)?;
        let _lock = StateLock::acquire(paths)?;
        cleanup_abandoned_session_locked(paths, owner)?;
        Ok(0)
    }

    pub(super) fn cancel_if_recorder_matches(
        paths: &Paths,
        expected_recorder: ProcessIdentity,
    ) -> io::Result<bool> {
        let lock = StateLock::acquire(paths)?;
        repair_orphans_locked(paths)?;
        let owner = process_identity::read_active_record(&paths.session_owner_pid);
        let matches_recorder =
            process_identity::read_active_record(&paths.recorder_pid) == Some(expected_recorder);
        let matches_session = owner.is_some()
            && process_identity::read_record(&paths.session_recorder_pid)
                == Some(expected_recorder);
        if !matches_recorder && !matches_session {
            return Ok(false);
        }
        process_identity::write_record(&paths.cancel, owner.unwrap_or(expected_recorder))?;
        let targets = cancellation_targets_locked(paths, owner);
        drop(lock);
        terminate_cancellation_targets(paths, targets)?;
        Ok(true)
    }

    pub(crate) fn status(paths: &Paths) -> io::Result<String> {
        let _lock = StateLock::acquire(paths)?;
        repair_orphans_locked(paths)?;
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
        let lock = StateLock::acquire(paths)?;
        terminate_record_locked(&paths.preview_overlay_pid, libc::SIGTERM)?;

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
        let identity = identify_child(&mut child, "preview overlay")?;
        if let Err(error) = register_child_locked(&mut child, &paths.preview_overlay_pid) {
            drop(lock);
            terminate_and_reap_child(&mut child, identity, libc::SIGTERM);
            return Err(error);
        }
        drop(lock);
        let result = wait_for_child(&mut child, None, Duration::from_secs(24 * 60 * 60));
        let _lock = StateLock::acquire(paths)?;
        process_identity::remove_record_if(&paths.preview_overlay_pid, identity);
        let status = result?;
        Ok(status.code().unwrap_or(0) as u8)
    }

    pub(crate) fn close_preview(paths: &Paths) -> io::Result<u8> {
        let lock = StateLock::acquire(paths)?;
        let preview = process_identity::read_active_record(&paths.preview_overlay_pid);
        drop(lock);
        if let Some(preview) = preview {
            terminate_owned_record(paths, &paths.preview_overlay_pid, preview, libc::SIGTERM)?;
        }
        Ok(0)
    }

    fn repair_orphans_locked(paths: &Paths) -> io::Result<()> {
        let owner = process_identity::read_record(&paths.session_owner_pid);
        if let Some(owner) = owner.filter(|owner| !owner.is_alive()) {
            cleanup_abandoned_session_locked(paths, owner)?;
        }
        let active_owner = process_identity::read_active_record(&paths.session_owner_pid);
        if active_owner.is_none() {
            remove(&paths.session_recorder_pid);
            terminate_record_locked(&paths.transcriber_pid, libc::SIGTERM)?;
            terminate_record_locked(&paths.typing_pid, libc::SIGTERM)?;
        }
        let recorder = process_identity::read_active_record(&paths.recorder_pid);
        if active_owner.is_none() && recorder.is_none() {
            terminate_record_locked(&paths.overlay_pid, libc::SIGTERM)?;
        }
        runtime_state::cleanup_stale(paths);
        Ok(())
    }

    fn cleanup_abandoned_session_locked(paths: &Paths, owner: ProcessIdentity) -> io::Result<bool> {
        if process_identity::read_record(&paths.session_owner_pid) != Some(owner) {
            return Ok(false);
        }

        let session_recorder = process_identity::read_record(&paths.session_recorder_pid);
        let mut first_error = None;
        if session_recorder.is_some()
            && process_identity::read_record(&paths.recorder_pid) == session_recorder
        {
            if let Err(error) = terminate_record_locked(&paths.recorder_pid, libc::SIGINT) {
                first_error.get_or_insert(error);
            }
        }
        for path in [
            &paths.transcriber_pid,
            &paths.typing_pid,
            &paths.overlay_pid,
        ] {
            if let Err(error) = terminate_record_locked(path, libc::SIGTERM) {
                first_error.get_or_insert(error);
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }

        process_identity::remove_record_if(&paths.session_owner_pid, owner);
        remove(&paths.session_recorder_pid);
        process_identity::remove_record_if(&paths.cancel, owner);
        runtime_state::clear_if_owner(paths, owner);
        remove(&paths.wav);
        RecoveryPaths::for_session(paths, owner).remove();
        Ok(true)
    }

    struct CancellationTargets {
        owner: Option<ProcessIdentity>,
        recorder: Option<ProcessIdentity>,
        transcriber: Option<ProcessIdentity>,
        typing: Option<ProcessIdentity>,
        overlay: Option<ProcessIdentity>,
    }

    fn cancellation_targets_locked(
        paths: &Paths,
        owner: Option<ProcessIdentity>,
    ) -> CancellationTargets {
        CancellationTargets {
            owner,
            recorder: process_identity::read_active_record(&paths.recorder_pid),
            transcriber: process_identity::read_active_record(&paths.transcriber_pid),
            typing: process_identity::read_active_record(&paths.typing_pid),
            overlay: process_identity::read_active_record(&paths.overlay_pid),
        }
    }

    fn terminate_cancellation_targets(
        paths: &Paths,
        targets: CancellationTargets,
    ) -> io::Result<()> {
        let mut first_error = None;
        for (path, identity, first_signal) in [
            (&paths.recorder_pid, targets.recorder, libc::SIGINT),
            (&paths.transcriber_pid, targets.transcriber, libc::SIGTERM),
            (&paths.typing_pid, targets.typing, libc::SIGTERM),
            (&paths.overlay_pid, targets.overlay, libc::SIGTERM),
            (&paths.session_owner_pid, targets.owner, libc::SIGTERM),
        ] {
            if let Some(identity) = identity {
                if let Err(error) = terminate_owned_record(paths, path, identity, first_signal) {
                    first_error.get_or_insert(error);
                }
            }
        }
        if let Ok(_lock) = StateLock::acquire(paths) {
            if let Some(owner) = targets.owner {
                RecoveryPaths::for_session(paths, owner).remove();
                if !owner.is_alive() {
                    runtime_state::clear_if_owner(paths, owner);
                }
            } else if let Some(recorder) = targets.recorder {
                if !recorder.is_alive() {
                    runtime_state::clear_if_owner(paths, recorder);
                }
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    #[cfg(test)]
    mod asr_recovery_tests {
        use super::*;

        fn paths(name: &str) -> Paths {
            let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tmp")
                .join(format!("codex-voice-asr-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&directory);
            fs::create_dir_all(&directory).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            Paths::in_directory(directory)
        }

        fn context(paths: &Paths) -> (StopContext, RecoveryPaths) {
            let owner = ProcessIdentity::current().unwrap();
            let recorder = ProcessIdentity {
                pid: i32::MAX,
                start_time: 1,
            };
            process_identity::write_record(&paths.session_owner_pid, owner).unwrap();
            process_identity::write_record(&paths.session_recorder_pid, recorder).unwrap();
            process_identity::write_record(&paths.recorder_pid, recorder).unwrap();
            fs::write(&paths.wav, b"recoverable audio").unwrap();
            (
                StopContext {
                    owner,
                    recorder,
                    overlay: None,
                },
                RecoveryPaths::for_session(paths, owner),
            )
        }

        fn script(paths: &Paths, name: &str, body: &str) -> PathBuf {
            let path = paths.lock.parent().unwrap().join(name);
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o700)
                .open(&path)
                .unwrap();
            writeln!(file, "#!/bin/sh").unwrap();
            writeln!(file, "{body}").unwrap();
            path
        }

        fn assert_private_recovery(recovery: &RecoveryPaths, transcript: &str) {
            assert_eq!(fs::read(&recovery.audio).unwrap(), b"recoverable audio");
            assert_eq!(
                fs::read_to_string(&recovery.transcript).unwrap(),
                transcript
            );
            for path in [&recovery.audio, &recovery.transcript] {
                assert_eq!(
                    fs::metadata(path).unwrap().permissions().mode() & 0o777,
                    0o600,
                    "{} was not private",
                    path.display()
                );
            }
        }

        fn begin_next_recording(paths: &Paths) {
            let _lock = StateLock::acquire(paths).unwrap();
            prepare_new_recording_locked(paths).unwrap();
            fs::write(&paths.wav, b"next recording").unwrap();
        }

        #[test]
        fn nonzero_asr_preserves_private_recovery_across_next_recording() {
            let paths = paths("nonzero");
            let (context, recovery) = context(&paths);
            let asr = script(
                &paths,
                "nonzero-asr",
                "printf 'partial transcript'; sleep 0.1; exit 23",
            );

            let error = stop_recording_with_options(
                &paths,
                context,
                &asr,
                Some(&[]),
                Duration::from_secs(2),
            )
            .unwrap_err();
            let message = error.to_string();
            assert!(message.contains("codex-asr failed"));
            assert!(message.contains(&recovery.audio.display().to_string()));
            assert!(message.contains(&recovery.transcript.display().to_string()));
            assert_private_recovery(&recovery, "partial transcript");

            begin_next_recording(&paths);
            assert_private_recovery(&recovery, "partial transcript");
            assert_eq!(fs::read(&paths.wav).unwrap(), b"next recording");
            fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
        }

        #[test]
        fn asr_spawn_failure_preserves_recovery_across_next_recording() {
            let paths = paths("spawn-failure");
            let (context, recovery) = context(&paths);
            let missing_asr = paths.lock.parent().unwrap().join("missing-codex-asr");

            let error = stop_recording_with_options(
                &paths,
                context,
                &missing_asr,
                Some(&[]),
                Duration::from_secs(2),
            )
            .unwrap_err();
            let message = error.to_string();
            assert!(message.contains("could not start codex-asr"));
            assert!(message.contains(&recovery.audio.display().to_string()));
            assert!(message.contains(&recovery.transcript.display().to_string()));
            assert_private_recovery(&recovery, "");

            begin_next_recording(&paths);
            assert_private_recovery(&recovery, "");
            assert_eq!(fs::read(&paths.wav).unwrap(), b"next recording");
            fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
        }

        #[test]
        fn source_metadata_failure_preserves_private_and_reports_original_audio() {
            let paths = paths("source-metadata-failure");
            let (context, recovery) = context(&paths);
            let mut hooks = StopRecordingHooks::production();
            hooks.metadata = |_| Err(io::Error::from_raw_os_error(libc::EIO));

            let error = stop_recording_with_hooks(
                &paths,
                context,
                Path::new("unused-asr"),
                Some(&[]),
                Duration::from_secs(2),
                hooks,
            )
            .unwrap_err();
            let message = error.to_string();
            assert!(message.contains("could not inspect recorded source audio"));
            assert!(message.contains(&paths.wav.display().to_string()));
            assert!(message.contains(&recovery.transcript.display().to_string()));
            assert_eq!(fs::read(&paths.wav).unwrap(), b"recoverable audio");
            assert_eq!(
                fs::metadata(&paths.wav).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert!(!recovery.audio.exists());
            assert!(!recovery.transcript.exists());
            fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
        }

        #[test]
        fn genuinely_empty_source_audio_is_discarded() {
            let paths = paths("empty-source");
            let (context, recovery) = context(&paths);
            fs::write(&paths.wav, b"").unwrap();

            assert_eq!(
                stop_recording_with_options(
                    &paths,
                    context,
                    Path::new("unused-asr"),
                    Some(&[]),
                    Duration::from_secs(2),
                )
                .unwrap(),
                1
            );
            assert!(!paths.wav.exists());
            assert!(!recovery.audio.exists());
            assert!(!recovery.transcript.exists());
            fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
        }

        #[test]
        fn missing_source_audio_is_not_reported_as_a_recovery_failure() {
            let paths = paths("missing-source");
            let (context, recovery) = context(&paths);
            fs::remove_file(&paths.wav).unwrap();

            assert_eq!(
                stop_recording_with_options(
                    &paths,
                    context,
                    Path::new("unused-asr"),
                    Some(&[]),
                    Duration::from_secs(2),
                )
                .unwrap(),
                1
            );
            assert!(!recovery.audio.exists());
            assert!(!recovery.transcript.exists());
            fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
        }

        #[test]
        fn source_permission_failure_preserves_and_reports_original_audio() {
            let paths = paths("source-permission-failure");
            let (context, recovery) = context(&paths);
            let mut hooks = StopRecordingHooks::production();
            hooks.set_audio_private = |_| Err(io::Error::from_raw_os_error(libc::EACCES));
            let error = stop_recording_with_hooks(
                &paths,
                context,
                Path::new("unused-asr"),
                Some(&[]),
                Duration::from_secs(2),
                hooks,
            )
            .unwrap_err();
            let message = error.to_string();
            assert!(message.contains("could not secure source audio for ASR recovery"));
            assert!(message.contains(&paths.wav.display().to_string()));
            assert!(message.contains(&recovery.transcript.display().to_string()));
            assert_eq!(fs::read(&paths.wav).unwrap(), b"recoverable audio");
            assert_eq!(
                fs::metadata(&paths.wav).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert!(!recovery.audio.exists());
            assert!(!recovery.transcript.exists());
            assert_eq!(
                fs::metadata(paths.wav.parent().unwrap())
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
        }

        #[test]
        fn source_rename_failure_preserves_private_and_reports_original_audio() {
            let paths = paths("source-rename-failure");
            let (context, recovery) = context(&paths);
            let mut hooks = StopRecordingHooks::production();
            hooks.rename_audio = |_, _| Err(io::Error::from_raw_os_error(libc::EXDEV));
            let error = stop_recording_with_hooks(
                &paths,
                context,
                Path::new("unused-asr"),
                Some(&[]),
                Duration::from_secs(2),
                hooks,
            )
            .unwrap_err();
            let message = error.to_string();
            assert!(message.contains("could not move source audio into ASR recovery"));
            assert!(message.contains(&paths.wav.display().to_string()));
            assert!(message.contains(&recovery.transcript.display().to_string()));
            assert_eq!(fs::read(&paths.wav).unwrap(), b"recoverable audio");
            assert_eq!(
                fs::metadata(&paths.wav).unwrap().permissions().mode() & 0o777,
                0o600
            );
            assert!(!recovery.audio.exists());
            assert!(!recovery.transcript.exists());
            fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
        }

        #[test]
        fn transcript_create_disk_full_preserves_private_recovery_across_next_recording() {
            let paths = paths("transcript-create-disk-full");
            let (context, recovery) = context(&paths);
            let mut hooks = StopRecordingHooks::production();
            hooks.open_transcript = |path| {
                let mut partial = open_private_recovery_transcript(path)?;
                partial.write_all(b"partial setup output")?;
                Err(io::Error::from_raw_os_error(libc::ENOSPC))
            };
            let error = stop_recording_with_hooks(
                &paths,
                context,
                Path::new("unused-asr"),
                Some(&[]),
                Duration::from_secs(2),
                hooks,
            )
            .unwrap_err();
            let message = error.to_string();
            assert!(message.contains("could not create ASR recovery transcript"));
            assert!(message.contains("os error 28"));
            assert!(message.contains(&recovery.audio.display().to_string()));
            assert!(message.contains(&recovery.transcript.display().to_string()));
            assert_private_recovery(&recovery, "partial setup output");

            begin_next_recording(&paths);
            assert_private_recovery(&recovery, "partial setup output");
            assert_eq!(fs::read(&paths.wav).unwrap(), b"next recording");
            fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
        }

        #[test]
        fn cancellation_discards_partial_asr_recovery() {
            let paths = paths("cancel");
            let (context, recovery) = context(&paths);
            let owner = context.owner;
            let asr = script(
                &paths,
                "cancelled-asr",
                "printf 'discard me'; while :; do sleep 1; done",
            );
            let cancellation_paths = paths.clone();
            let transcript = recovery.transcript.clone();
            let cancellation = thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(2);
                while fs::metadata(&transcript).map_or(true, |metadata| metadata.len() == 0) {
                    assert!(
                        Instant::now() < deadline,
                        "ASR did not produce partial output"
                    );
                    thread::sleep(Duration::from_millis(10));
                }
                process_identity::write_record(&cancellation_paths.cancel, owner).unwrap();
            });

            assert_eq!(
                stop_recording_with_options(
                    &paths,
                    context,
                    &asr,
                    Some(&[]),
                    Duration::from_secs(2),
                )
                .unwrap(),
                0
            );
            cancellation.join().unwrap();
            assert!(!recovery.audio.exists());
            assert!(!recovery.transcript.exists());
            fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
        }

        #[test]
        fn successful_empty_asr_cleans_recovery_artifacts() {
            let paths = paths("success");
            let (context, recovery) = context(&paths);
            let asr = script(&paths, "successful-asr", "sleep 0.1; exit 0");

            assert_eq!(
                stop_recording_with_options(
                    &paths,
                    context,
                    &asr,
                    Some(&[]),
                    Duration::from_secs(2),
                )
                .unwrap(),
                1
            );
            assert!(!recovery.audio.exists());
            assert!(!recovery.transcript.exists());
            fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
        }
    }

    #[cfg(test)]
    mod history_publication_tests {
        use super::*;
        use std::sync::{Arc, Barrier};

        fn error_after_waiting_cancel(name: &str, error: io::Error) {
            let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tmp")
                .join(format!("codex-voice-history-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&directory);
            fs::create_dir_all(&directory).unwrap();
            let paths = Paths::in_directory(directory.clone());
            let database = directory.join("history.sqlite3");
            let owner = ProcessIdentity::current().unwrap();
            process_identity::write_record(&paths.session_owner_pid, owner).unwrap();

            let operation_lock = StateLock::acquire(&paths).unwrap();
            let inserted =
                transcript_history::insert_at_for_test(&database, "cancelled session").unwrap();
            let cancel_started = Arc::new(Barrier::new(2));
            let cancel_written = Arc::new(Barrier::new(2));
            let cancellation_paths = paths.clone();
            let cancellation_database = database.clone();
            let cancellation_started = Arc::clone(&cancel_started);
            let cancellation_finished = Arc::clone(&cancel_written);
            let cancellation = thread::spawn(move || {
                cancellation_started.wait();
                let _lock = StateLock::acquire(&cancellation_paths).unwrap();
                process_identity::write_record(&cancellation_paths.cancel, owner).unwrap();
                transcript_history::insert_at_for_test(&cancellation_database, "newer history")
                    .unwrap();
                cancellation_finished.wait();
            });

            cancel_started.wait();
            drop(operation_lock);
            cancel_written.wait();
            assert_eq!(
                history_error_or_cancelled(&paths, owner, &inserted, error).unwrap(),
                0
            );
            cancellation.join().unwrap();
            assert_eq!(
                transcript_history::texts_at_for_test(&database).unwrap(),
                vec!["newer history"]
            );
            fs::remove_dir_all(directory).unwrap();
        }

        #[test]
        fn waiting_cancel_rolls_back_only_the_sessions_inserted_history() {
            let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tmp")
                .join(format!(
                    "codex-voice-history-cancel-race-{}",
                    std::process::id()
                ));
            let _ = fs::remove_dir_all(&directory);
            fs::create_dir_all(&directory).unwrap();
            let paths = Paths::in_directory(directory.clone());
            let database = directory.join("history.sqlite3");
            let owner = ProcessIdentity::current().unwrap();
            process_identity::write_record(&paths.session_owner_pid, owner).unwrap();

            let publication_lock = StateLock::acquire(&paths).unwrap();
            let inserted =
                transcript_history::insert_at_for_test(&database, "cancelled session").unwrap();
            let published = Arc::new(Barrier::new(2));
            let cancellation_written = Arc::new(Barrier::new(2));
            let cancellation_paths = paths.clone();
            let cancellation_database = database.clone();
            let cancellation_published = Arc::clone(&published);
            let cancellation_finished = Arc::clone(&cancellation_written);
            let cancellation = thread::spawn(move || {
                cancellation_published.wait();
                let _lock = StateLock::acquire(&cancellation_paths).unwrap();
                process_identity::write_record(&cancellation_paths.cancel, owner).unwrap();
                transcript_history::insert_at_for_test(&cancellation_database, "newer history")
                    .unwrap();
                cancellation_finished.wait();
            });

            published.wait();
            drop(publication_lock);
            cancellation_written.wait();
            let _lock = StateLock::acquire(&paths).unwrap();
            assert!(rollback_history_if_cancelled_locked(&paths, owner, &inserted).unwrap());
            drop(_lock);
            cancellation.join().unwrap();

            assert_eq!(
                transcript_history::texts_at_for_test(&database).unwrap(),
                vec!["newer history"]
            );
            fs::remove_dir_all(directory).unwrap();
        }

        #[test]
        fn waiting_cancel_wins_over_missing_ydotool_error() {
            error_after_waiting_cancel("missing-ydotool", paste_automation::missing());
        }

        #[test]
        fn waiting_cancel_wins_over_paste_process_error() {
            error_after_waiting_cancel(
                "paste-process-error",
                io::Error::other("clipboard paste process could not be identified"),
            );
        }

        #[test]
        fn genuine_post_history_error_retains_the_inserted_history() {
            let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tmp")
                .join(format!(
                    "codex-voice-history-genuine-error-{}",
                    std::process::id()
                ));
            let _ = fs::remove_dir_all(&directory);
            fs::create_dir_all(&directory).unwrap();
            let paths = Paths::in_directory(directory.clone());
            let database = directory.join("history.sqlite3");
            let owner = ProcessIdentity::current().unwrap();
            let inserted =
                transcript_history::insert_at_for_test(&database, "durable history").unwrap();

            let error = history_error_or_cancelled(
                &paths,
                owner,
                &inserted,
                io::Error::other("genuine paste failure"),
            )
            .unwrap_err();

            assert_eq!(error.to_string(), "genuine paste failure");
            assert_eq!(
                transcript_history::texts_at_for_test(&database).unwrap(),
                vec!["durable history"]
            );
            fs::remove_dir_all(directory).unwrap();
        }
    }
}

fn prepare_paste_automation(
    paths: &Paths,
    owner: ProcessIdentity,
    ydotool: &Path,
) -> io::Result<paste_automation::PasteCommand> {
    let output_path = paths.lock.with_file_name(format!(
        "codex-voice-ydotool-help-{}-{}.txt",
        owner.pid, owner.start_time
    ));
    remove(&output_path);
    let result = (|| {
        let output = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&output_path)?;
        let mut command = paste_automation::inspection_command(ydotool);
        command
            .stdout(Stdio::from(output.try_clone()?))
            .stderr(Stdio::from(output));
        kill_session_child_if_owner_dies(&mut command, owner);
        let mut child = command.spawn().map_err(|error| {
            paste_automation::inspection_failed(format!("could not inspect ydotool: {error}"))
        })?;
        let wait_result = if let Some(identity) = ProcessIdentity::for_pid(child.id() as i32) {
            if let Err(error) = register_session_child(paths, owner, &mut child, &paths.typing_pid)
            {
                match child.try_wait()? {
                    Some(status) => Ok(status),
                    None => {
                        terminate_and_reap_child(&mut child, identity, libc::SIGTERM);
                        return Err(error);
                    }
                }
            } else {
                let wait_result =
                    wait_for_child(&mut child, Some((paths, owner)), Duration::from_secs(2));
                {
                    let _lock = StateLock::acquire(paths)?;
                    process_identity::remove_record_if(&paths.typing_pid, identity);
                }
                wait_result
            }
        } else {
            match child.try_wait()? {
                Some(status) => Ok(status),
                None => {
                    let _ = child.kill();
                    let _ = child.wait();
                    Err(io::Error::other(
                        "could not identify ydotool inspection process",
                    ))
                }
            }
        };
        wait_result.map_err(|error| {
            paste_automation::inspection_failed(format!(
                "ydotool protocol inspection did not complete: {error}"
            ))
        })?;
        if cancelled(paths, owner)? {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "dictation cancelled",
            ));
        }
        let mut help = Vec::new();
        File::open(&output_path)?
            .take(paste_automation::INSPECTION_OUTPUT_LIMIT)
            .read_to_end(&mut help)?;
        paste_automation::prepare(&help)
    })();
    remove(&output_path);
    result
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

fn set_active_child(identity: ProcessIdentity) -> io::Result<()> {
    let pidfd = process_identity::open_pidfd(identity)?
        .ok_or_else(|| io::Error::other("child process exited before registration"))?;
    let previous = ACTIVE_PIDFD.swap(pidfd, Ordering::SeqCst);
    if previous >= 0 {
        unsafe { libc::close(previous) };
    }
    Ok(())
}

fn clear_active_child() {
    let pidfd = ACTIVE_PIDFD.swap(-1, Ordering::SeqCst);
    if pidfd >= 0 {
        unsafe { libc::close(pidfd) };
    }
}

fn register_session_child(
    paths: &Paths,
    owner: ProcessIdentity,
    child: &mut Child,
    record: &Path,
) -> io::Result<()> {
    let _lock = StateLock::acquire(paths)?;
    if is_cancelled(paths, owner) || INTERRUPTED.load(Ordering::SeqCst) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "dictation cancelled",
        ));
    }
    register_child_locked(child, record)
}

fn kill_session_child_if_owner_dies(command: &mut ProcessCommand, owner: ProcessIdentity) {
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) < 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::getppid() != owner.pid {
                return Err(io::Error::from_raw_os_error(libc::ESRCH));
            }
            Ok(())
        });
    }
}

fn identify_child(child: &mut Child, name: &str) -> io::Result<ProcessIdentity> {
    if let Some(identity) = ProcessIdentity::for_pid(child.id() as i32) {
        return Ok(identity);
    }
    let _ = child.kill();
    let deadline = Instant::now() + Duration::from_millis(750);
    while Instant::now() < deadline {
        if child.try_wait().is_ok_and(|status| status.is_some()) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }
    Err(io::Error::other(format!(
        "could not identify {name} process"
    )))
}

fn register_child_locked(child: &mut Child, record: &Path) -> io::Result<()> {
    let identity = ProcessIdentity::for_pid(child.id() as i32)
        .ok_or_else(|| io::Error::other("could not identify child process"))?;
    set_active_child(identity)?;
    if let Err(error) = process_identity::write_record(record, identity) {
        clear_active_child();
        return Err(error);
    }
    Ok(())
}

fn wait_for_child(
    child: &mut Child,
    cancellation: Option<(&Paths, ProcessIdentity)>,
    timeout: Duration,
) -> io::Result<ExitStatus> {
    let identity = ProcessIdentity::for_pid(child.id() as i32)
        .ok_or_else(|| io::Error::other("could not identify child process"))?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                clear_active_child();
                return Ok(status);
            }
            Ok(None) => {}
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => {
                terminate_and_reap_child(child, identity, libc::SIGTERM);
                return Err(error);
            }
        }
        let was_cancelled = INTERRUPTED.load(Ordering::SeqCst)
            || cancellation.is_some_and(|(paths, owner)| cancelled(paths, owner).unwrap_or(true));
        if was_cancelled {
            return terminate_and_reap_child(child, identity, libc::SIGTERM)
                .ok_or_else(|| io::Error::other("cancelled child could not be reaped"));
        }
        if Instant::now() >= deadline {
            terminate_and_reap_child(child, identity, libc::SIGTERM);
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "child process exceeded its time limit",
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn terminate_and_reap_child(
    child: &mut Child,
    identity: ProcessIdentity,
    first_signal: libc::c_int,
) -> Option<ExitStatus> {
    let _ = process_identity::signal(identity, first_signal);
    let deadline = Instant::now() + Duration::from_millis(750);
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(status)) => {
                clear_active_child();
                return Some(status);
            }
            Ok(None) | Err(_) => thread::sleep(Duration::from_millis(10)),
        }
    }
    let _ = child.kill();
    let kill_deadline = Instant::now() + Duration::from_millis(750);
    while Instant::now() < kill_deadline {
        match child.try_wait() {
            Ok(Some(status)) => {
                clear_active_child();
                return Some(status);
            }
            Ok(None) | Err(_) => thread::sleep(Duration::from_millis(10)),
        }
    }
    clear_active_child();
    None
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
    let identity = identify_child(&mut child, "clipboard")?;
    set_active_child(identity)?;
    if let Some(mut stdin) = child.stdin.take() {
        if let Err(error) = stdin.write_all(text.as_bytes()) {
            drop(stdin);
            terminate_and_reap_child(&mut child, identity, libc::SIGTERM);
            return Err(error);
        }
    }
    let status = wait_for_child(&mut child, None, Duration::from_secs(10))?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "clipboard command exited with {status}"
        )));
    }
    Ok(())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension(format!("tmp.{}", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temporary, path)
}

fn terminate_record_locked(path: &Path, first_signal: libc::c_int) -> io::Result<()> {
    if let Some(identity) = process_identity::read_active_record(path) {
        if process_identity::terminate(identity, first_signal)? {
            process_identity::remove_record_if(path, identity);
        } else {
            return Err(io::Error::other(format!(
                "process {} did not terminate after SIGKILL; tracking was preserved",
                identity.pid
            )));
        }
    }
    Ok(())
}

fn terminate_owned_record(
    paths: &Paths,
    path: &Path,
    identity: ProcessIdentity,
    first_signal: libc::c_int,
) -> io::Result<()> {
    if process_identity::terminate(identity, first_signal)? {
        let _lock = StateLock::acquire(paths)?;
        process_identity::remove_record_if(path, identity);
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "process {} did not terminate after SIGKILL; tracking was preserved",
            identity.pid
        )))
    }
}

fn close_overlay_before_paste(paths: &Paths, overlay: Option<ProcessIdentity>) -> io::Result<()> {
    if let Some(overlay) = overlay {
        terminate_owned_record(paths, &paths.overlay_pid, overlay, libc::SIGTERM)?;
        // GNOME restores focus only after processing the destroyed window.
        thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

fn is_cancelled(paths: &Paths, owner: ProcessIdentity) -> bool {
    process_identity::read_record(&paths.cancel) == Some(owner)
}

fn cancelled(paths: &Paths, owner: ProcessIdentity) -> io::Result<bool> {
    let _lock = StateLock::acquire(paths)?;
    Ok(is_cancelled(paths, owner) || INTERRUPTED.load(Ordering::SeqCst))
}
fn remove(path: &Path) {
    let _ = fs::remove_file(path);
}
fn trim_trailing_newlines(text: &mut String) {
    while text.ends_with(['\n', '\r']) {
        text.pop();
    }
}
fn make_audio_private(path: &Path) -> io::Result<()> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
}
fn source_audio_metadata(path: &Path) -> io::Result<fs::Metadata> {
    fs::metadata(path)
}
fn rename_recovery_audio(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}
fn open_private_recovery_transcript(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
}
fn source_audio_failure(
    paths: &Paths,
    owner: ProcessIdentity,
    cleanup: &mut SessionCleanup<'_>,
    recovery: &RecoveryPaths,
    context: &str,
    error: io::Error,
) -> io::Result<u8> {
    if cancelled(paths, owner).unwrap_or(false) {
        cleanup.preserve_source_audio = false;
        return Ok(0);
    }
    let _ = make_audio_private(&paths.wav);
    Err(asr_source_recovery_error(
        context,
        error,
        &paths.wav,
        &recovery.transcript,
    ))
}
fn asr_source_recovery_error(
    context: &str,
    error: io::Error,
    source_audio: &Path,
    recovery_transcript: &Path,
) -> io::Error {
    io::Error::new(
        error.kind(),
        format!(
            "{context}: {error}; source audio: {}; recovery transcript: {}",
            source_audio.display(),
            recovery_transcript.display()
        ),
    )
}
fn asr_recovery_error(context: &str, error: io::Error, recovery: &RecoveryPaths) -> io::Error {
    io::Error::new(
        error.kind(),
        format!(
            "{context}: {error}; recovery audio: {}; recovery transcript: {}",
            recovery.audio.display(),
            recovery.transcript.display()
        ),
    )
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
mod reliability_tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::sync::{Arc, Barrier};

    fn test_paths(name: &str) -> Paths {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tmp")
            .join(format!("codex-voice-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        Paths::in_directory(directory)
    }

    #[test]
    fn runtime_directory_is_private() {
        let paths = test_paths("private-runtime");
        let directory = paths.lock.parent().unwrap();
        assert_eq!(
            fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn concurrent_start_claims_create_one_recorder() {
        let paths = test_paths("concurrent-start");
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let paths = paths.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(thread::spawn(move || {
                barrier.wait();
                let _lock = StateLock::acquire(&paths).unwrap();
                if process_identity::read_active_record(&paths.recorder_pid).is_some() {
                    return None;
                }
                let mut child = ProcessCommand::new("sleep").arg("30").spawn().unwrap();
                let identity = identify_child(&mut child, "test recorder").unwrap();
                process_identity::write_record(&paths.recorder_pid, identity).unwrap();
                Some((child, identity))
            }));
        }
        barrier.wait();
        let mut recorders: Vec<_> = workers
            .into_iter()
            .filter_map(|worker| worker.join().unwrap())
            .collect();
        assert_eq!(recorders.len(), 1);
        let (mut recorder, identity) = recorders.pop().unwrap();
        let status = terminate_and_reap_child(&mut recorder, identity, libc::SIGTERM).unwrap();
        assert!(!status.success());
        process_identity::remove_record_if(&paths.recorder_pid, identity);
        fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
    }

    #[test]
    fn stubborn_child_is_escalated_and_reaped() {
        let mut child = ProcessCommand::new("sh")
            .args(["-c", "trap '' TERM; while :; do sleep 1; done"])
            .spawn()
            .unwrap();
        let identity = identify_child(&mut child, "stubborn test child").unwrap();
        thread::sleep(Duration::from_millis(50));
        let status = terminate_and_reap_child(&mut child, identity, libc::SIGTERM).unwrap();
        assert_eq!(status.signal(), Some(libc::SIGKILL));
        assert!(!identity.is_alive());
    }

    #[test]
    fn hung_ydotool_inspection_is_bounded_reaped_and_unregistered() {
        let paths = test_paths("hung-ydotool-inspection");
        let directory = paths.lock.parent().unwrap();
        let marker = directory.join("inspection.pid");
        let script = directory.join("ydotool");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o700)
            .open(&script)
            .unwrap();
        writeln!(file, "#!/bin/sh").unwrap();
        writeln!(file, "printf '%s\\n' $$ > '{}'", marker.display()).unwrap();
        writeln!(file, "trap '' TERM; while :; do :; done").unwrap();
        drop(file);
        let owner = ProcessIdentity::current().unwrap();
        process_identity::write_record(&paths.session_owner_pid, owner).unwrap();

        let started = Instant::now();
        let error = prepare_paste_automation(&paths, owner, &script)
            .err()
            .expect("hung inspection should fail")
            .to_string();

        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(error.contains("protocol inspection did not complete"));
        let helper_pid: i32 = fs::read_to_string(&marker).unwrap().trim().parse().unwrap();
        assert!(ProcessIdentity::for_pid(helper_pid).is_none());
        assert!(!paths.typing_pid.exists());
        assert!(!directory
            .join(format!(
                "codex-voice-ydotool-help-{}-{}.txt",
                owner.pid, owner.start_time
            ))
            .exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn dead_recorder_overlay_cannot_cancel_replacement_recording() {
        let paths = test_paths("stale-overlay-cancel");
        let mut old_recorder = ProcessCommand::new("sleep").arg("30").spawn().unwrap();
        let old_recorder_identity = identify_child(&mut old_recorder, "old recorder").unwrap();
        process_identity::write_record(&paths.recorder_pid, old_recorder_identity).unwrap();

        let mut stale_overlay = ProcessCommand::new("sleep").arg("30").spawn().unwrap();
        let stale_overlay_identity = identify_child(&mut stale_overlay, "stale overlay").unwrap();
        process_identity::write_record(&paths.overlay_pid, stale_overlay_identity).unwrap();

        terminate_and_reap_child(&mut old_recorder, old_recorder_identity, libc::SIGTERM).unwrap();
        let overlay_reaper = thread::spawn(move || stale_overlay.wait().unwrap());
        assert!(
            !dictation_session::cancel_if_recorder_matches(&paths, old_recorder_identity).unwrap()
        );
        assert!(!overlay_reaper.join().unwrap().success());
        assert!(!paths.recorder_pid.exists());
        assert!(!paths.overlay_pid.exists());

        let mut replacement = ProcessCommand::new("sleep").arg("30").spawn().unwrap();
        let replacement_identity =
            identify_child(&mut replacement, "replacement recorder").unwrap();
        process_identity::write_record(&paths.recorder_pid, replacement_identity).unwrap();

        assert!(
            !dictation_session::cancel_if_recorder_matches(&paths, old_recorder_identity).unwrap()
        );
        assert_eq!(
            process_identity::read_active_record(&paths.recorder_pid),
            Some(replacement_identity)
        );
        assert!(replacement.try_wait().unwrap().is_none());
        assert!(!paths.cancel.exists());

        terminate_and_reap_child(&mut replacement, replacement_identity, libc::SIGTERM).unwrap();
        process_identity::remove_record_if(&paths.recorder_pid, replacement_identity);
        fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
    }

    #[test]
    fn scoped_overlay_cancel_still_cancels_its_transcribing_session() {
        let paths = test_paths("scoped-transcribing-cancel");
        let recorder = ProcessIdentity {
            pid: 42,
            start_time: 99,
        };
        let mut owner = ProcessCommand::new("sleep").arg("30").spawn().unwrap();
        let owner_identity = identify_child(&mut owner, "transcribing owner").unwrap();
        process_identity::write_record(&paths.session_owner_pid, owner_identity).unwrap();
        process_identity::write_record(&paths.session_recorder_pid, recorder).unwrap();
        let owner_reaper = thread::spawn(move || owner.wait().unwrap());

        assert!(dictation_session::cancel_if_recorder_matches(&paths, recorder).unwrap());
        assert!(!owner_reaper.join().unwrap().success());
        assert_eq!(
            process_identity::read_record(&paths.cancel),
            Some(owner_identity)
        );

        assert!(!dictation_session::cancel_if_recorder_matches(&paths, recorder).unwrap());
        assert!(!paths.session_recorder_pid.exists());
        fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
    }

    #[test]
    fn killed_owner_supervisor_escalates_hung_asr_and_cleans_session() {
        let paths = test_paths("killed-owner-hung-asr");
        let mut owner = ProcessCommand::new("sleep").arg("30").spawn().unwrap();
        let owner_identity = identify_child(&mut owner, "fault-injected owner").unwrap();
        let mut asr = ProcessCommand::new("sh")
            .args(["-c", "trap '' TERM; while :; do sleep 1; done"])
            .spawn()
            .unwrap();
        let asr_identity = identify_child(&mut asr, "fault-injected ASR").unwrap();
        let mut typing = ProcessCommand::new("sleep").arg("30").spawn().unwrap();
        let typing_identity = identify_child(&mut typing, "fault-injected typing").unwrap();
        let mut overlay = ProcessCommand::new("sleep").arg("30").spawn().unwrap();
        let overlay_identity = identify_child(&mut overlay, "fault-injected overlay").unwrap();

        process_identity::write_record(&paths.session_owner_pid, owner_identity).unwrap();
        process_identity::write_record(&paths.transcriber_pid, asr_identity).unwrap();
        process_identity::write_record(&paths.typing_pid, typing_identity).unwrap();
        process_identity::write_record(&paths.overlay_pid, overlay_identity).unwrap();
        runtime_state::publish(&paths, runtime_state::State::Transcribing, owner_identity).unwrap();
        let recovery = RecoveryPaths::for_session(&paths, owner_identity);
        fs::write(&recovery.audio, b"abandoned audio").unwrap();
        fs::write(&recovery.transcript, b"abandoned transcript").unwrap();

        let asr_reaper = thread::spawn(move || asr.wait().unwrap());
        let typing_reaper = thread::spawn(move || typing.wait().unwrap());
        let overlay_reaper = thread::spawn(move || overlay.wait().unwrap());
        let supervisor_paths = paths.clone();
        let supervisor = thread::spawn(move || {
            dictation_session::supervise_owner(&supervisor_paths, owner_identity)
        });

        process_identity::signal(owner_identity, libc::SIGKILL).unwrap();
        owner.wait().unwrap();
        assert_eq!(supervisor.join().unwrap().unwrap(), 0);
        assert_eq!(asr_reaper.join().unwrap().signal(), Some(libc::SIGKILL));
        assert!(!typing_reaper.join().unwrap().success());
        assert!(!overlay_reaper.join().unwrap().success());

        for path in [
            &paths.session_owner_pid,
            &paths.transcriber_pid,
            &paths.typing_pid,
            &paths.overlay_pid,
            &paths.runtime_state,
        ] {
            assert!(!path.exists(), "{} was not cleaned", path.display());
        }
        assert!(!asr_identity.is_alive());
        assert!(!typing_identity.is_alive());
        assert!(!overlay_identity.is_alive());
        assert!(!recovery.audio.exists());
        assert!(!recovery.transcript.exists());
        fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
    }

    #[test]
    fn stale_owner_supervisor_does_not_kill_replacement_session() {
        let paths = test_paths("stale-owner-supervisor");
        let mut replacement_owner = ProcessCommand::new("sleep").arg("30").spawn().unwrap();
        let replacement_owner_identity =
            identify_child(&mut replacement_owner, "replacement owner").unwrap();
        let stale_owner = ProcessIdentity {
            start_time: replacement_owner_identity.start_time + 1,
            ..replacement_owner_identity
        };
        let mut replacement_asr = ProcessCommand::new("sleep").arg("30").spawn().unwrap();
        let replacement_asr_identity =
            identify_child(&mut replacement_asr, "replacement ASR").unwrap();
        process_identity::write_record(&paths.session_owner_pid, replacement_owner_identity)
            .unwrap();
        process_identity::write_record(&paths.transcriber_pid, replacement_asr_identity).unwrap();

        assert_eq!(
            dictation_session::supervise_owner(&paths, stale_owner).unwrap(),
            0
        );
        assert_eq!(
            process_identity::read_active_record(&paths.session_owner_pid),
            Some(replacement_owner_identity)
        );
        assert_eq!(
            process_identity::read_active_record(&paths.transcriber_pid),
            Some(replacement_asr_identity)
        );
        assert!(replacement_owner.try_wait().unwrap().is_none());
        assert!(replacement_asr.try_wait().unwrap().is_none());

        terminate_and_reap_child(
            &mut replacement_asr,
            replacement_asr_identity,
            libc::SIGTERM,
        )
        .unwrap();
        terminate_and_reap_child(
            &mut replacement_owner,
            replacement_owner_identity,
            libc::SIGTERM,
        )
        .unwrap();
        fs::remove_dir_all(paths.lock.parent().unwrap()).unwrap();
    }
}
