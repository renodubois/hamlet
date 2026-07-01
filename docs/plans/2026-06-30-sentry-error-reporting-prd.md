# Sentry Error Reporting PRD

## Problem Statement

Hamlet operators currently rely on local server logs to notice and diagnose internal failures. Logs remain essential, but they can be missed when the server runs unattended or when operators need centralized error aggregation. The application needs optional error reporting that can be enabled through runtime configuration without making Sentry credentials mandatory for local development, CI, or tests.

## Solution

Add optional server-side Sentry error reporting controlled by the server configuration. When a Sentry DSN is configured, the server initializes the Sentry SDK at startup and reports server error events from the existing tracing/logging path. Existing stdout/stderr logs continue to be emitted exactly as before. When no DSN is configured, the server behaves as it does today with no Sentry dependency at runtime and no network calls to Sentry.

## User Stories

1. As a Hamlet operator, I want to configure a Sentry DSN for the server, so that production server errors are reported centrally.
2. As a Hamlet operator, I want Sentry to be disabled by default, so that local development and private deployments do not send telemetry accidentally.
3. As a Hamlet operator, I want existing logs to continue when Sentry is enabled, so that local diagnostics and log collection still work.
4. As a Hamlet operator, I want internal server errors to be reported to Sentry, so that I can triage failures without watching the console live.
5. As a Hamlet operator, I want startup to clearly indicate whether Sentry reporting is enabled or disabled, so that I can validate deployment configuration quickly.
6. As a Hamlet developer, I want Sentry configuration to flow through the existing server configuration object, so that runtime configuration remains centralized.
7. As a Hamlet developer, I want blank Sentry DSN configuration to be treated the same as missing configuration, so that worktree and local `.env` files can leave the value empty safely.
8. As a Hamlet developer, I want tests to exercise Sentry configuration without contacting Sentry, so that CI remains hermetic.
9. As a Hamlet developer, I want error reporting to use the existing tracing events, so that handler code does not need duplicate report-and-log calls.
10. As a privacy-conscious deployer, I want Sentry to be opt-in only, so that no data leaves the server unless I explicitly configure a DSN.
11. As a maintainer, I want invalid or unusable Sentry configuration to avoid replacing normal logs, so that misconfiguration does not make failures invisible.
12. As a maintainer, I want this feature to stay server-scoped for now, so that renderer/client reporting can be considered separately with its own privacy and release decisions.

## Implementation Decisions

- Build this as a server-side telemetry feature because the current architecture centralizes backend errors through Rust `tracing`, while the Electron renderer has separate privacy and packaging considerations.
- Extend the server configuration with an optional Sentry DSN value loaded by the existing configuration mechanism.
- Treat missing and empty Sentry DSN values as disabled error reporting.
- Initialize the Sentry SDK during server startup when a valid DSN is present, and retain the SDK guard for the server lifetime so queued events can flush on shutdown.
- Add a Sentry tracing layer alongside the existing formatted logging layer. This makes Sentry additive rather than replacing logs.
- Report error-level tracing events to Sentry. Existing internal error paths already emit error-level events before returning sanitized responses.
- Keep performance tracing, client-side reporting, release distribution, and user-identifying context out of scope unless a later PRD explicitly adds them.
- Avoid requiring Sentry credentials or live network access in tests.

## Testing Decisions

- Good tests should verify externally visible configuration behavior and startup wiring decisions without depending on Sentry's network service.
- Unit tests should cover optional DSN parsing from configuration, including missing, empty, and non-empty values.
- Telemetry tests, if added, should avoid installing a global tracing subscriber repeatedly in a way that flakes across the Rust test process.
- Existing server checks remain the prior art: `cargo fmt`, `cargo clippy -- -D warnings`, and `cargo test`.

## Out of Scope

- Client/renderer Sentry reporting.
- Electron main/preload Sentry reporting.
- Capturing authenticated user identity, request bodies, cookies, or other PII in Sentry events.
- Sentry performance tracing, distributed tracing, logs product ingestion, metrics, release health dashboards, or source map upload automation.
- Adding a disk-backed config file format beyond the server's existing runtime configuration mechanism.

## Further Notes

The feature should preserve the current logging stack for all deployments. Sentry is an optional secondary sink for errors, enabled only by setting the configured DSN value. Tests should not need real Sentry credentials, should not send events over the network, and should remain deterministic in local worktrees and CI.
