use hamlet_client_iced::storage::{FileStorage, Preferences, Storage, VoiceDevicePreferences};
use tempfile::tempdir;

#[test]
fn file_storage_round_trips_server_url_preference() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let storage = FileStorage::at_path(dir.path().join("preferences.json"));
    let preferences = Preferences::with_server_url("https://hamlet.example.test/")?;

    storage.save_preferences(&preferences)?;

    assert_eq!(
        storage.load_preferences()?,
        Preferences::with_server_url("https://hamlet.example.test")?
    );

    Ok(())
}

#[test]
fn missing_preference_file_loads_defaults() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let storage = FileStorage::at_path(dir.path().join("missing.json"));

    assert_eq!(storage.load_preferences()?, Preferences::default());

    Ok(())
}

#[test]
fn file_storage_round_trips_session_token() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let storage = FileStorage::at_path(dir.path().join("preferences.json"));
    let preferences = Preferences::with_server_url_and_session_token(
        "https://hamlet.example.test/",
        Some("session-token".to_string()),
    )?;

    storage.save_preferences(&preferences)?;

    assert_eq!(
        storage.load_preferences()?,
        Preferences::with_server_url_and_session_token(
            "https://hamlet.example.test",
            Some("session-token".to_string())
        )?
    );

    Ok(())
}

#[test]
fn file_storage_round_trips_voice_device_preferences() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let storage = FileStorage::at_path(dir.path().join("preferences.json"));
    let preferences = Preferences::with_server_url_session_token_and_voice(
        "https://hamlet.example.test/",
        Some("session-token".to_string()),
        VoiceDevicePreferences::new(
            Some(" mic-id ".to_string()),
            Some(" output-id ".to_string()),
        ),
    )?;

    storage.save_preferences(&preferences)?;

    assert_eq!(
        storage.load_preferences()?,
        Preferences::with_server_url_session_token_and_voice(
            "https://hamlet.example.test",
            Some("session-token".to_string()),
            VoiceDevicePreferences::new(Some("mic-id".to_string()), Some("output-id".to_string()),),
        )?
    );

    Ok(())
}
