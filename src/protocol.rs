use serde::Serialize;

pub const SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRuntimeState {
    pub state: String,
    pub owner_pid: i32,
    pub owner_start_time: u64,
}

pub fn parse_active_runtime_state(text: &str) -> Option<ActiveRuntimeState> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let state = value.get("state")?.as_str()?;
    let owner_pid = i32::try_from(value.get("ownerPid")?.as_i64()?).ok()?;
    let owner_start_time = value.get("ownerStartTime")?.as_u64()?;
    (value.get("schemaVersion")?.as_u64()? == u64::from(SCHEMA_VERSION)
        && matches!(state, "recording" | "transcribing" | "typing")
        && owner_pid > 0
        && owner_start_time > 0
        && value.get("startedAt")?.as_u64().is_some())
    .then(|| ActiveRuntimeState {
        state: state.to_owned(),
        owner_pid,
        owner_start_time,
    })
}

pub fn serialize<T: Serialize>(document: &T) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_runtime_fixtures_define_v1_compatibility() {
        assert_eq!(
            parse_active_runtime_state(include_str!(
                "../tests/fixtures/protocol/runtime-valid.json"
            ))
            .unwrap()
            .state,
            "recording"
        );
        for fixture in [
            include_str!("../tests/fixtures/protocol/runtime-unknown-state.json"),
            include_str!("../tests/fixtures/protocol/runtime-malformed.json"),
            include_str!("../tests/fixtures/protocol/runtime-unsupported-version.json"),
            include_str!("../tests/fixtures/protocol/runtime-missing-owner-start-time.json"),
        ] {
            assert!(parse_active_runtime_state(fixture).is_none());
        }
    }
}
