#![allow(clippy::expect_used)]

use std::fs;
use std::path::Path;

#[test]
fn production_compose_uses_one_application_data_volume_for_sqlite_and_uploads() {
    let compose = read_server_file("docker-compose.yml");

    assert!(
        compose.contains("HAMLET_DATA_DIR: /var/lib/hamlet"),
        "production compose should point Hamlet's data directory at the mounted app-data volume"
    );
    assert!(
        compose.contains("HAMLET_CONFIG_FILE: /var/lib/hamlet/server-config.json"),
        "production compose should keep the editable server config in the app-data volume"
    );
    assert!(
        compose.contains("HAMLET_DATABASE_URL: \"sqlite:///var/lib/hamlet/hamlet.db?mode=rwc\""),
        "production compose should set an explicit file-backed SQLite URL inside the data dir"
    );
    assert!(
        compose.contains("HAMLET_UPLOADS_DIR: /var/lib/hamlet/uploads"),
        "public uploads should live under the same app-data volume as the database"
    );
    assert!(
        compose.contains(
            "HAMLET_MESSAGE_ATTACHMENTS_DIR: /var/lib/hamlet/private-uploads/message-attachments"
        ),
        "private uploads should live under the same app-data volume as the database"
    );
    assert!(
        compose.contains("HAMLET_BOOTSTRAP_DEFAULT_CHANNELS: \"true\""),
        "production compose should document the default channel bootstrap setting"
    );
    assert!(
        compose.contains("HAMLET_SEED_DEV_DATA: \"false\""),
        "production compose should keep release-shaped seed defaults"
    );
    assert!(
        compose.contains("- hamlet_data:/var/lib/hamlet"),
        "the app-data directory must be backed by a named volume"
    );
    assert!(
        compose.contains("  hamlet_data:"),
        "the app-data named volume must be declared"
    );
    assert!(
        !compose.contains("hamlet_uploads") && !compose.contains("hamlet_private_uploads"),
        "database and uploads should reset together through one application data volume"
    );
}

#[test]
fn development_compose_explicitly_enables_seed_data_without_removing_hot_reload() {
    let override_compose = read_server_file("docker-compose.override.yml");

    assert!(
        override_compose.contains("dockerfile: Dockerfile.dev"),
        "development compose should keep using the hot-reload dev image"
    );
    assert!(
        override_compose.contains("HAMLET_SEED_DEV_DATA: \"true\""),
        "development compose should opt into dev seed data explicitly"
    );
    assert!(
        override_compose.contains("- .:/app"),
        "development compose should keep the source bind mount for cargo-watch hot reload"
    );
    assert!(
        override_compose.contains("- hamlet_data:/var/lib/hamlet"),
        "development compose should keep the app-data volume outside the hot-reload bind mount"
    );
    assert!(
        override_compose.contains("- hamlet_target:/app/target"),
        "development compose should keep the incremental target cache volume"
    );
}

#[test]
fn runtime_dockerfile_prepares_owned_data_and_upload_directories_before_user_switch() {
    let dockerfile = read_server_file("Dockerfile");
    let directory_setup = dockerfile
        .find("RUN mkdir -p")
        .expect("Dockerfile should create runtime directories");
    let user_switch = dockerfile
        .find("USER hamlet")
        .expect("Dockerfile should run the app as the hamlet user");

    assert!(
        directory_setup < user_switch,
        "runtime directories must be created before switching to the non-root app user"
    );
    for directory in [
        "/var/lib/hamlet",
        "/var/lib/hamlet/uploads",
        "/var/lib/hamlet/private-uploads/message-attachments",
        "/app/uploads",
        "/app/private-uploads/message-attachments",
    ] {
        assert!(
            dockerfile.contains(directory),
            "Dockerfile should create {directory}"
        );
    }
    assert!(
        dockerfile.contains("chown -R hamlet:hamlet /app /var/lib/hamlet"),
        "runtime directories should be owned by the non-root app user"
    );
}

#[test]
fn development_dockerfile_uses_the_same_application_data_layout() {
    let dockerfile = read_server_file("Dockerfile.dev");

    for expected in [
        "/var/lib/hamlet",
        "/var/lib/hamlet/uploads",
        "/var/lib/hamlet/private-uploads/message-attachments",
        "HAMLET_DATA_DIR=/var/lib/hamlet",
        "HAMLET_UPLOADS_DIR=/var/lib/hamlet/uploads",
        "HAMLET_MESSAGE_ATTACHMENTS_DIR=/var/lib/hamlet/private-uploads/message-attachments",
    ] {
        assert!(
            dockerfile.contains(expected),
            "Dockerfile.dev should contain {expected}"
        );
    }
}

#[test]
fn docker_build_context_excludes_local_sqlite_and_data_artifacts() {
    let dockerignore = read_server_file(".dockerignore");

    for pattern in [
        "data/",
        ".hamlet-data/",
        "hamlet-data/",
        ".env",
        ".env.*",
        "!.env.example",
        "livekit.local*.yaml",
        "*.db",
        "*.db-*",
        "*.sqlite",
        "*.sqlite-*",
        "*.sqlite3",
        "*.sqlite3-*",
    ] {
        assert!(
            dockerignore_line_exists(&dockerignore, pattern),
            ".dockerignore should include {pattern:?}"
        );
    }
}

#[test]
fn server_env_example_documents_persistence_controls() {
    let env_example = read_server_file(".env.example");

    for expected in [
        "HAMLET_DATA_DIR",
        "HAMLET_DATABASE_URL=sqlite://data/hamlet.db?mode=rwc",
        "HAMLET_DATABASE_URL=sqlite:file:hamlet_clean_room?mode=memory&cache=shared",
        "HAMLET_BOOTSTRAP_DEFAULT_CHANNELS=true",
        "HAMLET_CONFIG_FILE",
        "HAMLET_ACCOUNT_REGISTRATION_ENABLED",
        "HAMLET_SEED_DEV_DATA",
    ] {
        assert!(
            env_example.contains(expected),
            ".env.example should document {expected}"
        );
    }
}

#[test]
fn gitignore_excludes_local_sqlite_and_data_artifacts() {
    let server_gitignore = read_server_file(".gitignore");
    let root_gitignore = read_repo_file(".gitignore");

    for pattern in [
        "data/",
        ".hamlet-data/",
        "hamlet-data/",
        "*.db",
        "*.db-*",
        "*.sqlite",
        "*.sqlite-*",
        "*.sqlite3",
        "*.sqlite3-*",
    ] {
        assert!(
            gitignore_line_exists(&server_gitignore, pattern),
            "server .gitignore should include {pattern:?}"
        );
        assert!(
            gitignore_line_exists(&root_gitignore, pattern),
            "root .gitignore should include {pattern:?}"
        );
    }
}

fn read_server_file(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .expect("server file should be readable")
}

fn read_repo_file(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(path))
        .expect("repo file should be readable")
}

fn dockerignore_line_exists(contents: &str, pattern: &str) -> bool {
    line_exists(contents, pattern)
}

fn gitignore_line_exists(contents: &str, pattern: &str) -> bool {
    line_exists(contents, pattern) || line_exists(contents, &format!("/{pattern}"))
}

fn line_exists(contents: &str, pattern: &str) -> bool {
    contents.lines().any(|line| line.trim() == pattern)
}
