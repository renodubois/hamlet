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
fn production_compose_uses_loopback_binding_and_env_driven_livekit_settings() {
    let compose = read_server_file("docker-compose.yml");

    assert!(
        compose.contains("\"127.0.0.1:${HAMLET_SERVER_PORT:-3030}:3030\""),
        "production compose should only publish the API on loopback by default"
    );
    for expected in [
        "HAMLET_ACCOUNT_REGISTRATION_ENABLED: \"${HAMLET_ACCOUNT_REGISTRATION_ENABLED:-false}\"",
        "HAMLET_ALLOWED_ORIGINS: \"${HAMLET_ALLOWED_ORIGINS:-}\"",
        "HAMLET_COOKIE_SECURE: \"${HAMLET_COOKIE_SECURE:-}\"",
        "HAMLET_COOKIE_SAME_SITE: \"${HAMLET_COOKIE_SAME_SITE:-}\"",
        "HAMLET_SENTRY_DSN: \"${HAMLET_SENTRY_DSN:-}\"",
        "LIVEKIT_URL: \"${LIVEKIT_URL:-}\"",
        "LIVEKIT_API_KEY: \"${LIVEKIT_API_KEY:-}\"",
        "LIVEKIT_API_SECRET: \"${LIVEKIT_API_SECRET:-}\"",
    ] {
        assert!(
            compose.contains(expected),
            "production compose should contain {expected}"
        );
    }
    assert!(
        !compose.contains("devkey") && !compose.contains("devsecretdevsecretdevsecretdevsecret"),
        "production compose must not bake in development LiveKit credentials"
    );
    assert!(
        !compose.contains("depends_on:") && !compose.contains("  livekit:"),
        "production compose should not require LiveKit unless an optional compose file is used"
    );
}

#[test]
fn optional_production_livekit_compose_is_separate_from_base_compose() {
    let compose = read_server_file("docker-compose.livekit.yml");

    assert!(
        compose.contains("  livekit:"),
        "optional production LiveKit compose should define the LiveKit service"
    );
    assert!(
        compose.contains("source: ${HAMLET_LIVEKIT_CONFIG:-./livekit.prod.yaml}"),
        "optional production LiveKit compose should default to a production config file"
    );
    assert!(
        compose.contains("depends_on:") && compose.contains("- livekit"),
        "optional production LiveKit compose should start LiveKit with the server"
    );
    assert!(
        !compose.contains("devkey") && !compose.contains("devsecretdevsecretdevsecretdevsecret"),
        "optional production LiveKit compose must not bake in development credentials"
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
        override_compose.contains("LIVEKIT_URL: \"${LIVEKIT_URL:-ws://127.0.0.1:7880}\""),
        "development compose should keep local LiveKit URL defaults in the dev-only override"
    );
    assert!(
        override_compose.contains("LIVEKIT_API_KEY: \"${LIVEKIT_API_KEY:-devkey}\"")
            && override_compose.contains(
                "LIVEKIT_API_SECRET: \"${LIVEKIT_API_SECRET:-devsecretdevsecretdevsecretdevsecret}\"",
            ),
        "development compose should keep dev LiveKit credentials only in the dev override"
    );
    assert!(
        override_compose.contains("  livekit:"),
        "development compose should keep starting the local LiveKit service by default"
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
    assert!(
        dockerfile.contains("cargo build --release --locked --bin hamlet --bin hamlet-admin"),
        "production Dockerfile should build both deployment binaries"
    );
    assert!(
        dockerfile.contains("/app/target/release/hamlet-admin /usr/local/bin/hamlet-admin"),
        "production Dockerfile should copy hamlet-admin into the runtime image"
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
        "HAMLET_ALLOWED_ORIGINS=",
        "HAMLET_COOKIE_SECURE=false",
        "HAMLET_COOKIE_SAME_SITE=lax",
        "HAMLET_ACCOUNT_REGISTRATION_ENABLED=false",
        "HAMLET_SENTRY_DSN=",
        "HAMLET_SEED_DEV_DATA",
        "LIVEKIT_URL=",
        "LIVEKIT_API_KEY=",
        "LIVEKIT_API_SECRET=",
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
