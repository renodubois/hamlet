# Live updates — integrated verification (#70)

## Scope and result

Baseline: `d7dc5754b46aef52ad50c4f9941ec10cc4e84e79`, branch `live-updates`.
Prerequisites #63–#69 are completed/verified in the [historical ledger](implement-epic/renodubois-hamlet-71.md).
This is **verified uncommitted work**, not tracker closure, merge, deployment, native acceptance or a production capacity claim.

Added two real-route, separately authenticated desktop-coordinator scenarios and one bounded server fanout measurement. Existing correctness/compatibility/headless-view tests were audited and rerun, not deferred to this ticket. No production behavior, dependency, lockfile, OpenAPI artifact, migration, human documentation, README or legacy client changed. Only the client test-module declaration touches a production module. No new production metrics/test-only public APIs were introduced.

All required checks pass: protocol **4/5** tests without/with OpenAPI; server **40** tests; desktop **188** tests. None failed or ignored. The explicit server contract suite passes **2** tests and regenerated OpenAPI is byte-identical. No earlier-child production regression requiring reopening was found.

## New integrated evidence

### Real HTTP/event orders

`client/src/conversation/tests/route.rs::real_route_http_event_orders_reconcile_once_without_healthy_reads`

- Two production `ConversationHandle`s use separately signed-up Alice/Bob identities against two real Actix workers, disposable SQLite and real loopback HTTP/SSE.
- A five-second response barrier holds Alice's actual message POST response: both clients display its event while Alice's draft remains pending. Releasing the response clears only that operation's draft, with one row.
- A separate barrier holds actual SSE bytes for Alice's next send: its real HTTP confirmation clears the draft and renders before Alice receives the event; Bob already sees the event. A later remote channel event is an ordered consumption barrier proving the released duplicate has been reconciled.
- Both clients converge, remote channel creation preserves selection and allocates no history, and **both ordinary GET counts remain unchanged** through message/remote-creation updates.
- The same real-route barriers force **both channel-creation response/event orders**. An event alone cannot select or confirm Alice's pending creation. Her eventual HTTP confirmation intentionally selects the new uncached channel, requiring exactly **one selected-history navigation read** per local creation; Bob performs **zero** reads and keeps his selection without allocating that history. Released duplicate channel events cause no additional reads or rows. These selection reads are not hidden as healthy catch-up traffic.
- Test barriers do not fabricate payloads, bypass parsing or implement another conversation workflow.
- The SSE relay has a **one-chunk bounded queue**, at most 64 chunk reads, five-second network/barrier deadlines and an owned abort-on-drop task. Successful cleanup aborts/awaits that relay explicitly. This transport wrapper is used only to force arrival orders; the restart scenario uses the unwrapped production stream adapter.

### Actual server restart, not a controlled EOF

`client/src/conversation/tests/route.rs::two_authenticated_coordinators_recover_after_actual_server_restart`

- Both coordinators load a 50-row page, traverse to 53 rows and populate an inactive history/draft. Concurrent Alice/Bob sends reconcile HTTP and SSE into the same unique sorted rows, without follow-up reads. Remote channel/message events do not steal selection or allocate unloaded histories.
- Stops the actual listening Actix server with `stop(false)`, awaits its server task and observes both real TCP stream failures. Both clients expose **Reconnecting… messages may be out of date**, retain selected display rows, discard authoritative histories/older state, and preserve selected/inactive drafts.
- Rebinds the **same loopback address** with a **fresh AppState, database pool and event hub**, opening the same file database. This is a real server/worker lifecycle restart in the test process, not a synthetic body EOF; it is not an external binary/SIGKILL drill.
- Creates 55 messages and a channel before advancing either client's controlled retry clock. Existing credentials remain usable. Each client performs exactly **two** recovery GETs (channels + selected newest history), converges on the newest **50** rows, discards old confirmed rows/inactive caches, preserves selection/drafts and increments its reset revision.
- A subsequent deliberate older-page request reaches 100 rows using the **new** cursor rather than a pre-restart exhausted cursor. Both clients converge again.
- Retry/deadline policy uses the existing controlled execution clock. Five-second finite I/O/convergence bounds and millisecond yielding accommodate real loopback I/O; no sleep guesses establish ordering and no successful stream is collected to EOF.

