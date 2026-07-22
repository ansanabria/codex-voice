use codex_voice::{Application, Command};
use std::env;
use std::io;
use std::process::ExitCode;

fn parse_args(args: &[String]) -> io::Result<Command> {
    match args {
        [] => Ok(Command::Toggle),
        [flag] if flag == "--toggle" => Ok(Command::Toggle),
        [flag] if flag == "--start" => Ok(Command::Start),
        [flag] if flag == "--stop" => Ok(Command::Stop),
        [flag] if flag == "--cancel" => Ok(Command::Cancel),
        [flag, recorder_pid, recorder_start_time] if flag == "--cancel-recording" => {
            Ok(Command::CancelRecording {
                recorder_pid: recorder_pid.parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid recorder PID")
                })?,
                recorder_start_time: recorder_start_time.parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid recorder start time")
                })?,
            })
        }
        [flag] if flag == "--status" => Ok(Command::Status),
        [flag] if flag == "--settings" => Ok(Command::LaunchSettings),
        [flag] if flag == "--preview" => Ok(Command::Preview),
        [flag] if flag == "--close-preview" => Ok(Command::ClosePreview),
        [flag] if flag == "--version" => Ok(Command::Version),
        [flag] if flag == "--copy-last" => Ok(Command::CopyLastTranscript),
        [flag, overlay_pid, overlay_start_time, recorder_pid, recorder_start_time]
            if flag == "--watch-session" =>
        {
            Ok(Command::WatchSession {
                overlay_pid: overlay_pid.parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid overlay PID")
                })?,
                overlay_start_time: overlay_start_time.parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid overlay start time")
                })?,
                recorder_pid: recorder_pid.parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid recorder PID")
                })?,
                recorder_start_time: recorder_start_time.parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid recorder start time")
                })?,
            })
        }
        [flag, owner_pid, owner_start_time] if flag == "--supervise-owner" => {
            Ok(Command::SuperviseOwner {
                owner_pid: owner_pid.parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid owner PID")
                })?,
                owner_start_time: owner_start_time.parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidInput, "invalid owner start time")
                })?,
            })
        }
        [settings, command] if settings == "settings" && command == "get" => {
            Ok(Command::SettingsGet)
        }
        [settings, command] if settings == "settings" && command == "reset" => {
            Ok(Command::SettingsReset)
        }
        [settings, command, key, value] if settings == "settings" && command == "set" => {
            Ok(Command::SettingsSet {
                key: key.clone(),
                value: value.clone(),
            })
        }
        [history, command, offset, limit] if history == "history" && command == "list" => {
            Ok(Command::HistoryList {
                offset: offset.parse().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "history offset must be an integer",
                    )
                })?,
                limit: limit.parse().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "history limit must be an integer",
                    )
                })?,
                query: String::new(),
            })
        }
        [history, command, offset, limit, query] if history == "history" && command == "list" => {
            Ok(Command::HistoryList {
                offset: offset.parse().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "history offset must be an integer",
                    )
                })?,
                limit: limit.parse().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "history limit must be an integer",
                    )
                })?,
                query: query.clone(),
            })
        }
        [history, command, id] if history == "history" && command == "delete" => {
            Ok(Command::HistoryDelete {
                id: id.parse().map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "transcript id must be an integer",
                    )
                })?,
            })
        }
        [history, command] if history == "history" && command == "clear" => {
            Ok(Command::HistoryClear)
        }
        [history, command] if history == "history" && command == "has" => {
            Ok(Command::HistoryHasEntries)
        }
        [history, ..] if history == "history" => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: codex-voice history list <offset> <limit> [query]|delete <id>|clear|has",
        )),
        [flag, ..] if flag == "settings" => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: codex-voice settings get|reset|set <key> <value>",
        )),
        [flag, ..] => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("unknown argument `{flag}`"),
        )),
    }
}

fn run() -> io::Result<u8> {
    let args: Vec<String> = env::args().skip(1).collect();
    let output = Application::from_environment()?.execute(parse_args(&args)?)?;
    if let Some(stdout) = output.stdout {
        println!("{stdout}");
    }
    Ok(output.exit_code)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_command() {
        assert_eq!(parse_args(&[]).unwrap(), Command::Toggle);
        assert_eq!(parse_args(&["--status".into()]).unwrap(), Command::Status);
        assert_eq!(
            parse_args(&["--cancel-recording".into(), "42".into(), "99".into()]).unwrap(),
            Command::CancelRecording {
                recorder_pid: 42,
                recorder_start_time: 99,
            }
        );
        assert_eq!(
            parse_args(&["--supervise-owner".into(), "42".into(), "99".into()]).unwrap(),
            Command::SuperviseOwner {
                owner_pid: 42,
                owner_start_time: 99,
            }
        );
        assert_eq!(
            parse_args(&["settings".into(), "get".into()]).unwrap(),
            Command::SettingsGet
        );
        assert_eq!(
            parse_args(&[
                "settings".into(),
                "set".into(),
                "language".into(),
                "en".into()
            ])
            .unwrap(),
            Command::SettingsSet {
                key: "language".into(),
                value: "en".into()
            }
        );
    }
}
