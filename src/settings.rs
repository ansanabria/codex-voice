use crate::{invalid_input, product_package, EXTENSION_UUID};
use serde::Serialize;
use std::env;
use std::io;
use std::path::PathBuf;
use std::process::Command;

const SCHEMA: &str = "io.github.andy_spike.CodexVoice";
const DEFAULT_KEYBINDING: &str = "<Control><Super>space";

#[derive(Debug, Clone)]
pub(crate) struct Settings {
    pub(crate) enabled: bool,
    show_tray_icon: bool,
    keybinding: String,
    language: String,
    language_override: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsDocument<'a> {
    schema_version: u8,
    enabled: bool,
    show_tray_icon: bool,
    keybinding: &'a str,
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
            schema_version: crate::protocol::SCHEMA_VERSION,
            enabled: self.enabled,
            show_tray_icon: self.show_tray_icon,
            keybinding: &self.keybinding,
            language: &self.language,
            overrides: Overrides {
                language: self.language_override.as_deref(),
            },
        }
    }

    pub(crate) fn effective_language(&self) -> String {
        self.language_override
            .clone()
            .unwrap_or_else(|| self.language.clone())
    }
}

pub(crate) fn json() -> io::Result<String> {
    Ok(serde_json::to_string(&load()?.document()).expect("settings JSON is serializable"))
}

fn gsettings_command() -> Command {
    let mut command = Command::new("gsettings");
    let schema_dirs = product_package::schema_directories();
    if !schema_dirs.is_empty() {
        let mut values: Vec<String> = schema_dirs
            .into_iter()
            .map(|dir| dir.display().to_string())
            .collect();
        if let Some(old) = env::var_os("GSETTINGS_SCHEMA_DIR") {
            values.push(PathBuf::from(old).display().to_string());
        }
        command.env("GSETTINGS_SCHEMA_DIR", values.join(":"));
    }
    command
}

fn gsettings(args: &[&str]) -> io::Result<String> {
    let output = gsettings_command().args(args).output()?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned());
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(io::Error::other(if message.is_empty() {
        "GSettings operation failed".into()
    } else {
        message
    }))
}

pub(crate) fn load() -> io::Result<Settings> {
    load_with(gsettings)
}

fn load_with(mut get: impl FnMut(&[&str]) -> io::Result<String>) -> io::Result<Settings> {
    let gsettings = &mut get;
    let enabled = parse_gvariant_bool(&gsettings(&["get", SCHEMA, "enabled"])?)
        .ok_or_else(|| io::Error::other("GSettings returned an invalid enabled value"))?;
    let show_tray_icon = parse_gvariant_bool(&gsettings(&["get", SCHEMA, "show-tray-icon"])?)
        .ok_or_else(|| io::Error::other("GSettings returned an invalid show-tray-icon value"))?;
    let keybinding = parse_gvariant_string_array(&gsettings(&["get", SCHEMA, "keybinding"])?)
        .into_iter()
        .next()
        .unwrap_or_else(|| DEFAULT_KEYBINDING.into());
    let language = parse_gvariant_string(&gsettings(&["get", SCHEMA, "language"])?)
        .and_then(|v| normalize_language(&v))
        .unwrap_or_else(|| "auto".into());
    let language_override = env::var("CODEX_VOICE_LANG")
        .ok()
        .and_then(|v| normalize_language(&v));
    Ok(Settings {
        enabled,
        show_tray_icon,
        keybinding,
        language,
        language_override,
    })
}

fn parse_gvariant_bool(value: &str) -> Option<bool> {
    match value.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

pub(crate) fn set(key: &str, value: &str) -> io::Result<()> {
    match key {
        "enabled" | "show-tray-icon" => {
            if value != "true" && value != "false" {
                return Err(invalid_input("boolean setting must be true or false"));
            }
            gsettings(&["set", SCHEMA, key, value])?;
        }
        "keybinding" => {
            let accelerator = normalize_accelerator(value)
                .ok_or_else(|| invalid_input("invalid GNOME accelerator"))?;
            let escaped = accelerator.replace('\\', "\\\\").replace('\'', "\\'");
            gsettings(&["set", SCHEMA, key, &format!("['{escaped}']")])?;
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

pub(crate) fn reset() -> io::Result<()> {
    gsettings(&["reset-recursively", SCHEMA]).map(|_| ())
}

pub(crate) fn extension_is_active() -> io::Result<bool> {
    let output = Command::new("gnome-extensions")
        .args(["info", EXTENSION_UUID])
        .output()?;
    if !output.status.success() {
        return Ok(false);
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|line| line.trim().eq_ignore_ascii_case("State: ACTIVE")))
}

fn parse_gvariant_string(value: &str) -> Option<String> {
    value
        .trim()
        .strip_prefix('\'')
        .and_then(|v| v.strip_suffix('\''))
        .map(|v| v.replace("\\'", "'").replace("\\\\", "\\"))
}

fn parse_gvariant_string_array(value: &str) -> Vec<String> {
    value
        .trim()
        .strip_prefix('[')
        .and_then(|v| v.strip_suffix(']'))
        .unwrap_or("")
        .split(',')
        .filter_map(parse_gvariant_string)
        .collect()
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

pub(crate) fn transcriber_language_args(language: &str) -> Vec<String> {
    if language == "auto" {
        Vec::new()
    } else {
        vec!["--language".into(), language.into()]
    }
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
        && key[1..].parse::<u8>().is_ok_and(|n| (1..=35).contains(&n));
    let normal_key =
        key.chars().count() == 1 && key.chars().all(|c| c.is_ascii_alphanumeric() || c == ' ');
    ((modifiers.contains("<Control>")
        || modifiers.contains("<Alt>")
        || modifiers.contains("<Super>")
        || supported_function)
        && (normal_key || supported_function || key == "space"))
        .then(|| format!("{modifiers}{}", if key == " " { "space" } else { key }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_languages_and_legacy_empty_values() {
        assert_eq!(normalize_language(""), Some("auto".into()));
        assert_eq!(normalize_language("EN-US"), Some("en-us".into()));
        assert_eq!(normalize_language("en_US"), None);
    }

    #[test]
    fn parses_gsettings_boole_strictly() {
        assert_eq!(parse_gvariant_bool("true"), Some(true));
        assert_eq!(parse_gvariant_bool("false"), Some(false));
        assert_eq!(parse_gvariant_bool("not-a-bool"), None);
    }

    #[test]
    fn propagates_gsettings_read_failures() {
        let error = load_with(|_| Err(io::Error::other("read failed"))).unwrap_err();
        assert_eq!(error.to_string(), "read failed");
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
    fn effective_language_and_asr_arguments_agree() {
        let settings = Settings {
            enabled: true,
            show_tray_icon: true,
            keybinding: DEFAULT_KEYBINDING.into(),
            language: "auto".into(),
            language_override: None,
        };
        assert_eq!(settings.effective_language(), "auto");
        assert!(transcriber_language_args(&settings.effective_language()).is_empty());
        assert_eq!(transcriber_language_args("es"), vec!["--language", "es"]);
    }
}