These are characterization additions to already implemented behavior, not invented behavioral red/green claims. They passed on first execution. All three subsequent repeated runs of both scenarios passed. Existing semantic headless tests separately verify the visible status, retained rows/input and reader reset through real Kit controls (mapping below).

### Bounded fanout measurement

`server/src/live_updates/tests/transport.rs::measured_bounded_fanout_at_ten_changes_per_second`

The owner-local test uses real registered authenticated routes, file SQLite, two real Actix workers, **8 healthy TCP SSE readers** and **1 registered HTTP body deliberately not polled after readiness**. The fanout clients reuse one disposable session; separately authenticated clients are exercised above. All nine subscriptions are counted before traffic starts. A ready frame must arrive before each reader enters measurement.

It submits **270** real message creations, alternates two payload sizes and schedules one change per 100 ms for **27 seconds**. The mutation side uses the in-process registered HTTP route/SQLite, not a loopback POST; the delivery side crosses actual HTTP chunked TCP sockets. Latency is from route submission through the reader extracting the complete SSE frame, **not** server-enqueue latency or desktop render latency. Readers are sampled serially within each change, so results include test scheduling and preceding readers' JSON validation work. Every reader compares every event to the actual HTTP result. Heartbeats are read separately. The test has finite frame/header limits and deadlines; samples/vectors are bounded by the fixed client/change counts. Measurement uses ordinary monotonic `Instant`/Tokio time, **never accelerated policy clocks**.

Captured successful repeat (`70-fanout-repeat.log`):

| Measurement | Result |
| --- | ---: |
| Scheduled changes / elapsed / achieved rate | 270 / **27.000806201 s** / **9.999701416 changes/s** |
| Healthy TCP clients / paused HTTP bodies | **8 / 1** |
| Message character counts | **128 / 4,000** Unicode scalars |
| Message UTF-8 bytes | **128 / 7,000** |
| Complete SSE frame bytes, min–max | **356–14,228** |
| Healthy change deliveries | **2,160**, all matching; none skipped/duplicated |
| Healthy complete change-frame bytes, total | **15,750,720** (excludes ready/comments/HTTP/TCP overhead) |
| Ready latency, all eight connections (ms) | **2.635110, 0.388545, 0.289537, 0.287517, 0.270536, 0.278117, 0.279077, 0.322216** |
| Route-to-complete-frame latency p50 / p95 / max | **2.123006 / 6.257348 / 7.226738 ms** |
| Write-response latency p95 / max | **2.432203 / 2.974856 ms** |
| Maximum scheduling lateness | **1.442795 ms** |
| Shared retained events, peak | **256** |
| Shared retained complete-frame payload bytes, peak | **1,866,752** (~1.78 MiB) |
| Paused body outcome | **Terminal at change 257**, no resumed cursor after loss |
| Retained events after paused-body termination / at end | **0 / 0** |
| Subscriptions after bounded server-worker cleanup | **0** |

The larger payload is 1,000 repeats of four scalars (control U+0001, newline, crab and quote), exercising JSON escaping as well as Unicode. The ring-byte measurement sums the actual trailing frame lengths retained by the broadcast ring after healthy readers have consumed each change. It counts each shared `Bytes` payload **once**, not once per subscriber. Existing hub pointer-identity coverage establishes shared allocations. An earlier successful run measured 9.999676811 changes/s, p95 6.526051 ms, max 9.274190 ms and peak 1,866,749 bytes; small frame-length variation follows generated IDs/timestamps.

The initial measurement run passed delivery/overflow assertions but wrongly assumed that awaiting Actix's server future implied immediate destruction of every worker body. It observed four remaining subscriptions at that instant. Replacing that **test-harness assumption** with a five-second bounded worker-cleanup wait passed, including zero subscriptions/retention. No production cancellation change was necessary. The failed first run is not counted as a successful measurement.

