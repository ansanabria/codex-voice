use crate::process_identity::ProcessIdentity;
use crate::{protocol, remove, write_atomic, Paths};
use serde::Serialize;
use std::fs;
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum State {
    Recording,
    Transcribing,
    Typing,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Document {
    schema_version: u8,
    state: State,
    owner_pid: i32,
    owner_start_time: u64,
    started_at: u128,
}

pub(crate) fn publish(paths: &Paths, state: State, owner: ProcessIdentity) -> io::Result<()> {
    let document = Document {
        schema_version: protocol::SCHEMA_VERSION,
        state,
        owner_pid: owner.pid,
        owner_start_time: owner.start_time,
        started_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    };
    let json = serde_json::to_vec(&document).expect("runtime state is serializable");
    write_atomic(&paths.runtime_state, &json)
}

// Callers hold the state lock, so a stale read cannot race a newer publish
// between the content comparison and unlink.
pub(crate) fn read(paths: &Paths) -> Option<String> {
    let contents = fs::read(&paths.runtime_state).ok()?;
    let active = std::str::from_utf8(&contents)
        .ok()
        .and_then(protocol::parse_active_runtime_state)
        .filter(|document| {
            ProcessIdentity {
                pid: document.owner_pid,
                start_time: document.owner_start_time,
            }
            .is_alive()
        });
    if let Some(document) = active {
        return Some(document.state);
    }
    if fs::read(&paths.runtime_state).is_ok_and(|current| current == contents) {
        remove(&paths.runtime_state);
    }
    None
}

pub(crate) fn cleanup_stale(paths: &Paths) {
    let _ = read(paths);
}

pub(crate) fn clear_if_owner(paths: &Paths, owner: ProcessIdentity) {
    let Ok(contents) = fs::read(&paths.runtime_state) else {
        return;
    };
    let belongs_to_owner = std::str::from_utf8(&contents)
        .ok()
        .and_then(protocol::parse_active_runtime_state)
        .is_some_and(|document| {
            document.owner_pid == owner.pid && document.owner_start_time == owner.start_time
        });
    if belongs_to_owner && fs::read(&paths.runtime_state).is_ok_and(|current| current == contents) {
        remove(&paths.runtime_state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StateLock;
    use std::path::PathBuf;

    #[test]
    fn document_has_protocol_identity_fields() {
        let value = serde_json::to_value(Document {
            schema_version: 1,
            state: State::Recording,
            owner_pid: 42,
            owner_start_time: 99,
            started_at: 1,
        })
        .unwrap();
        assert_eq!(value["state"], "recording");
        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["ownerPid"], 42);
        assert_eq!(value["ownerStartTime"], 99);
    }

    #[test]
    fn publish_and_stale_cleanup_verify_full_owner_identity() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tmp")
            .join(format!(
                "codex-voice-runtime-state-test-{}",
                std::process::id()
            ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let paths = Paths::in_directory(PathBuf::from(&directory));
        let owner = ProcessIdentity::current().unwrap();
        let _lock = StateLock::acquire(&paths).unwrap();

        publish(&paths, State::Recording, owner).unwrap();
        let contents = fs::read_to_string(&paths.runtime_state).unwrap();
        let parsed = protocol::parse_active_runtime_state(&contents).unwrap();
        assert_eq!(parsed.owner_pid, owner.pid);
        assert_eq!(parsed.owner_start_time, owner.start_time);
        assert_eq!(read(&paths).as_deref(), Some("recording"));

        let stale = ProcessIdentity {
            start_time: owner.start_time + 1,
            ..owner
        };
        publish(&paths, State::Transcribing, stale).unwrap();
        assert_eq!(read(&paths), None);
        assert!(!paths.runtime_state.exists());
        drop(_lock);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn owner_cleanup_does_not_remove_replacement_state() {
        let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tmp")
            .join(format!(
                "codex-voice-runtime-owner-test-{}",
                std::process::id()
            ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        let paths = Paths::in_directory(PathBuf::from(&directory));
        let owner = ProcessIdentity::current().unwrap();
        let replacement = ProcessIdentity {
            start_time: owner.start_time + 1,
            ..owner
        };
        publish(&paths, State::Recording, replacement).unwrap();
        clear_if_owner(&paths, owner);
        assert!(paths.runtime_state.exists());
        fs::remove_dir_all(directory).unwrap();
    }
}
