# Best-effort live-update verification (#74)

Verified the integrated behavior after desktop #72 and server #73, starting at `c378c7f`. This report supersedes synchronization expectations for current behavior, not historical evidence in `LIVE-UPDATES-VERIFICATION.md` or `implement-epic/renodubois-hamlet-71.md`.

**Contract:** connected clients normally receive complete creation entities without polling. Reconnect restores future delivery, not missed creations or synchronized state. The database and ordinary HTTP reads remain authoritative. Disconnected, replacing-read and canceled-write publication gaps are accepted. No replay, catch-up operation, Refresh control, automatic write retry or new recovery machinery was added.

## Seams and changes

Verification uses the existing public conversation-owner boundary (`ConversationHandle`) against real registered Actix routes, bearer authentication, JSON/SSE decoding and disposable SQLite databases. That boundary predates #72, but exercises its simplified production coordinator and #73's inline write-then-notify operations. The test scope was recalibrated to the new best-effort contract, not the former readiness/baseline/recovery lifecycle.

New/extended coverage lives in `client/src/conversation/tests/route.rs`:

- `real_route_http_event_orders_reconcile_once_without_healthy_reads`: two distinct authenticated conversation owners observe both creation types. Message and channel confirmations exercise event-before-response and response-before-event. Identity deduplication, complete HTTP/event entity agreement, selection preservation and exact GET counts remain checked. Invalid messages, nonexistent channels, invalid channel names and duplicate names produce no phantom entities; later successful stream frames form finite observation barriers.
- `two_authenticated_coordinators_reconnect_without_catchup_after_actual_server_restart`: stops and awaits the real server, restarts a fresh pool/hub/listener with the same database and credentials, and creates 55 messages plus a channel while both clients are disconnected. At 2,999 ms attempts remain `[1, 1]`; at 3,000 ms they become `[2, 2]`, with zero automatic entity GETs. Retained active/inactive histories, 100 loaded original messages, older cursors, drafts and selected channel survive. Future messages/channels arrive without repairing the gap. Explicit pagination still reaches the oldest three original messages through the retained cursor. Healthy timer ticks over ten controlled seconds produce no polling. Reopening one conversation then performs exactly one ordinary channel-list and one newest-history load, retrieving the missed channel and a missed message. The other closed client performs no further reads.
- `real_route_independent_reads_pending_and_uncertain_writes_survive_reconnect` (new): delays actual HTTP responses and SSE bytes at transport boundaries. Initial channel/history loads finish while stream readiness is held. A local history failure leaves the healthy stream alone. A replacing-read snapshot may omit overlapping creations without buffering. A delayed history response and accepted-but-unconfirmed POST survive stream termination and the fixed retry; reconnect adds no GETs or POSTs, preserves selection/draft/pending state, and later HTTP confirmation inserts the complete entity once. Losing a successful POST response leaves uncertainty intact across another reconnect and a same-author, same-text creation. Only deliberately submitted writes are counted; no automatic resend occurs. Teardown prevents further stream attempts.

The reusable suite-local stream relay now supports multiple attempts and controlled termination. Each relay has a finite 64-chunk budget, each chunk/gate has a five-second timeout, and owned tasks are canceled on cleanup. Observation loops and server shutdown are also bounded. Reconnect time advances only through the controlled application clock; real sleeps yield for loopback I/O, not retry policy.

No production bug requiring a #72/#73 change was found. Verification slices were run immediately after adding each scenario/assertion. They exercise already-implemented behavior, so no behavioral red/green implementation cycle is claimed. An initial compile error used `history` instead of `history_page`; a focused failure used an invalid-length missing-channel ID and was corrected to a valid nonexistent ID. Strict clippy initially reported two unnecessary clones; both were removed before final checks.

## Representative retained coverage

Full suites retain implementation-owner responsibilities rather than duplicating every test here:

| Acceptance | Representative suites |
| --- | --- |
| Independent loading, local read failures, in-flight requests | New real-route scenario above; `conversation/tests/live_coordinator.rs` |
| Fixed retry/no reads, EOF/transport/parser/deadline/overflow | `every_terminal_failure_retries_once_after_three_seconds_without_reads` in `live_coordinator.rs`: two cycles for each terminal failure, 2,999/3,000 ms boundaries, queued creations discarded, no reads or resets |
| Pending channel/message writes, uncertainty, operation-only confirmation | New real-route scenario; `disconnect_keeps_pending_writes_and_only_originating_confirmation_clears_inputs` |
| Session teardown/replacement, obsolete attempts/requests, authoritative rejection | `conversation/tests/live_coordinator.rs`; `session/tests/{coordinator,route,state,binding}.rs`; `views/tests/session_lifecycle.rs` |
| Reading position, visible input retention, status and absent controls | `views/tests/live_updates.rs`, `views/tests/history_lifecycle.rs`, workspace/composer lifecycle suites: semantic controls and viewport anchoring, no native automation |
| Parser/frame/API queue bounds and auth | `api/tests/{events,event_http}.rs`: real TCP and controlled bytes, 64-KiB frame bound, 256-delivery queue, auth and cancellation |
| Server retention/slow-consumer isolation, fresh subscription after overflow | `server/src/live_updates/tests/hub.rs`, `server/tests/{live_updates,channel_publication,message_publication}.rs` |
| Normal failed database writes, payload identity, missed publication does not latch delivery closed | Server publication suites and owner-local `server/src/{channels,messages}/tests/publication.rs`; real-route API consumer scenario in `api/tests/event_http.rs` |
| Wire shapes, authentication, pagination and ordinary writes | `protocol/tests/wire.rs`, server contract/auth/pagination/write suites, client route/state/binding suites |

Generated current client/server contract, presentation and verification docs were reconciled. Historical verification/implementation records and all human-authored documentation/README files were left unchanged. The preexisting user edit to `llm-docs/live-updates-plan.md` was neither changed nor included in this work. Routes, protocol entities, OpenAPI artifact, dependencies and lockfiles are unchanged.

## Actual checks

All commands are relative to the repository root or the named component, not this document's directory.

| Component | Final commands (from component directory) | Result |
| --- | --- | --- |
| `protocol/` | `cargo fmt --check`; `cargo clippy --locked --all-targets -- -D warnings`; `cargo test --locked` | Pass; 4 wire tests |
| `server/` | `cargo fmt --check`; `cargo fmt --manifest-path migration/Cargo.toml --check`; `cargo clippy --locked --all-targets -- -D warnings`; `cargo test --locked` | Pass; 36 tests, 1 existing opt-in measurement ignored |
| `server/` | `cargo test --locked --test contract` | Pass; 2 tests, artifact drift and route/security/response checks; no regeneration needed |
| `client/` | `cargo fmt --check`; `cargo clippy --locked --all-targets -- -D warnings`; `cargo test --locked`; `cargo build --locked` | Pass; 171 tests, none ignored; build does not launch client |
| Root | `git diff --check` | Pass |

Intermediate `cargo check --manifest-path client/Cargo.toml --locked --all-targets` passed. Focused runs passed: all five conversation route scenarios, all nine live-coordinator scenarios, the reconnect-status view scenario and the reader-anchor scenario. Full applicable suites were then run at completion; the client suite was rerun after the review-stage helper refactor. Final command logs were written to `/tmp/hamlet-74-{protocol,server,client}-checks.log` and `/tmp/hamlet-74-client-review-checks.log` (ephemeral local evidence, not repository artifacts).

## Review

Parallel Standards and Spec reviews used `git diff c378c7f...HEAD` and GitHub issue #74. Spec found no missing, incorrect or out-of-scope requirements. Standards found one incorrect safety-gate documentation path and one nonblocking Message Chains suggestion about scenarios accessing relay task storage. The path was corrected and relay construction/awaited cancellation were encapsulated in suite-local owner methods; focused tests, strict clippy and the complete client suite passed again. No production behavior changed.

## Limitations

- This verifies best-effort future delivery, not gap-free handoff, missed-update repair, exactly-once writes, durable/cancellation-proof publication or database-commit ordering.
- Controlled response loss models an accepted write with an unavailable confirmation; it is not a native network-fault drill. Queue bounds are independent and do not bound total socket/proxy/process memory.
- No throughput target, fanout benchmark, scaling infrastructure, native desktop automation or real keyring access was required or run. The existing ignored fanout measurement remains optional.
- Native IME/accessibility, physical input, precise delayed pixel anchoring and real-wallet behavior remain separately unverified under the [native safety gate](client/VERIFY.md#native-safety-gate) in `llm-docs/client/VERIFY.md`.