**Limits of the measurement:** not a saturation test, arbitrary slow-network test, RSS/allocator profiler, desktop throughput benchmark, cross-host latency result or production sizing guarantee. Kernel, Actix, reqwest, SQLite, request/response and decoded client allocations are not included in retained-frame bytes. An unread TCP socket can temporarily drain into transport buffers, so deterministic slow-consumer overflow uses a paused registered HTTP body rather than claiming a precise socket-buffer budget. Healthy sockets continue receiving and writes complete while the slow body lags, then after it terminates. Existing real FIN/RST cleanup tests cover transport disconnection separately. The 10/s target and 256-event defaults remain **tuning candidates**, not a rate limit or proof of production capacity.

## Separate buffering/deadline bounds

| Owner | Current bound/policy | Failure/evidence |
| --- | --- | --- |
| Server shared hub | 256 retained complete serialized events, shared immutable payloads | Lag terminates old body; no catch-up from oldest surviving entry. Measured peak above; `hub::tests` checks exact capacity, sharing and release. |
| Server transport | Separate Actix/kernel socket buffering; no claimed total byte quota | Paused body models actual pressure on hub; TCP buffering is deliberately not conflated with hub retention. |
| Client SSE parser | 65,536 encoded bytes **per frame**, including incomplete frames/comments | 65,537 fails; several legal frames in one larger network chunk succeed. Decoded strings/current network chunk are separate allocations. |
| Client API deliveries | 256 supported deliveries (including readiness), separate terminal result | Full queue ends body/attempt; terminal preempts backlog. |
| Client executor bridge | 256 ordinary deliveries, **one** prioritized terminal slot | Creations never await ordinary capacity; overflow invalidates whole attempt. HTTP completions may wait without losing write results. |
| Client recovery buffer | 256 deliveries | Overflow abandons recovery, cannot acknowledge live. |
| Client replacing-read staging | 256 distinct channel creations and 256 distinct message creations in their respective staging maps | Duplicate IDs do not consume extra slots. Overflow cannot accept a lossy baseline. |
| Server validation/heartbeat | About 15 s; known expiry has priority | Due/stalled validation suppresses delivery; expiry/revocation/database failure ends body. |
| Client readiness / idle | 8 s from submission / 45 s without byte progress | No ordinary eight-second **total** stream timeout. |
| Client automatic recovery | Equal jitter in [base/2, base], base 1, 2, 4, 8, 16, 30 s capped | Reset only after accepted/reconciled baseline, not readiness. |

These are separate count/frame bounds, **not** a single client memory budget. Loaded histories and genuinely unresolved HTTP confirmations have different ownership/lifetimes; the #69 confirmation-retirement regression remains covered. No new metrics surface or production configuration was added for measurement.

## Complete #70 acceptance mapping

Paths are repository-root-relative. Existing tests are preserved and included in the full checks; test-name prefixes below identify the relevant scenarios without duplicating prior implementation.

