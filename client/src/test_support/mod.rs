//! Crate-private fixtures shared across feature suites; never compiled into the client.
pub(crate) mod storage;

pub(crate) fn session_fixtures() -> serde_json::Value {
    serde_json::from_str(include_str!("fixtures/session.json")).unwrap()
}
