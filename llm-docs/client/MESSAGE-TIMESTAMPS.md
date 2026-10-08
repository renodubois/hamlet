# Relative message timestamps

## Agreed behavior

- Message timestamps use calendar boundaries in the viewer's local timezone.
- Messages from today show elapsed time: `just now` below one minute, whole minutes below one hour, then whole hours. Round down and use singular/plural labels (`1 minute ago`, `5 minutes ago`, `1 hour ago`).
- Default to a 12-hour clock with AM/PM; a 24-hour clock setting is deferred.
- Messages from yesterday show e.g. `yesterday at 1:41 PM`.
- Older messages use a zero-padded month/day/four-digit-year date and time, e.g. `03/14/2026 1:41 PM`.
- Labels are calculated on initial rendering and normal redraws (#75). Automatic once-per-minute refreshing, including calendar-boundary changes, remains a follow-up.

- Hovering a timestamp reveals its exact local date and time including seconds, e.g. `03/14/2026 1:41:08 PM`.
- Future timestamps display `just now` until the viewer's clock catches up; the exact-time tooltip still shows the actual timestamp.

## Implementation and test seams

`client/src/views/conversation/message_timestamp.rs` formats labels and exact times from an RFC3339 instant and an injected viewer-local current time. `message_row.rs` supplies `chrono::Local::now()` and attaches the exact-time tooltip only to the timestamp. This is display-only: API data, ordering and storage remain unchanged.

The agreed deterministic seams are the formatter and headless GPUI message-row rendering/hover. Suites live in `client/src/views/conversation/tests/message_timestamp.rs` and `message_row.rs`, declared by their owning conversation modules. They cover thresholds, calendar/year boundaries, timezone conversion, future instants, noon/midnight, tooltip hover and normal redraws. No desktop automation or real keyring is used.

Run from `client/`: `cargo test --locked message_timestamp` and `cargo test --locked message_row`.

## #75 automated verification

- Formatter and headless row/tooltip tests pass (four new tests).
- `cargo fmt --check`, `cargo check --locked` and `cargo build --locked` pass.
- Full `cargo test --locked`: 173 pass, nine fail. An archived copy of starting commit `3b5c8ea` reproduces the same nine failures (169 pass): theme naming, absent storage-status controls/feedback, and absent connection-status controls. These unrelated failures are not changed by #75.
- Strict `cargo clippy --locked --all-targets -- -D warnings` fails on the same five pre-existing unused/dead-code diagnostics as the starting commit: `restoring`, `storage_feedback`, `WorkspaceView.workspace`, and both live-update `status` methods.
- No native desktop or real-keyring verification was performed.
