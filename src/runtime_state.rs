use crate::{process_exists, protocol, remove, write_atomic, Paths};
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
    owner_pid: u32,
    started_at: u128,
}

pub(crate) fn publish(paths: &Paths, state: State, owner_pid: u32) -> io::Result<()> {
    let document = Document {
        schema_version: protocol::SCHEMA_VERSION,
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

pub(crate) fn read(paths: &Paths) -> Option<String> {
    let result = (|| {
        let contents = fs::read_to_string(&paths.runtime_state).ok()?;
        let document = protocol::parse_active_runtime_state(&contents)?;
        process_exists(document.owner_pid).then_some(document.state)
    })();
    if result.is_none() && paths.runtime_state.exists() {
        clear(paths);
    }
    result
}

pub(crate) fn cleanup_stale(paths: &Paths) {
    let _ = read(paths);
}

pub(crate) fn clear(paths: &Paths) {
    remove(&paths.runtime_state);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_has_protocol_fields() {
        let value = serde_json::to_value(Document {
            schema_version: 1,
            state: State::Recording,
            owner_pid: 42,
            started_at: 1,
        })
        .unwrap();
        assert_eq!(value["state"], "recording");
        assert_eq!(value["schemaVersion"], 1);
    }
}
