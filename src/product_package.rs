use std::env;
use std::io;
use std::path::PathBuf;
use std::process::Command;

const SYSTEM_SHARE: &str = "/usr/share/codex-voice";

struct Layout {
    source_root: Option<PathBuf>,
    executable_dir: Option<PathBuf>,
    user_share: Option<PathBuf>,
}

impl Layout {
    fn from_environment() -> Self {
        Self {
            source_root: env::current_dir().ok(),
            executable_dir: env::current_exe()
                .ok()
                .and_then(|executable| executable.parent().map(PathBuf::from)),
            user_share: home_dir().map(|home| home.join(".local/share/codex-voice")),
        }
    }

    fn overlay_candidates(&self) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(dir) = &self.executable_dir {
            candidates.extend([
                dir.join("../share/codex-voice/overlay.py"),
                dir.join("../lib/codex-voice/overlay.py"),
                dir.join("../src/overlay.py"),
                dir.join("../../src/overlay.py"),
            ]);
        }
        if let Some(root) = &self.source_root {
            candidates.push(root.join("src/overlay.py"));
        }
        if let Some(share) = &self.user_share {
            candidates.push(share.join("overlay.py"));
        }
        candidates.push(PathBuf::from(SYSTEM_SHARE).join("overlay.py"));
        candidates
    }

    fn schema_directories(&self) -> Vec<PathBuf> {
        let mut directories = Vec::new();
        if let Some(share) = &self.user_share {
            directories.push(share.join("schemas"));
        }
        directories.push(PathBuf::from("/usr/share/glib-2.0/schemas"));
        directories
    }
}

fn override_file(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .map(PathBuf::from)
        .filter(|path| path.is_file())
}

pub(crate) fn launch_settings() -> io::Result<u8> {
    Command::new(settings_binary()?).spawn()?;
    Ok(0)
}

pub(crate) fn settings_binary() -> io::Result<PathBuf> {
    env::var_os("CODEX_VOICE_SETTINGS_BIN")
        .map(PathBuf::from)
        .or_else(|| {
            [
                PathBuf::from("/usr/bin/codex-voice-settings"),
                home_dir()?.join(".local/bin/codex-voice-settings"),
            ]
            .into_iter()
            .find(|path| path.is_file())
        })
        .filter(|path| path.is_file())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "codex-voice-settings is not installed",
            )
        })
}

pub(crate) fn overlay_script() -> Option<PathBuf> {
    override_file("CODEX_VOICE_OVERLAY").or_else(|| {
        Layout::from_environment()
            .overlay_candidates()
            .into_iter()
            .find(|path| path.is_file())
    })
}

pub(crate) fn schema_directories() -> Vec<PathBuf> {
    Layout::from_environment()
        .schema_directories()
        .into_iter()
        .filter(|path| path.is_dir())
        .collect()
}

pub(crate) fn overlay_backend() -> Option<String> {
    env::var("CODEX_VOICE_OVERLAY_BACKEND")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            env::var("CODEX_VOICE_GDK_BACKEND")
                .ok()
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            env::var_os("DISPLAY")
                .filter(|value| !value.is_empty())
                .map(|_| "x11".into())
        })
}

pub(crate) fn command(name: &str) -> Option<PathBuf> {
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
    if name == "codex-asr" {
        return [
            Some(PathBuf::from("/usr/lib/codex-voice/codex-asr")),
            home_dir().map(|home| home.join(".cargo/bin/codex-asr")),
        ]
        .into_iter()
        .flatten()
        .find(|path| path.is_file());
    }
    None
}

pub(crate) fn home_dir() -> Option<PathBuf> {
    env::var_os("HOME").map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        Layout {
            source_root: Some(PathBuf::from("/work/codex-voice")),
            executable_dir: Some(PathBuf::from("/opt/Codex Voice")),
            user_share: Some(PathBuf::from("/home/user/.local/share/codex-voice")),
        }
    }

    #[test]
    fn overlay_layout_covers_installed_user_and_development_forms() {
        let candidates = layout().overlay_candidates();
        assert!(candidates.contains(&PathBuf::from("/usr/share/codex-voice/overlay.py")));
        assert!(candidates.contains(&PathBuf::from(
            "/home/user/.local/share/codex-voice/overlay.py"
        )));
        assert!(candidates.contains(&PathBuf::from("/work/codex-voice/src/overlay.py")));
        assert!(candidates.contains(&PathBuf::from(
            "/opt/Codex Voice/../share/codex-voice/overlay.py"
        )));
    }

    #[test]
    fn schema_resources_cover_user_and_system_layouts() {
        assert_eq!(
            layout().schema_directories(),
            vec![
                PathBuf::from("/home/user/.local/share/codex-voice/schemas"),
                PathBuf::from("/usr/share/glib-2.0/schemas"),
            ]
        );
    }
}