| #70 criterion | Audited evidence |
| --- | --- |
| **1. Two-client direct creations, orders, concurrency, no reads/selection theft/cache allocation** | New two-coordinator real-route order/restart tests above. Existing `client/src/conversation/tests/route.rs` tests no-read second-user creations and live burst + older pagination; `client/src/views/tests/journeys.rs::bob_activity_arrives_through_session_stream_and_real_server_routes` exercises semantic controls. Concurrent multi-channel/message and paired-name/channel routes: `server/tests/{message_publication,channel_publication}.rs::concurrent_creations*`. Deterministically nonmonotonic creation instants plus ties: `server/src/messages/tests/publication.rs::independent_creations_arrive_out_of_timestamp_order_but_match_sortable_history`; client parsed-instant/numeric-ID order, duplicate merges and unloaded-history rules: `client/src/conversation/tests/live_state.rs::creations_deduplicate_and_order_by_instant_then_numeric_id_without_loading_other_channels`, `recovery_replaces_cursors_and_orders_concurrent_creations`, `remote_channels_keep_selection_and_local_confirmation_selects_without_duplicates`. Real concurrent delivery is exercised end-to-end; exact forced timestamp disorder is covered at the server publication and client merge seams, not falsely attributed to OS scheduling. |
| **2. Common visible recovery, newest-only authoritative reload, never false synchronization** | New **actual restart** test. `client/src/conversation/tests/live_coordinator.rs::terminal_failures_and_required_reads_share_retry_but_older_failure_stays_local` covers EOF, malformed supported events, transport, idle timeout and uncached selection failure. `initial_readiness_and_baseline_failures_keep_minimal_connecting_state`, `queued_obsolete_read_rejection_cannot_end_recovery_and_required_read_failure_retries`, `executor_and_recovery_overflow_abandon_attempt_without_applying_queued_entities`, `uncached_read_staging_overflow_abandons_live_attempt_before_read_completion`. Pure policy `client/src/conversation/tests/live_updates.rs` gates readiness/current baselines/merge acknowledgement and bounded jitter. Semantic `client/src/views/tests/live_updates.rs` verifies persistent notice/content/input through failed and incomplete baselines; `history_lifecycle.rs::healthy_events_keep_reader_anchor_but_recovery_resets_identical_ids_to_newest`. |
| **3. Read/navigation/older/write races, empty channels, preserved drafts/operations, uncertainty** | `live_coordinator.rs::readiness_precedes_baseline_and_buffered_creations_reconcile_without_followup_reads` covers before/during/already-in-read changes. `outage_keeps_write_identity_and_retargets_only_history_then_merges_confirmation`, `queued_a_to_b_to_a_rejection_cannot_settle_new_history_within_same_attempt`, `new_http_writes_remain_available_during_outage_without_premature_reads`, `uncertain_send_does_not_read_retry_or_match_text_and_confirmation_merges_in_either_order`. Empty channels synchronize without history in `queued_obsolete_read_rejection*`. `live_state.rs` covers replacing/older-page merges, staging overflow, pending identity preservation, obsolete reads, repeated resets, retirement of reconciled confirmations and inactive retention disposal. `coordinator.rs::pending_sends_are_per_channel_timeout_preserves_origin_and_never_reads_or_replays`; composer/draft/older-retry view coverage remains. |
| **4. Lifetime, auth, one stream, no obsolete tasks/credential leakage** | `client/src/conversation/tests/coordinator.rs` independent observers/closed activity/current rejection/expiry cases; `client/src/session/tests/coordinator.rs::session_loss_closes_surviving_conversation_handles_before_any_host_update`; `client/src/views/tests/{protected_binding,history_lifecycle,composer_lifecycle,workspace}.rs` recreation/hidden controls/multiple observers/server replacement; `execution.rs::focus_and_elapsed_time_never_poll_or_restart_the_session_stream`. `client/src/api/tests/events.rs` immutable binding/drop/canceled wait and nonauthoritative EOF; `event_http.rs` real redirect/no bearer forwarding and TCP body drop; `binding.rs` old/new context origins/credentials. `server/tests/live_updates.rs` expiry, continuous-traffic revocation, pending validation, expiry during stalled validation, and `failed_validation_ends_stream_but_next_handshake_is_server_error_not_rejection` distinguish database failure/500 from revoked credential/401. New relay is explicitly canceled; restart closes both real streams. |
| **5. Independent compatibility fixtures, framing and no replay** | `protocol/tests/wire.rs` literal JSON for Message/Author/Channel/ChannelType and both tagged variants, unchanged entity shapes, Unicode/newlines/IDs/timestamps/max bodies/extra fields; not just shared-type compilation. `client/src/api/tests/{binding,events}.rs` independent literal HTTP/SSE fixtures, both known entities, validation, extra fields, unknown SSE names/application types, malformed/duplicate recognized fields, every byte split/BOM/CR/LF/CRLF/multiline framing and finite final EOF. `server/tests/live_updates.rs::subscription_precedes_ready_and_fanout_is_fresh_and_escaped` supplies Last-Event-ID but expects fresh ready, no pre-subscription replay and no IDs. `server/src/live_updates/tests/transport.rs::real_workers_share_fanout_and_disconnect_releases_receivers` compares literal on-wire SSE across workers. Ready-before-baseline is covered in the coordinator and every restart. Unknown-event ignoring is **provisional**, not a promise that old clients remain correct after future deletion/edits/permission events; revisit compatibility before adding them. |
| **6. Bounded ~10/s fanout with measurable pressure/slow outcome** | New owner-local measured test and results/bounds above: eight TCP readers + paused route body, 270 changes/27 s, two payload sizes, readiness/latency/retention, termination at 257, zero retained subscriptions after cleanup. Server/client limits distinguished; no production guarantee. |
| **7. Full locked checks and contract inventory/artifact** | Exact commands/results below. All component lockfiles unchanged; no root workspace introduced. |
| **8. Reproducible documentation/cancellation/manual limitations/no polling or auto retry** | This report, appended historical ledger, and corrected generated `llm-docs/client/{VERIFY,PRESENTATION}.md`. Source audit finds no production conversation polling, Refresh controls, manual refresh entry points or automatic send/uncertainty catch-up scheduling; only the internal baseline `refresh_channels` helper remains. The one-second coordinator timer serves expiry/recovery, not periodic conversation reads. Older-page user retry and session/storage workflows are distinct. Semantic absence/no-GET tests execute these guarantees rather than relying on grep alone. |

