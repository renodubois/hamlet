//! Tracing / logging setup. Called once from `main`.

use tracing_subscriber::{EnvFilter, fmt};

/// Install a `tracing` subscriber that respects the same `RUST_LOG` syntax
/// `env_logger` did. Call this once at process start; subsequent calls are
/// ignored.
pub fn init(default_filter: &str) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    // `try_init` so tests that call into this twice don't panic.
    let _ = fmt().with_env_filter(filter).with_target(true).try_init();
}
