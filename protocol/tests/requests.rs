use hamlet_protocol::{CreateChannel, CreateMessage, Credentials, RenameChannel};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn assert_request_contract<T: DeserializeOwned + Serialize>(wire: Value) {
    let decoded: T = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(decoded).unwrap(), wire);

    let mut extra = wire.clone();
    extra["unexpected"] = json!(true);
    assert!(serde_json::from_value::<T>(extra).is_err());

    for key in wire.as_object().unwrap().keys() {
        let mut missing = wire.clone();
        missing.as_object_mut().unwrap().remove(key);
        assert!(serde_json::from_value::<T>(missing).is_err());
        let mut wrong_type = wire.clone();
        wrong_type[key] = json!(42);
        assert!(serde_json::from_value::<T>(wrong_type).is_err());
    }
}

#[test]
fn credentials_preserve_the_strict_request_contract() {
    assert_request_contract::<Credentials>(json!({
        "username": "Alice", "password": "test password"
    }));
}

#[test]
fn channel_creation_preserves_the_strict_request_contract() {
    assert_request_contract::<CreateChannel>(json!({"name": "general", "type": "text"}));
    assert!(
        serde_json::from_value::<CreateChannel>(json!({
            "name": "general", "type": "voice"
        }))
        .is_err()
    );
}

#[test]
fn channel_rename_preserves_the_strict_name_only_contract() {
    assert_request_contract::<RenameChannel>(json!({"name": "new name"}));
    assert!(serde_json::from_value::<RenameChannel>(json!({"name":"new", "type":"text"})).is_err());
}

#[test]
fn message_creation_preserves_the_strict_request_contract() {
    assert_request_contract::<CreateMessage>(json!({"text": "Hello 世界\nsecond line"}));
}