## Cancellation conclusions and exceptional failure policy

Full server verification reruns `server/tests/channel_disconnect.rs` and `server/tests/message_disconnect.rs`: real TCP RST cancels the originating Actix service future while SQLite is inside a deterministic commit barrier. SQLx's database worker can still commit. Each feature-owned write-plus-notify task survives ordinary request cancellation and publishes the committed identity to a separately connected subscriber. These tests originated in #66/#67 and remain the evidence; this ticket does not claim to have rediscovered the gap.

The owner-local channel/message publication tests also rerun injected post-commit panic and owned-task abort while SQLite can commit later. The **approved simple restart-required latch** still ends old and fresh SSE delivery, while HTTP remains available. Reconnecting before a late commit would be unsafe; no deeper reconnect redesign, outbox or multi-writer scheme was added. Normal disconnect and ordinary database failure do not latch the hub. Actual restart now has two-client baseline recovery evidence above, without pretending to measure crash durability.

Read-half FIN alone is not immediate response cancellation on this Actix HTTP version; existing transport cleanup tests use finite follow-up writes to detect vanished readers. Idle production heartbeats normally provide those writes. During stalled revalidation, delivery/heartbeats stop until validation resolves or known expiry. Awaiting server stop can precede worker body destruction; the measured test waits for actual zero receivers with a finite bound. No obsolete workflow, task or credential is inferred safe merely from dropping an unrelated handle.

## Reproduction and full checks

Environment: Linux `7.2.7-arch1-1 x86_64`; `rustc 1.95.0 (59807616e 2026-04-14)`, `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`; dev/test profiles, local file SQLite/WAL. Locked versions: Actix Web **4.15.0**, Actix HTTP **3.13.6**, SeaORM **2.0.3**, SQLx/SQLx SQLite **0.9.0**, Tokio **1.53.1**, reqwest **0.12.28**, GPUI Kit **0.6.1**, GPUI-pre **0.3.4**. No versions changed.

All commands below ran successfully from the **repository root**. Temporary logs are under `/tmp/hamlet-epic-71/70-*`; this report records durable results independently of those logs.

```sh
cargo fmt --manifest-path protocol/Cargo.toml --check
cargo clippy --manifest-path protocol/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path protocol/Cargo.toml --locked
cargo clippy --manifest-path protocol/Cargo.toml --locked --all-targets --features openapi -- -D warnings
cargo test --manifest-path protocol/Cargo.toml --locked --features openapi

cargo fmt --manifest-path server/Cargo.toml --check
cargo fmt --manifest-path server/migration/Cargo.toml --check
cargo clippy --manifest-path server/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path server/Cargo.toml --locked
cargo test --manifest-path server/Cargo.toml --locked --test contract
cargo run --manifest-path server/Cargo.toml --locked --quiet --bin generate-openapi > /tmp/hamlet-epic-71/70-openapi.json
cmp server/openapi.json /tmp/hamlet-epic-71/70-openapi.json

cargo fmt --manifest-path client/Cargo.toml --check
cargo clippy --manifest-path client/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path client/Cargo.toml --locked
cargo build --manifest-path client/Cargo.toml --locked

git diff --check
```

