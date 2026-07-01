//! Tracing / logging setup. Called once from `main`.

use sentry::integrations::tracing::EventFilter;
use tracing::Level;
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

/// Holds optional telemetry resources that must stay alive for the process
/// lifetime.
pub struct TelemetryGuard {
    sentry: Option<sentry::ClientInitGuard>,
}

impl TelemetryGuard {
    /// Whether Sentry error reporting is currently enabled.
    pub fn sentry_enabled(&self) -> bool {
        self.sentry.as_ref().is_some_and(|guard| guard.is_enabled())
    }
}

enum PreparedSentry {
    Enabled(sentry::ClientInitGuard),
    Disabled,
    InvalidDsn(String),
}

impl PreparedSentry {
    fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled(guard) if guard.is_enabled())
    }

    fn into_guard(self) -> TelemetryGuard {
        let sentry = match self {
            Self::Enabled(guard) => Some(guard),
            Self::Disabled | Self::InvalidDsn(_) => None,
        };

        TelemetryGuard { sentry }
    }
}

/// Install a `tracing` subscriber that respects the same `RUST_LOG` syntax
/// `env_logger` did. Call this once at process start; subsequent calls are
/// ignored.
///
/// When `sentry_dsn` is set to a valid Sentry DSN, error-level tracing events
/// are sent to Sentry in addition to the normal formatted logs.
pub fn init(default_filter: &str, sentry_dsn: Option<&str>) -> TelemetryGuard {
    let sentry = prepare_sentry(sentry_dsn);
    let sentry_enabled = sentry.is_enabled();

    init_subscriber(default_filter, sentry_enabled);

    match &sentry {
        PreparedSentry::Enabled(_) => tracing::info!("Sentry error reporting enabled"),
        PreparedSentry::Disabled => tracing::info!("Sentry error reporting disabled"),
        PreparedSentry::InvalidDsn(error) => tracing::warn!(
            error = %error,
            "invalid Sentry DSN; Sentry error reporting disabled"
        ),
    }

    sentry.into_guard()
}

fn prepare_sentry(sentry_dsn: Option<&str>) -> PreparedSentry {
    let Some(dsn) = sentry_dsn.map(str::trim).filter(|value| !value.is_empty()) else {
        return PreparedSentry::Disabled;
    };

    match dsn.parse::<sentry::types::Dsn>() {
        Ok(dsn) => {
            let guard = sentry::init(sentry::ClientOptions {
                dsn: Some(dsn),
                release: sentry::release_name!(),
                ..Default::default()
            });

            if guard.is_enabled() {
                PreparedSentry::Enabled(guard)
            } else {
                PreparedSentry::Disabled
            }
        }
        Err(error) => PreparedSentry::InvalidDsn(error.to_string()),
    }
}

fn init_subscriber(default_filter: &str, sentry_enabled: bool) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));
    let fmt_layer = fmt::layer().with_target(true);

    // `try_init` so tests that call into this twice don't panic.
    if sentry_enabled {
        let sentry_layer = sentry::integrations::tracing::layer()
            .event_filter(|metadata| match *metadata.level() {
                Level::ERROR => EventFilter::Event,
                _ => EventFilter::Ignore,
            })
            .span_filter(|_| false);

        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .with(sentry_layer)
            .try_init();
    } else {
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(fmt_layer)
            .try_init();
    }
}
