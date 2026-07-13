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
        [flag] if flag == "--status" => Ok(Command::Status),
        [flag] if flag == "--settings" => Ok(Command::LaunchSettings),
        [flag] if flag == "--preview" => Ok(Command::Preview),
        [flag] if flag == "--close-preview" => Ok(Command::ClosePreview),
        [flag] if flag == "--version" => Ok(Command::Version),
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