Focused reproduction (current route-suite filter):

```sh
cargo test --manifest-path client/Cargo.toml --locked conversation::route_tests -- --nocapture
cargo test --manifest-path server/Cargo.toml --locked --lib measured_bounded_fanout -- --nocapture
```

Historical two-client runs used `conversation::restart_tests` (two tests, plus three additional consecutive passes). The Standards fix moved both scenarios into `tests/route.rs` and renamed their module filter to `conversation::route_tests`, which also includes the two existing route tests. Historical logs retain the old filter; it no longer selects tests.

The server measurement runs in the ordinary full suite (not ignored); `--nocapture` prints `FANOUT_METRICS` for repeatable capture. There is no generic benchmarking infrastructure. Assertions enforce matching deliveries, bounds and cleanup, not a machine-dependent millisecond performance threshold. The initial cleanup-assumption failure is retained in `70-fanout.log`; successful captures are `70-fanout-green.log` and `70-fanout-repeat.log`. Full logs: `70-{protocol*,server*,migration-fmt,contract,client*}.log`; integrated repeats: `70-integrated-repeat-{1,2,3}.log`.

## #70 Standards follow-up — route-suite ownership

Baseline `dd21ebc`. Both two-client scenarios and their private helpers now live in `client/src/conversation/tests/route.rs`, under the existing test-only `conversation::route_tests` module. Removed `restart_tests` and `tests/restart.rs`; no production visibility or runtime behavior changed. The moved test bodies/helpers are byte-identical to the baseline; the existing second-user route test now reuses the same suite-local GET-counting adapter with its original HTTP client configuration. All four route scenarios/assertions remain covered. This is a test-layout refactor, not a behavioral red/green change.

Passed from repository root after the move:

```sh
cargo test --manifest-path client/Cargo.toml --locked conversation::route_tests -- --nocapture
cargo fmt --manifest-path client/Cargo.toml --check
cargo clippy --manifest-path client/Cargo.toml --locked --all-targets -- -D warnings
cargo test --manifest-path client/Cargo.toml --locked
cargo build --manifest-path client/Cargo.toml --locked
git diff --check
```

Focused route suite: **4 passed**; full desktop suite: **188 passed**, none failed/ignored. Logs: `/tmp/hamlet-epic-71/70-standards-route.log` and `70-standards-client-{fmt,clippy,test,build}.log`. Prior server/protocol evidence above is historical, not rerun for this client-only move. No native/keyring access, server/README edits, commit or tracker mutation. Verified uncommitted follow-up; manual limitations below remain unchanged.

## Outstanding consent-dependent/manual checks and limitations

- **Not run:** native desktop automation or real Secret Service/keyring access. Headless GPUI/Kit and controlled providers are not native acceptance. Follow `llm-docs/client/VERIFY.md`'s consent/unlocked-workspace/provider-isolation safety gate.
- Native two-profile interaction, IME candidate/Enter behavior, physical keyboard/accessibility, exact delayed-prepend pixel anchoring, and locked/slow real-wallet timeout/deletion remain unverified. Packaging/non-Linux are unverified.
- Removed the stale generated manual whole-body proxy example: collecting upstream SSE to EOF is invalid. A consented native delayed/lost-POST drill needs a separate isolated SSE-aware relay; it was not implemented or run. Automated uncertainty and real response/event barriers are covered above.
- No external-process kill/crash/power-loss durability drill, real-network fanout saturation, arbitrary socket-buffer memory measurement, production-capacity promise, multi-instance deployment, external database writers, replay log, deletion/permissions or unlimited future-event compatibility.
- No unresolved automated #70 correctness gap was observed. Parent still owns independent review, any authorized commit and tracker decisions; none were performed here.
