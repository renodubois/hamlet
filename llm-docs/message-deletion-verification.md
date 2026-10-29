# Message deletion verification (#85–#88)

Implemented in order against baseline `a94c3272dcbac133d8374f84da0fd73901b72e5a`. The human approved including existing uncommitted history/spec/configuration changes and these testing seams: history presentation/controls, authenticated HTTP/SSE routes with retention/race checks, Chat request/event behavior, and real GPUI controls.

## Automated coverage

- **#85:** `client/src/views/conversation/tests/history_removals.rs` covers variable-height removals/mixed edits, surviving/deleted anchors, latest-row removal, bottom-follow and empty older-history availability/loading/retry with retained cursors and no automatic backfill.
- **#86 server/wire:** `protocol/tests/wire.rs`, `server/tests/message_deletion.rs`, `server/src/messages/tests/deletion.rs`, `server/tests/storage.rs` and `server/tests/contract.rs` cover the payload/route, persistent ownership, rejection/repeat silence, retained values, concurrent repeats, channel-deletion ordering, newest/older snapshot races, filtering/pagination/cursors and nullable initialization.
- **#86 client:** `client/src/api/tests/message_delete.rs`, `client/src/chat/tests/message_delete_events.rs`, `client/src/chat/tests/message_delete_route.rs`, API event suites and conversation deletion controls cover binding/decoding, unloaded/inactive targets, observed-deletion filtering of delayed pages/creation/send overlays and live removal without reads.
- **#87:** `client/src/chat/tests/message_delete_confirmation.rs`, `message_delete_races.rs` and `client/src/views/tests/message_delete_controls.rs` cover request-free open/cancel, captured persistent author identity, pending suppression/dismissal, bounded preview/Cancel focus, HTTP/event orders, stale outcomes, definite/uncertain failures, deliberate retry, channel removal and session end.
- **#88:** the same real-control suite covers hover/focus/tooltip/keyboard, pane-local stable geometry, pointer reachability, selection/copy and composer Delete behavior. `client/src/views/tests/message_delete_route.rs` shares an authenticated route fixture for context menu and tray journeys, retained-row/live-removal checks and surviving-reader position.

## Final automated checks

From each component directory, formatting, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked` and `cargo build --locked` passed. Protocol Clippy/test/build also used `--all-features`. Server migration formatting passed; the server contract suite checked generated OpenAPI drift and route inventory.

- Client: 239 tests passed.
- Server: 51 tests passed; one pre-existing opt-in fanout measurement remained ignored.
- Protocol with all features: 17 tests passed.

Tests used controlled transports/time, fake credential providers, ephemeral loopback ports and temporary SQLite databases. No desktop client or real keyring was accessed. Red–green slices were followed during implementation; final checks do not establish native acceptance.

## Review corrections

Parallel Standards and Spec reviews found two small maintainability suggestions and two presentation bugs. Generic ID parsing was renamed and duplicated deletion-frame test helpers consolidated. A deleted tall row's offset is now clamped to keep its shorter measured replacement visible (unmeasured fallbacks start at their top); surviving-anchor offsets remain exact. The context-menu action now uses a named, red `MenuItem` on a real Kit base button, with a narrow view-owned single-action menu, since the pinned generic custom-item API cannot name its actionable node. Real-control regressions cover both corrections, including actual text color and dismissal/focus restoration. Final independent reviews read the complete baseline-to-implementation diff and all four originating issues: **Standards: 0 findings; Spec: 0 findings**. Reviews were read-only; the automated check outcomes above were established separately.

## Native boundary

At original implementation completion, native deletion acceptance was unverified. The subsequent consented 2026-10-12 [adversarial review](message-deletion-adversarial-review.md) fixed resize/tail-removal anchoring and clipped-tray defects, exercised isolated two-client native deletion journeys, and recorded pixel-identical surviving anchors. Final client checks passed 254 tests; server/protocol checks remained green. Physical keyboard/pointer hardware, assistive technology, IME and the other explicitly listed native boundaries remain unverified. Future native runs still require separate consent and the safety gate in `llm-docs/client/VERIFY.md`. No restoration, automatic backfill/retry, catch-up reads, replay, outbox or durable synchronization was introduced.
