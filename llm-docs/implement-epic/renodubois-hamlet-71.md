# Epic #71 execution ledger

- Parent: [Live updates: implementation and verification tracker](https://github.com/renodubois/hamlet/issues/71).
- Branch: `live-updates`.
- Immutable epic baseline: `f15a69306c9122f318b60ee79940fa96b4b86951`.
- Working tree at baseline: clean. User committed the previously untracked skill and plan before authorizing discovery; neither is implementation work for this run.
- Plan status: user approved the full eight-child plan, test seams, local commits/reviews, and tracker closure policy.
- Discovery: recursively paginated native sub-issues and dependencies; full bodies, paginated comments, states/reasons, labels and assignees retrieved. Eight open, unassigned, `ready-for-agent` leaves. No nested containers, external blockers, cycles, or disagreement with the parent's explicit child list/textual dependencies.
- Scope: all eight leaves; parent is a container with integrated acceptance auditing, not an additional implementation task. Issue bodies remain authoritative; existing design context: `llm-docs/live-updates-plan.md`.

## Execution frontier

Sequential order follows native hierarchy among runnable issues: #63, #64, #65, #66, #67, #68, #69, #70. Each blocker must be verified on this branch and closed as completed before dependent implementation starts.

| Issue | Blockers | Status | Acceptance coverage / evidence | Tests / results | Review | Commits |
| --- | --- | --- | --- | --- | --- | --- |
| [#63 Shared wire types](https://github.com/renodubois/hamlet/issues/63) | None | Completed; tracker verified CLOSED/COMPLETED | Exact entity/event JSON, 4000-character Unicode/newline body, timestamp/ID/additive fields, unchanged HTTP/OpenAPI artifact, API validation | Protocol 4/5 tests (without/with OpenAPI); server 8 tests; desktop 159 tests; all fmt/clippy/build checks pass | Standards: 0; Spec: 0 actionable findings | `a411f6d` |
| [#64 Race-safe conversation state](https://github.com/renodubois/hamlet/issues/64) | None | Completed; tracker verified CLOSED/COMPLETED | Entity merging, replacing/older read races, bounded staging, recovery preservation | Desktop check/fmt/clippy/test/build pass; 171 tests | Standards: no violations, 2 optional cleanups; Spec: 0 findings | `402c872` |
| [#65 Authenticated bounded SSE](https://github.com/renodubois/hamlet/issues/65) | #63 | Completed; tracker verified CLOSED/COMPLETED | Protected SSE, shared 256-event hub, framing/readiness, priority validation/expiry, bounded lag, cross-worker/disconnect, contract | Server 22 tests; desktop 171 tests; fmt/clippy/build and artifact checks pass | Standards: no violations, 2 optional cleanups; Spec: 0 findings | `44bf5f2` |
| [#66 Channel publication](https://github.com/renodubois/hamlet/issues/66) | #65 | Completed; tracker verified CLOSED/COMPLETED | Matching channel fanout, invalid/duplicate/insert failures, deterministic ID retries, concurrent writes, real TCP reset during SQLite commit, supervised lifetime and approved fail-closed exceptional delivery | Server 30 tests; strict fmt/clippy/check, unchanged OpenAPI, desktop locked build; five repeated focused runs | Standards: no violations, 1 optional fixture cleanup; Spec: 0 findings | `76b736d` |
| [#67 Message publication](https://github.com/renodubois/hamlet/issues/67) | #65 | Completed; tracker verified CLOSED/COMPLETED | Matching message payloads, pre-write author preparation, failures/cancellation, approved exceptional safeguard | Server 39 tests; desktop 171 tests/build; protocol 4/5 tests; all checks pass | Standards: no violations, 1 optional helper cleanup; Spec: 0 findings | `e0bc0a0` |
| [#68 Desktop API stream](https://github.com/renodubois/hamlet/issues/68) | #66, #67 | Completed; tracker verified CLOSED/COMPLETED | Verified server gate; bound transport, incremental parsing/validation, deadlines, bounded delivery, actual creations | Desktop 189 tests/build; server39; protocol4/5; all checks pass | Standards fixture-layout finding fixed/re-reviewed; Spec: 0 findings | `108c8a5`, `ca23c7c` |
| [#69 Desktop live synchronization](https://github.com/renodubois/hamlet/issues/69) | #64, #68 | Completed; tracker verified CLOSED/COMPLETED | One session stream/recovery lifecycle, races, local work preservation, stale UI, polling removal, real two-user no-followup-read proof | Desktop186 tests; fmt/strict clippy/build pass | Confirmation-retention and test-layout findings fixed; both axes re-reviewed pass | `7c75c3b`, `6671da9` |
| [#70 Integrated verification](https://github.com/renodubois/hamlet/issues/70) | #69 | Completed; tracker verified CLOSED/COMPLETED | Two-client actual restart and response/event orders, compatibility, measured bounded fanout, full checks | Protocol4/5, server40, desktop188; all checks pass | Standards route placement fixed/re-reviewed; Spec:0 findings | `dd21ebc`, `22512a2` |
| [#71 Parent acceptance](https://github.com/renodubois/hamlet/issues/71) | All children | Verified; closure pending checkpoint | Integrated no-polling updates, newest-only authoritative recovery preserving local work, visible bounded failure, actual two-client restart and measured fanout | Final protocol4/5, server40, desktop188; all required checks pass | Original-baseline Standards/Spec pass; stale-contract followup fixed/re-reviewed; no material findings | Integrated through `eecde5c`; documentation fix `accd9f3` |

## Approved test seams

1. Protocol public Serde/OpenAPI boundary (`protocol/tests/`), independent exact JSON fixtures and existing HTTP contract/validation checks; feature on/off checks.
2. Conversation owner-local transition/coordinator boundaries (`client/src/conversation/tests/`): entity/read/write identities, ordering, bounded staging/reset; preserve polling behavior until #69.
3. Server public HTTP routes and concrete hub subscription lifecycle: disposable databases, real loopback for transport/disconnect/cross-worker behavior, finite frame reads, controlled clocks and deterministic database barriers. Mutation results observed through HTTP and subscriptions, not private implementation assertions.
4. Authenticated desktop API boundary (`client/src/api/tests/`): production binding/decoding with controlled transport/time and real routes; finite deadline/frame/queue behavior, cleanup.
5. Conversation lifecycle and headless semantic view boundaries (`client/src/conversation/tests/`, `client/src/views/tests/`): injected API/time, real-route two-client scenarios, selection/session/write races, notices and history reset. No native desktop automation or real keyring.
6. Integrated real-route two-client harness and bounded fanout measurement in #70, recording client count/duration/payload sizes/latency/pressure/slow consumers without production capacity claims.

## Verification and delivery policy

- Test-first vertical slices at approved seams; focused tests and type/static checks during implementation.
- Each child gets its own implementation commit before parallel read-only Standards and Spec reviews; fixes are committed, checked and reviewed before closure.
- Protocol: format, lint/test with and without `openapi`; lockfile updates deliberate, no root workspace.
- Server: `cargo fmt --check`; migration formatting; `cargo clippy --locked --all-targets -- -D warnings`; `cargo test --locked`; contract artifact/inventory verification. Full suite on route/DTO changes.
- Client: `cargo fmt --check`; `cargo clippy --locked --all-targets -- -D warnings`; `cargo test --locked`; `cargo build --locked`.
- Before #68: complete server publication/cancellation contract and full server checks; existing desktop still builds.
- Final full affected-component checks and integrated parallel Standards/Spec review from original baseline required before parent closure.
- Verified issues close as implemented and verified on this branch, even while commits are local/unpushed. Tracker comments explicitly distinguish this from merge/deployment.
- No push, PR, merge, branch change, native automation, real keyring access, legacy-client edits, or human-owned documentation edits.
- Scope conflicts/cancellation machinery beyond the issue constraints require renewed approval. Failed/unavailable required checks block completion; native consent-dependent checks remain explicit manual limitations, not claimed passes.

## Resume checkpoint

#63–#70 closed with evidence and parent progress comments. Parent #71 verification is complete; closure pending final checkpoint. Original-baseline integrated Standards/Spec reviews through `eecde5c` passed with no material findings. Optional stale server-contract wording fixed in `accd9f3` and re-reviewed on both axes; optional duplicated server transport test helpers retained as non-blocking maintenance work. Live recursive tree/dependencies refreshed: no new children, changed requirements, or unresolved blockers; parent checklist alone auto-updated to checked children. Full required protocol/server/client checks rerun at `22512a2`: all pass (protocol4/5, server40, desktop188), including migration formatting, artifact comparison and desktop build. Logs `/tmp/hamlet-epic-71/final-{protocol,server,client}.log`; exact commands match the complete verification report. Server contract/publication/cancellation gate is verified on this branch; protocol and full server/desktop checks passed before starting client transport. #65 baseline was `3a5a55dd20c3944a07411835e14976c5ec16cc99`. Prior parallel read-only Standards and Spec reviewers read the complete committed #63/#64 diffs and passed. Initial reviewer attempts lacked command tools; supplied complete diff artifacts for the successful second reviews. All prior commits remain local/unpushed.

### #63 verification

Red tests first failed on absent message/event types, then absent channel variant/types; each slice passed after implementation. Added compatibility/validation characterization coverage. An initial OpenAPI test over-specified an explicit discriminator (not required by the contract); corrected it to assert both tagged `oneOf` payloads. No artifact regeneration was needed: existing server contract tests prove generated OpenAPI stayed unchanged.

All commands below passed from the repository root:

- `cargo check --manifest-path server/Cargo.toml` and `cargo check --manifest-path client/Cargo.toml` (deliberate lockfile updates add only the shared path package).
- `cargo fmt --manifest-path protocol/Cargo.toml --check`
- `cargo clippy --manifest-path protocol/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path protocol/Cargo.toml --locked` (4 tests)
- `cargo clippy --manifest-path protocol/Cargo.toml --locked --all-targets --features openapi -- -D warnings`
- `cargo test --manifest-path protocol/Cargo.toml --locked --features openapi` (5 tests)
- `cargo fmt --manifest-path server/Cargo.toml --check`
- `cargo fmt --manifest-path server/migration/Cargo.toml --check`
- `cargo clippy --manifest-path server/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path server/Cargo.toml --locked` (8 tests including contract artifact/inventory)
- `cargo fmt --manifest-path client/Cargo.toml --check`
- `cargo clippy --manifest-path client/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path client/Cargo.toml --locked` (159 tests)
- `cargo build --manifest-path client/Cargo.toml --locked`

Temporary diagnostic logs: `/tmp/hamlet-epic-71/63-*-checks.log` (not needed to resume; command results recorded above). No native automation/keyring access.

### #64 implementation evidence

Baseline: `810b1056fc8fea6c0527ed188130549f461b8876` on `live-updates`; implementation commit `402c872`. Only conversation state, its owner-local tests, and this ledger changed. Parallel committed-diff reviews passed with no material findings. Standards suggested optionally inlining the legacy merge wrapper and sharing similar bounded-staging mechanics; retained owner-specific logic for this prefactor, with legacy reconciliation due for removal in #69. Spec had no findings. Tracker closure verified completed; parent progress posted.

Implemented:

- Shared ID-based message/channel creation merges. Message insertion compares parsed RFC3339 instants then descending numeric IDs; channels retain snapshot order and insert by the existing server name-key order. Remote channels do not select; confirmed local creation still selects and deduplicates event/response races. Unloaded message channels do not allocate histories.
- Channel-list and replacing-history reads each stage up to 256 distinct concurrent creations. Duplicate delivery consumes no extra capacity. Merges return `Err(ReconciliationOverflow)` on overflow; `reconciliation_overflowed()` also exposes overflow caused by HTTP confirmations. Overflowed snapshots cannot replace state as successful baselines. Retry/reset clears the affected flags. These are per-read creation bounds, not new limits on existing history, local confirmed outcomes, or polling catch-up traversal.
- Accepted replacing reads reconcile staged creations and pending confirmed entities. Older pages merge into current history, preserving concurrent additions and their own cursor. Existing session/request checks reject obsolete completions.
- `reset_for_recovery()` discards all history/cursors/older-page state and invalidates read identities, preserving the selected channel, retained channel list, drafts, pending write identities, confirmed outcomes, and uncertainty. It does not cancel write tasks or perform full session teardown. A replacement channel list still validates selection. History is discarded rather than displayed stale in this prefactor.
- `recovery_reset_revision()` changes on every recovery reset, independently of entity IDs. Recovery, overflow observation, and revision access have narrowly scoped dead-code allowances until #69 wires their consumers. No scheduler, transport, view, or API-validation changes.
- Polling/manual refresh/uncertain catch-up remain active. Catch-up captures its original head so concurrent merges cannot move the continuity boundary. A separate pending-confirmation boundary preserves polling continuity when an initial snapshot is reconciled with a newer confirmed send.

#### Red/green slices

Commands below ran from `client/`. Each red command preceded that slice's production implementation. Logs: `/tmp/hamlet-epic-71/64-NN-red.log` and `64-NN-green.log`.

| Slice | Red command (`cargo test --locked` filter) | Observed red | Green command (`cargo test --locked` filter) / result |
| --- | --- | --- | --- |
| 01 | `conversation::state::live_tests::creations` | Missing shared message merge | Same filter: 1 passed |
| 02 | `conversation::state::live_tests::remote_channels` | Missing shared channel merge | `conversation::state::live_tests`: 2 passed |
| 03 | `conversation::state::live_tests::replacing_channel` | Stale list changed selection and lost confirmed channel | `conversation::state::`: 30 passed |
| 04 | `conversation::state::live_tests::replacing_history` | Replacing history lost concurrent confirmation | `conversation::state::`: 31 passed |
| 05 | `conversation::state::live_tests::replacing_read_overflow` | Missing bounded staging/explicit overflow result | `conversation::state::`: 32 passed |
| 06 | `conversation::state::live_tests::older_page` | Older page appended behind an earlier-timestamp live creation | `conversation::state::`: 33 passed |
| 07 | `conversation::state::live_tests::polling_catchup` | Concurrent creation moved catch-up's head and falsely broke continuity | `conversation::`: 47 passed |
| 08 | `conversation::state::live_tests::recovery_reset` | Missing reset/revision interface | `conversation::`: 48 passed |
| 09 | `conversation::state::live_tests::confirmation_during_initial` | Initial-read confirmation prematurely ended polling catch-up before intervening message | `conversation::`: 49 passed |

Three additional preservation/obsolete-read characterizations passed without production changes: canceled staging cannot contaminate new selection; confirmed HTTP outcomes survive overflow/reset; repeated identical-ID baselines still change reset revision and preserve uncertainty. Final focused command: `cargo test --locked conversation::state::live_tests` — 12 passed (159 filtered out).

Intermediate `cargo check --locked` passed after slices 02, 05, 08. Intermediate `cargo clippy --locked --all-targets -- -D warnings` passed after slices 04 and 07. The slice-09 lint run caught one test-only `get(...).is_none()` style warning; changed it to `contains_key` and the final strict lint passed.

#### Final checks

All passed from `client/`:

- `cargo check --locked`
- `cargo fmt --check` (after `cargo fmt`)
- `cargo clippy --locked --all-targets -- -D warnings`
- `cargo test --locked` — **171 passed, 0 failed, 0 ignored** (all 159 pre-existing tests retained)
- `cargo build --locked`

`git diff --check` passed. Final logs: `/tmp/hamlet-epic-71/64-final-{check,fmt,clippy,test,build}.log`.

Handoff to #69: call reset from the conversation coordinator, cancel/invalidate its read tasks (not writes), route merge errors/HTTP overflow observation into global recovery, and consume the revision in history presentation. This child intentionally does not activate those paths, remove polling-era workflows, or add a second synchronization policy. Existing polling confirmation queues/catch-up behavior remain until cutover. No native automation or real keyring access; no native acceptance claimed.

### #65 implementation and verification evidence

Baseline: `3a5a55dd20c3944a07411835e14976c5ec16cc99` on `live-updates`. Worker scope was only approved #65, as sole writer. No commit, branch change, tracker mutation, or child #66/#67 implementation. Parent owns commit and subsequent Standards/Spec reviews.

Implemented:

- Protected `GET /api/v1/events`, standard bearer/error distinctions and authenticated uniform 405s. Explicit `text/event-stream`, identity encoding, no-cache/no-transform and local proxy buffering hint. Compression bypass is exercised through real loopback with Actix Compress enabled.
- `AppState` initializes one clone-sharing concrete hub before the existing worker factory. Synchronous `notify(Event)` serializes once into immutable shared Bytes, retains at most 256 events and treats no receivers as normal. Subscription registration precedes ready. Lag before/after readiness is terminal rather than Tokio cursor recovery; drop/terminal body cleanup releases retention.
- Complete ready `{}`, tagged change JSON, escaped Unicode/newlines and heartbeat-comment frames. No IDs or replay; Last-Event-ID yields a fresh subscription. No channel/message operation publishes yet, and desktop polling/build behavior remains intact.
- Shared session lookup extracted from bearer middleware; private digest/expiry with no raw-token storage in the stream. Direct deadline checks plus prioritized timers prevent busy events from delaying expiry or due validation. Pending validation suppresses changes and heartbeats. Expiry also interrupts a blocked validation future. Revocation/expiry/missing session/database failure closes the stream body without replacing its already-sent HTTP status.
- OpenAPI endpoint/Event schema, generated artifact and explicit source inventory updated. Representative contract response reads exactly one bounded frame. Language-neutral contract in `llm-docs/server/LIVE-UPDATES.md` covers framing, recovery, bounds, auth versus EOF, deployment and provisional compatibility; generated overview/architecture links updated.
- Minimal dependency edges: direct futures-util (std only) and explicit Tokio sync/time/macros plus test clock/I/O features. Deliberate unlocked checks updated both component locks; the final lockfile diffs each add only the hamlet → futures-util edge, with no package version changes. No root workspace or migration.

#### Red/green slices

All commands used `--manifest-path server/Cargo.toml`; `cargo test` commands below also used `--locked`. Raw logs are under `/tmp/hamlet-epic-71/65-*` (the durable evidence is this ledger).

| Slice | Failing seam/test before implementation | Observed red | Green result |
| --- | --- | --- | --- |
| 01 | `--test live_updates protected_stream` | Protected events path returned 404 instead of 405 | Protected route and finite ready frame passed |
| 02 | `--test live_updates subscription_precedes` | AppState had no event hub | Two registered subscribers receive ready then one escaped change; pre-subscription notification is not replayed |
| 03 | `--test live_updates idle_stream` | Bounded heartbeat read timed out | Controlled 15-second advance yields a comment |
| 04 | `--test live_updates revoked_session` | Revoked idle stream emitted heartbeat instead of EOF | Idle and queued/busy revocation passed after shared validation extraction and priority checks |
| 05 | `--test live_updates known_expiry` | Expired idle stream did not end within bounded read | Idle, queued and not-yet-ready expiry all terminate |
| 06 | `--test live_updates lag_terminates` | Due heartbeat escaped before lag termination | Lagged pre/post-ready bodies terminate without further frames |
| 08 | `--test live_updates protected_stream` | Missing explicit identity encoding | Header test and real compression-middleware bypass pass |
| 10 | `--test contract` | New registered endpoint absent from OpenAPI inventory | Artifact/schema/source inventory and bounded HTTP example pass |

Development corrections: the first route-test compilation used unsupported HeaderMap indexing; corrected before its behavioral red run. The initial event fixture used `username` rather than the existing Author `display_name`; corrected without changing the protocol. During slice 04, queued events exposed that biased timer polling alone did not cover timer-wheel granularity; direct deadline checks fixed that priority gap. These are recorded rather than counted as acceptance passes.

Additional characterization (`65-07`, `65-09`, `65-11` logs) proved:

- A real single-connection SQLite transaction blocks revalidation deterministically. No heartbeat/change is delivered while blocked; releasing it resumes only after successful validation, and known expiry terminates even before the barrier releases.
- Dropping the sessions table closes the existing 200 stream without an error frame; a new handshake returns 500/internal_error, not 401. Logout's subsequent handshake returns the ordinary 401.
- Fourteen successive clock advances and delivered changes after logout cannot restart the validation interval: the queued change at the fifteenth second is not delivered.
- Exact capacity 256 remains readable; event 257 causes pre-ready lag. Payload pointer identity demonstrates immutable allocation sharing; retained count stays 256 during a 1024-event burst, then falls to zero when subscribers drop. Terminal HTTP lag releases its receiver even while the caller retains the ended body.
- Real HTTP/1.1 subscribers on **two distinct Actix workers** (test-only worker response IDs) receive the same exact channel frame. Finite chunk-decoded reads have five-second bounds and a 64 KiB test accumulation limit. Dropping TCP readers and subsequent writes cause receiver/retention counts to reach zero.

The initial loopback characterization expected TCP FIN alone to release an indefinite HTTP/1 response within five seconds; that expectation failed. Installed Actix HTTP 3.13.6 dispatcher source confirms permitted read-half closure. The corrected transport test uses bounded follow-up notifications to detect the vanished reader (idle heartbeats provide such writes in production). This is documented, not claimed as immediate FIN cancellation, and no global transport redesign or cancellation registry was introduced. An indefinitely stalled database suppresses heartbeats/delivery until validation resolves or known expiry; it does not let queued events escape.

#### Final verification

All commands passed from repository root:

- `cargo check --manifest-path server/Cargo.toml --locked`
- `cargo fmt --manifest-path server/Cargo.toml --check`
- `cargo fmt --manifest-path server/migration/Cargo.toml --check`
- `cargo clippy --manifest-path server/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path server/Cargo.toml --locked` — **22 passed**, including all 8 prior tests, 10 HTTP lifecycle tests and 4 focused/transport tests; none ignored.
- `cargo test --manifest-path server/Cargo.toml --locked --test contract` — **2 passed**; also included in the full 22.
- `cargo run --manifest-path server/Cargo.toml --locked --quiet --bin generate-openapi` to a temporary artifact, then `cmp` against `server/openapi.json` — identical.
- Five additional consecutive runs of both `--test live_updates` and `--lib live_updates` — all **14 tests passed** on every run.
- `cargo fmt --manifest-path client/Cargo.toml --check`
- `cargo clippy --manifest-path client/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path client/Cargo.toml --locked` — **171 passed**, polling and existing real-server journeys retained.
- `cargo build --manifest-path client/Cargo.toml --locked`
- `git diff --check`

Final logs: `/tmp/hamlet-epic-71/65-final-{fmt,clippy,test,check,contract}.log`, `65-final-client-{fmt,clippy,test,build}.log`, `65-repeat-{1,2,3,4,5}.log`; generated comparison artifact `65-final-openapi.json`. Official WHATWG, Tokio and Actix sources are cited in the contract; installed APIs were inspected as well. No native UI automation/keyring access or human-authored/README/legacy-client edits.

Committed as `44bf5f2`; parallel complete-diff Standards/Spec reviews found no material issues. Optional existing status-map and repeated test-fixture duplication retained as non-blocking cleanup suggestions; no Spec findings. Closed as completed and parent progress posted. No material design blocker found. #66/#67 must add safe creation publication and cancellation evidence before claiming production changes flow; #68/#69 own client streaming and eventual polling removal. This ticket does not claim measured fanout capacity, durable delivery, or the full epic's server/client cutover.

### #66 implementation and verification evidence

Worker baseline: `6325e493684d3ee4c39a17dbc5e53f19fe7b7bd5`, branch `live-updates`, initially clean. Scope only #66 as sole writer; worker made no commit or tracker mutation. Parent owns commit and Standards/Spec reviews.

User decision: after explanation of the reproduced origin-disconnect gap versus exceptional owned-task panic/abort, the user approved the simplest fail-closed, restart-required safeguard. Ordinary origin disconnects complete write-plus-notify automatically; only unexpected owned-task failure latches delivery closed. Deeper reconnect semantics are deliberately deferred, not a reason to add recovery machinery now.

Implemented:

- Channel operations, not handlers, select `ChannelCreated`. Each attempt constructs the complete normalized channel and serializes `PreparedEvent` before insertion. The success branch synchronously enqueues exactly once before returning that same channel, with no intervening await or serialization. The hub no longer serializes with `expect` after a write. Its prepared bytes remain shared across subscribers.
- Invalid input, case-insensitive duplicate names, SQLite trigger-induced insert failure, real primary-key collisions/retries and five-attempt exhaustion publish nothing. Ordinary failures leave existing streams usable. Startup bootstrap and pre-subscription creations are not replayed. No subscribers is normal; a dropped receiver and a receiver overflowing during 257 real creations do not delay mutations or lose the healthy receiver's changes.
- Concurrent paired-name creates on a temporary file database yield eight confirmations, eight conflicts, and exactly the eight corresponding event identities; tests compare sets/payloads, not database commit order.
- **Actual cancellation gap reproduced before adding lifetime protection.** With the real HTTP/1.1 origin held inside SQLite's commit hook, TCP RST caused Actix to drop its service future. Releasing SQLite committed the channel, verified by ordinary `GET /api/v1/channels`, but the separately connected SSE client received no change. The red log prints confirmation of both request cancellation and the committed HTTP-visible channel before timing out on the event.
- The fix is one channel-owned Tokio task covering write plus notify. Dropping the request's JoinHandle detaches rather than cancels it. A narrow `PendingChannelWrite` drop guard supervises the uncertain DB/notify interval even when no HTTP waiter remains. It disarms after a known insert error or successful notification; no generic mutation coordinator, global write gate, transaction framework, or outbox was introduced.
- Unexpected task panic/abort **latches the shared hub closed until process restart**. This is a delivery-only safety latch; HTTP reads/writes remain available. Existing subscribers are woken/ended, and new subscribers end before readiness. Merely ending old streams is unsafe because SQLite can still commit *after* a fresh reconnect baseline. The owned-lifecycle test explicitly aborts a task inside SQLite's commit hook, proves old/fresh delivery is closed before release, then proves the channel committed after abort. A separate post-commit panic test proves HTTP 500, both existing streams ending, fresh delivery closed, and the authoritative read exposing the committed channel. Ordinary origin disconnects are not this exceptional case: the owned task survives and publication succeeds.
- The tiny random-ID/post-commit-fault seam is `cfg(test)` task-local control under `server/src/channels/tests/`. Registered production routes propagate it only in unit-test builds; no production configuration, public injection API, or global mutable test selector exists. External integration tests use the uninstrumented production build. Fault hooks never log credentials or entity payloads.
- No message publication, desktop cutover/polling changes, native automation/keyring access, migration, human documentation, README or legacy changes. No dependency/lockfile changes were needed; all Cargo commands retained `--locked`.

#### Installed cancellation evidence and source inspection

Versions read from `server/Cargo.lock`: Actix Web **4.15.0**, Actix HTTP **3.13.6**, SeaORM **2.0.3**, SQLx/SQLx SQLite **0.9.0**, Tokio **1.53.1**. Inspected source root: `/home/reno/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`.

| Installed source | Relevant observed fact |
| --- | --- |
| `actix-http-3.13.6/src/h1/dispatcher.rs:1212-1240,1326` | A read error such as a reset with no new partial read propagates through the dispatcher, dropping its service future. FIN/half-close is a different policy and is not evidence of immediate cancellation. The test's middleware drop guard confirms non-completion while the DB barrier is still held. |
| `sea-orm-2.0.3/src/driver/sqlx_sqlite.rs:162-173` | SeaORM acquires a SQLx connection and directly awaits query execution; it does not own an independent publication lifetime. |
| `sqlx-sqlite-0.9.0/src/connection/mod.rs:51-59,331-341,470-497` | SQLite is driven by a separate worker thread. The commit-hook callback is inside the database step; SQLx negates its Rust boolean so `true` permits commit. `lock_handle` installs hooks safely without concurrent raw-handle access. |
| `sqlx-sqlite-0.9.0/src/connection/worker.rs:164-205`; `connection/execute.rs:71-124`; `statement/handle.rs:429` | The worker steps SQLite before sending the result to the async receiver. Dropping that receiver can prevent observing the result without preventing the already-running autocommit. |
| `tokio-1.53.1/src/runtime/task/join.rs:18-36,357-364`; `src/net/tcp/stream.rs:1325-1354` | JoinHandle drop detaches; `set_zero_linger` makes socket close an actual abortive TCP reset. Used without new dependencies. |

Transport test `server/tests/channel_disconnect.rs` starts a two-worker loopback server and separate authenticated origin/subscriber TCP connections. All five file-database pool connections are acquired before installing the one-shot commit barrier, so the insertion cannot accidentally use an uninstrumented connection. Hook entry and service-future drop are explicit signals; only after confirmed cancellation is the hook released. The subscriber must receive the same entity exposed by authoritative HTTP reads. No timing sleep guesses or in-process timeout stand in for transport cancellation. Five consecutive repeat runs passed after the fix.

Official references searched/fetched and read: [SQLite commit hooks](https://www.sqlite.org/c3ref/commit_hook.html) (non-reentrant, before commit, no database calls from the hook); [Tokio 1.53.1 JoinHandle](https://docs.rs/tokio/1.53.1/tokio/task/struct.JoinHandle.html) (detach, panic, abort). SQLx 0.9.0 docs.rs fetch reported a failed documentation build; installed source, not search snippets, supplied its exact API/return semantics.

#### Exact red/green and characterization commands

All commands below ran from the repository root. Red failures preceded their respective implementation changes. Logs are `/tmp/hamlet-epic-71/66-*`; this ledger preserves the results independently of temporary logs.

| Slice | Exact command | Result |
| --- | --- | --- |
| 01 red | `cargo test --manifest-path server/Cargo.toml --locked --test channel_publication channel_creation` | Committed HTTP creation produced no event; bounded SSE frame timed out (`66-01-red.log`). |
| 01 green | `cargo test --manifest-path server/Cargo.toml --locked --test channel_publication` | Multiple subscribers receive matching payload exactly once (`66-01-green.log`). |
| 02 red | `cargo test --manifest-path server/Cargo.toml --locked --test channel_disconnect -- --nocapture` | Real RST canceled the request; HTTP read confirmed commit; other real SSE connection timed out (`66-02-red.log`). |
| 02 green | Same exact disconnect command, then `cargo test --manifest-path server/Cargo.toml --locked --test channel_publication` | Owned task publishes after canceled origin; fanout regression passes (`66-02-green.log`). |
| 03 red | `cargo test --manifest-path server/Cargo.toml --locked --lib channels::operations::tests::unexpected` | Injected post-commit task panic returned 500 but left subscriber waiting as healthy (`66-03-red.log`). |
| 03 green | Same exact owner-test command, then `cargo test --manifest-path server/Cargo.toml --locked --test channel_disconnect` | Existing/fresh delivery closes safely on unexpected failure; normal canceled-origin delivery still passes (`66-03-green.log`). |
| 04 characterization | `cargo test --manifest-path server/Cargo.toml --locked --test channel_publication rejected` | Invalid/duplicate/failed/no-subscriber paths pass without further production change (`66-04-characterization.log`). |
| 05 characterization | `cargo test --manifest-path server/Cargo.toml --locked --test channel_publication` | All four external publication tests pass, including bounded slow/drop isolation and concurrent paired-name writes (`66-05-characterization.log`). |
| 06 characterization | `cargo test --manifest-path server/Cargo.toml --locked --lib channels::operations::tests::id_collisions` | Deterministic real primary-key retries and exhaustion pass without further production change (`66-06-characterization.log`). |
| 07 characterization | `cargo test --manifest-path server/Cargo.toml --locked --lib channels::operations::tests::aborted` | Explicit owned-task abort closes old/new delivery even when SQLite later commits (`66-07-characterization.log`). |

Development correction: the first transport test draft captured only the lifetime guard's boolean field, generating unused-variable warnings and permitting premature guard drop. Added an explicit `drop(lifetime)` inside the future to capture the whole guard, reran the behavioral red, and recorded **that corrected red** above before implementing lifetime protection. No warning-bearing draft is counted as evidence. Slice 03 added the permanent delivery latch rather than permitting unsafe reconnect after an uncertain abort. No fault serialization format or API response changed.

Intermediate checks passed: `cargo check --manifest-path server/Cargo.toml --locked` after slices 01 and 06; `cargo clippy --manifest-path server/Cargo.toml --locked --all-targets -- -D warnings` after slice 03. Each characterization followed the preceding completed vertical slice rather than pre-writing a speculative full suite.

#### Final verification and handoff

All passed from repository root:

- `cargo fmt --manifest-path server/Cargo.toml --check`
- `cargo fmt --manifest-path server/migration/Cargo.toml --check`
- `cargo clippy --manifest-path server/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo check --manifest-path server/Cargo.toml --locked`
- `cargo test --manifest-path server/Cargo.toml --locked` — **30 passed**, none failed/ignored; all 22 existing tests retained.
- `cargo test --manifest-path server/Cargo.toml --locked --test contract` — **2 passed** (included in the full 30).
- `cargo run --manifest-path server/Cargo.toml --locked --quiet --bin generate-openapi > /tmp/hamlet-epic-71/66-final-openapi.json`, then `cmp server/openapi.json /tmp/hamlet-epic-71/66-final-openapi.json` — identical, no artifact edit.
- Five consecutive runs of `cargo test --manifest-path server/Cargo.toml --locked --test channel_disconnect -- --nocapture`, `cargo test --manifest-path server/Cargo.toml --locked --test channel_publication`, and `cargo test --manifest-path server/Cargo.toml --locked --lib channels::operations::tests` — all **8 focused tests passed** each run.
- `cargo build --manifest-path client/Cargo.toml --locked` — unchanged desktop still builds; no native execution. Full desktop test/clippy was not repeated for this server-only child.
- `git diff --check` — clean.

Final logs: `66-final-{fmt,clippy,check,test,contract}.log`, `66-final-client-build.log`, `66-repeat-{1,2,3,4,5}.log`. Committed as `76b736d`; full-diff parallel Standards and Spec reviews found no material issues. Tracker closure verified and parent progress posted. No material blocker remains. Approved operational safety trade-off: an unexpected write-task panic/abort requires server restart to restore SSE; normal disconnects and normal database errors do not. #67 should reuse pre-write `PreparedEvent` plus feature-owned task/uncertain-write guard, with its own author preparation and real cancellation evidence; do not extract a generic coordinator or activate message events in this child.

### #67 implementation and verification evidence

Parent completion: committed as `e0bc0a0`; complete-diff parallel Standards/Spec reviews found no material issues (one optional test-decoding helper cleanup). Closed as completed with evidence and parent progress posted. Following worker evidence describes the verified implementation before its commit.

Worker baseline: `bd907c26c411be0a1026cedcdd54b0fdaa6a5ade` on `live-updates`, initially clean. Scope only #67, sole writer. Implementation is **verified but uncommitted**; no tracker mutation, branch change, stash/reset, or commit. Parent owns commit and subsequent reviews. This appended checkpoint records implementation progress without rewriting the historical frontier rows.

Approved seams: registered HTTP/SSE routes; real loopback reset with deterministic database barriers; narrow owner-local randomness, clock and exceptional task-lifecycle controls. User explicitly approved reusing #66's restart-required exceptional delivery latch: ordinary origin disconnects finish automatically; unexpected owned-task abort/panic ends old and fresh delivery until restart. No generic coordinator, outbox or global write gate.

Implemented:

- `messages::operations::post` accepts the concrete hub and bearer-validated `UserIdentity`; the handler only extracts and maps responses. Each attempt constructs the full Message (author ID/name, unchanged text, channel/message IDs, creation instant) and serializes PreparedEvent **before insertion**. Successful insertion synchronously enqueues once and returns that same Message, with no post-write author lookup, serialization or await before notification. Existing history still resolves the author's current displayed name.
- The author-window regression uses a disposable SQLite `AFTER INSERT` trigger to replace the author's stored name with invalid UTF-8. The original post-insert lookup/decode returned HTTP 500; the new operation returns 201 and both Alice's and independently authenticated Bob's subscriptions receive the exact response payload from the already-validated author. After repairing the deliberately corrupted fixture, ordinary authenticated history confirms that same committed entity. This is a real database fault/public-behavior test, not a source-code assertion or mock of an internal function.
- Message-specific **real TCP reset** independently reproduced the cancellation gap before lifetime protection: the origin's service future was dropped while SQLite was inside its commit hook; releasing the hook made the message visible through HTTP history, but the separate SSE TCP connection timed out. One message-owned task now retains the write/notify lifetime across the dropped HTTP waiter. An owner-local PendingMessageWrite drop guard reuses the channel safety pattern and existing hub latch; no shared hub changes were necessary.
- Unexpected post-commit panic returns 500, closes both established streams and a fresh HTTP stream, while authoritative history exposes the committed message. Explicit owned-task abort inside the real SQLite commit barrier closes old/fresh delivery **before** the database finishes; releasing SQLite still commits the message, observed through history, and fresh HTTP delivery remains closed. Known insert errors and collision retries disarm the guard and leave delivery usable.
- Invalid whitespace/oversized text/extra request fields, missing channels, trigger-rejected inserts, real ID collisions and five-attempt exhaustion emit no phantom/duplicate events. A later successful create still delivers after ordinary failures. No subscribers is normal and pre-subscription creations are not replayed. Maximum legal 4000-character Unicode/control-character text survives HTTP/SSE identity checks.
- A dropped subscriber and a non-consuming subscriber cannot block 257 bounded real writes or healthy delivery; the slow stream ends on overflow. Sixteen concurrent same-text creations across two channels on a file database produce exactly sixteen unique event/response/history identities, compared as sets rather than commit order. A separate owner-local clock fixture delivers independent creations at `.900`, `.100`, `.900` creation instants, retaining those exact HTTP/event values; history sorts them by descending instant and then descending ID. This is controlled out-of-timestamp-order input, not a claim that the concurrency test forces a particular scheduler/commit order.
- Test controls compile only into the owning module's unit-test build and propagate per task; production route integration tests use the uninstrumented library. No deletion, migration, client streaming/polling change, native execution, real keyring access, README/human-documentation or legacy change. Dependencies, lockfiles, routes, DTO schemas and the checked-in OpenAPI artifact are unchanged.

#### Exact red/green and characterization commands

All commands ran from repository root; logs live under `/tmp/hamlet-epic-71/67-*`. Each behavioral red preceded its production fix. Characterization rows added coverage after the preceding completed slice and required no further production semantics.

| Slice | Exact command | Result |
| --- | --- | --- |
| 01 red | `cargo test --manifest-path server/Cargo.toml --locked --test message_publication author_lookup -- --nocapture` | Real post-insert author-decode fault returned **500**, expected 201 (`67-01-red.log`). |
| 01 green | `cargo test --manifest-path server/Cargo.toml --locked --test message_publication` | Pre-write author/response/event preparation removes the failure window; exact payload reaches both authenticated subscribers (`67-01-green.log`). |
| 02 red | `cargo test --manifest-path server/Cargo.toml --locked --test message_disconnect -- --nocapture` | Middleware proved request cancellation during SQLite commit; history confirmed the committed message; the separate SSE connection timed out (`67-02-red.log`). |
| 02 green | Same disconnect command, then `cargo test --manifest-path server/Cargo.toml --locked --test message_publication` | Owned write-plus-notify task survives origin reset; author/fanout regression remains green (`67-02-green.log`). |
| 03 red | `cargo test --manifest-path server/Cargo.toml --locked --lib messages::operations::tests::unexpected -- --nocapture` | Injected post-commit panic returned 500 but left existing delivery apparently healthy/waiting (`67-03-red.log`). |
| 03 green | Same owner-test command, then `cargo test --manifest-path server/Cargo.toml --locked --test message_disconnect --test message_publication` | Drop guard ends old/fresh delivery safely on exceptional failure; normal origin reset still publishes (`67-03-green.log`). |
| 04 characterization | `cargo test --manifest-path server/Cargo.toml --locked --test message_publication rejected` | Invalid/missing/failed writes emit nothing; no-subscriber success, maximum legal escaped text, and delivery after ordinary failure pass (`67-04-characterization.log`). |
| 05 characterization | `cargo test --manifest-path server/Cargo.toml --locked --test message_publication slow_and_dropped` | 257 bounded writes reach the healthy subscriber, dropped subscriber has no effect, slow receiver terminates (`67-05-characterization.log`). |
| 06 characterization | `cargo test --manifest-path server/Cargo.toml --locked --test message_publication` | All four external publication scenarios pass, including concurrent multi-channel identity sets (`67-06-characterization.log`). |
| 07 characterization | `cargo test --manifest-path server/Cargo.toml --locked --lib messages::operations::tests::id_collisions` | Real deterministic primary-key retries/exhaustion emit no extras; subsequent creation/history agree (`67-07-characterization.log`). |
| 08 characterization | `cargo test --manifest-path server/Cargo.toml --locked --lib messages::operations::tests::aborted -- --nocapture` | Explicit owned-task abort closes old/fresh delivery before SQLite's later commit, then history recovers the entity (`67-08-characterization.log`). |
| 09 characterization | `cargo test --manifest-path server/Cargo.toml --locked --lib messages::operations::tests::independent` | Nonmonotonic creation instants are delivered unchanged, with tied-ID history ordering (`67-09-characterization.log`). |

Intermediate checks passed: `cargo check --manifest-path server/Cargo.toml --locked` after slices 01 and 06; `cargo clippy --manifest-path server/Cargo.toml --locked --all-targets -- -D warnings` after slices 03 and 08. No compilation-error draft was substituted for a behavioral red.

#### Installed-version and cancellation evidence

Re-read `server/Cargo.lock`: Actix Web **4.15.0**, Actix HTTP **3.13.6**, SeaORM **2.0.3**, SQLx/SQLx SQLite **0.9.0**, Tokio **1.53.1**. Toolchain: `rustc 1.95.0 (59807616e 2026-04-14)`, `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`. No dependency versions changed.

Re-inspected installed sources under `/home/reno/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/`: `actix-http-3.13.6/src/h1/dispatcher.rs:1208-1245` propagates reset/read errors (distinct from FIN); `sea-orm-2.0.3/src/driver/sqlx_sqlite.rs:159-173` awaits SQLx execution directly; `sqlx-sqlite-0.9.0/src/connection/worker.rs:163-207` executes SQLite before sending its result; `sqlx-sqlite-0.9.0/src/connection/mod.rs:468-497` documents/implements the commit hook (`true` permits commit).

`server/tests/message_disconnect.rs` starts two real Actix workers and independent authenticated SSE/origin TCP connections. It acquires **all five** file-database pool connections before installing the one-shot hook. An explicit hook-entry signal establishes that SQLite is doing the write; `set_zero_linger` plus socket drop causes actual TCP RST. The middleware drop signal confirms the service future was canceled, not completed, **before** release. The hook runs no database calls; after release, history and the separate SSE connection must expose the identical message. Barriers/transport reads have finite time bounds; no timing sleep guesses substitute for DB entry or cancellation. All five repeated final runs passed.

Primary references searched/fetched/read: [SQLite commit hooks](https://sqlite.org/c3ref/commit_hook.html) (before commit, non-reentrant, no database work from callback) and [Tokio 1.53.1 JoinHandle](https://docs.rs/tokio/1.53.1/tokio/task/struct.JoinHandle.html) (drop detaches, panic is captured, abort is asynchronous). One Tokio fetch initially failed in the fetch tool; retry succeeded. Installed source and behavioral red/green, not search snippets, establish the exact current stack behavior.

#### Final server-to-desktop gate

All passed from repository root:

- `cargo fmt --manifest-path server/Cargo.toml --check`
- `cargo fmt --manifest-path server/migration/Cargo.toml --check`
- `cargo clippy --manifest-path server/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo check --manifest-path server/Cargo.toml --locked`
- `cargo test --manifest-path server/Cargo.toml --locked` — **39 passed**, none failed/ignored; all 30 prior tests retained.
- `cargo test --manifest-path server/Cargo.toml --locked --test contract` — **2 passed** (included in full 39).
- `cargo run --manifest-path server/Cargo.toml --locked --quiet --bin generate-openapi > /tmp/hamlet-epic-71/67-final-openapi.json`, then `cmp server/openapi.json /tmp/hamlet-epic-71/67-final-openapi.json` — identical.
- Five consecutive runs of `cargo test --manifest-path server/Cargo.toml --locked --test message_disconnect -- --nocapture`, `cargo test --manifest-path server/Cargo.toml --locked --test message_publication`, and `cargo test --manifest-path server/Cargo.toml --locked --lib messages::operations::tests` — **9 focused tests passed** each run.
- `cargo fmt --manifest-path client/Cargo.toml --check`
- `cargo clippy --manifest-path client/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path client/Cargo.toml --locked` — **171 passed**, none failed/ignored, including current production-server route/polling journeys.
- `cargo build --manifest-path client/Cargo.toml --locked`
- `cargo fmt --manifest-path protocol/Cargo.toml --check`
- `cargo clippy --manifest-path protocol/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path protocol/Cargo.toml --locked` — **4 passed**.
- `cargo clippy --manifest-path protocol/Cargo.toml --locked --all-targets --features openapi -- -D warnings`
- `cargo test --manifest-path protocol/Cargo.toml --locked --features openapi` — **5 passed**.
- `git diff --check` — clean.

Final logs: `67-final-{fmt,clippy,check,test,contract,protocol}.log`, `67-final-client-{fmt,clippy,test,build}.log`, `67-repeat-{1,2,3,4,5}.log`; temporary comparison artifact `67-final-openapi.json`. Source changes are limited to message operation/handler wiring and three message-owned test files; generated server documentation and this appended ledger describe the now-complete creation publication contract.

Handoff: no implementation blocker found. Parent must commit/review and update the tracker; none of those actions were performed by the worker. #68's server/unchanged-desktop verification gate is green, but no client SSE implementation, native acceptance or measured #70 fanout claim is made. Approved residual risk: exceptional owned-task abort/panic requires process restart for SSE; ordinary origin disconnects finish and publish automatically, ordinary DB failures do not latch delivery, and process-crash loss is recovered by fresh authoritative reads rather than durable replay.

### #68 implementation and verification evidence

Parent completion: committed as `108c8a5`, followed by `ca23c7c` moving cross-suite TCP fixtures into API-owned `tests/support/http.rs`. Initial full-diff Spec passed; Standards reported that required layout violation. Fix reviews on both axes passed. Focused API tests: 39; full desktop tests: 189; fmt, strict clippy, build all passed again after fixture move. Commands: `cargo test --manifest-path client/Cargo.toml --locked api::`, `cargo fmt --manifest-path client/Cargo.toml --check`, `cargo clippy --manifest-path client/Cargo.toml --locked --all-targets -- -D warnings`, `cargo test --manifest-path client/Cargo.toml --locked`, `cargo build --manifest-path client/Cargo.toml --locked`. During cleanup, clippy caught an obsolete import and formatting caught the shortened import layout; corrected before final passing run. Logs `68-fix-*.log`. Tracker closure verified and parent progress posted. Following worker evidence describes pre-commit verification.

Worker baseline: `8b1725a62151aee75be3ebe76ff5f01f245bf3bd` on `live-updates`, initially clean. Scope only #68, sole writer. **Verified, uncommitted implementation**; parent owns commit, Standards/Spec reviews and tracker updates. No commit/tracker mutation, branch change, reset/stash, dependency/lockfile change, server implementation change or #69 activation occurred. Historical frontier/checkpoint entries above are preserved.

The parent verified #63–#67 CLOSED/COMPLETED before this task: complete protocol/SSE/creation/cancellation contract, server 39 tests, protocol 4/5 tests and existing desktop 171 tests/build with all checks. The worker read the full issue, plan, client architecture/instructions, TDD skill/support, glossary and server contract. The requested `server/LIVE-UPDATES.md` path does not exist; the canonical contract is `llm-docs/server/LIVE-UPDATES.md`. Approved seams were the authenticated API with controlled transport/time and real-loopback server routes, with tests kept under `client/src/api/tests/`.

Implemented:

- `AuthenticatedClient::events(&Execution)` starts one immutable session-bound attempt. API code owns URL/GET path, bearer and Accept headers, status/media-type validation, decoding and deadlines. Callers receive only typed Ready/ChannelCreated/MessageCreated values or terminal StreamError outcomes. Raw HTTP and credentials remain private; no Last-Event-ID, reconnect, conversation policy, automatic write retry or streaming/polling coexistence is activated.
- The streaming reqwest client is separate from the ordinary eight-second-total client, while both reuse the same no-proxy, fixed localhost resolution, redirect rejection and default certificate validation policy. Eight seconds from submission cover **connection plus readiness**, including time spent awaiting headers. After readiness, 45 seconds without nonempty body bytes is terminal; comments/partial lines count as progress, empty chunks do not. Controlled tests use the existing execution/time seam, not another scheduler implementation.
- Incremental standard SSE covers initial BOM, every byte split of a Unicode/multiline/CRLF/CR fixture, one-byte delivery, LF/CR/CRLF, comments, multiple data lines, last event-field selection, ignored extension/ID/retry fields and discarded partial EOF frames. Unknown SSE names and valid unknown application types are ignored; extra JSON fields remain accepted. Readiness requires one object before supported creations. Malformed supported JSON/entities and duplicate recognized JSON fields/discriminators terminate rather than disappearing as an unknown event. No serialized sentinel variant was added.
- Channel HTTP creation/list and SSE now share `decode_channel`. SSE messages reuse the HTTP conversion, including IDs/author/timestamp validation; empty message channel IDs are explicitly rejected because there is no request channel to compare against on a community-wide stream. Supported payloads deserialize from original JSON, not a Value that would collapse duplicate entity fields. Ordinary HTTP behavior remains covered by all prior API/feature tests.
- Each frame is limited to **65,536 encoded bytes**, including comments/fields/line endings; accumulation resets per frame, not per chunk. Exact 65,536 succeeds and 65,537 fails for LF, CR and CRLF. Incomplete data/comment/unknown frames and repeated complete comment lines are bounded. Four legal 4,000-control-character server-serialized messages (>96 KiB total) in one chunk succeed independently; 4,000 astral scalars escaped as 48,000 bytes of surrogate pairs also fit and decode correctly. Network/current-chunk allocation and decoded-string overhead are separate from this encoded-frame bound, not claimed as a total memory quota.
- Delivery retains at most **256 supported deliveries (readiness included)**. A full channel terminates immediately with Overflow instead of waiting or silently skipping. The existing execution bridge supplies a separate bounded one-result completion channel; terminal results preempt the backlog and remain observable. Dropping the handle cancels its task/body; canceling only a pending `next()` wait leaves the owned attempt usable. EOF/unavailable transport is not authentication invalidity. No unbounded event bridge is introduced.
- Actual production reqwest/server-route tests establish two authenticated consumers receiving both creation types with payload equality to HTTP writes and no follow-up entity reads/polling. Invalid channels, duplicate channels, invalid messages and SQLite-trigger-rejected inserts emit no phantom events. Maximum legal 4,000-character Unicode/control/escaping text preserves identity. A revoked credential is rejected at a fresh handshake; real redirects never reach the destination with the bearer. Actual TCP body cleanup occurs on handle drop. A real SSE body remains usable after **8.2 seconds**, while ordinary HTTP's incomplete JSON body still hits its **eight-second total timeout**.
- Only API code/tests and generated client documentation/this ledger changed. No native UI/keyring access, human/README/legacy edits, server latch changes, migration, dependency update, #69 lifecycle or #70 measurement claim. Narrow documented dead-code/export allowances keep the unactivated API compiled until #69.

#### Exact red/green and characterization commands

All commands ran from repository root. Table filters are appended to **`cargo test --manifest-path client/Cargo.toml --locked`**. Each red preceded its production implementation. Logs: `/tmp/hamlet-epic-71/68-NN-{red,green,characterization}.log`.

| Slice | Exact filter / command suffix | Red observation | Green / characterization result |
| --- | --- | --- | --- |
| 01 | `api::events::tests::ready_is` | Missing stream operation/types/adapter | Immutable clone/new-session URL, GET, bearer, Accept and no-replay binding; ready delivery passes. |
| 02 | `api::events::tests::handshake` | No terminal result for rejected headers; attempted to wait on body | Status and media-type checks terminate without consuming body; `api::events::tests` passes 2 tests. |
| 03 | `api::events::tests::creations` | Missing typed creation variants | Both creations, escaping/additional fields and shared HTTP validation pass; green `api::` passes 24 tests. |
| 04 | `api::events::tests::standard_sse` | CR/BOM/multiline fixture produced no ready delivery | Every byte split and incomplete EOF scenario passes; green `api::events::tests` passes 4 tests. |
| 05 | `api::events::tests::unknown` | Unknown application type prevented the following supported delivery | Unknown/extra compatibility, malformed supported payloads, readiness ordering and sticky terminal cleanup pass; green owner suite passes 5 tests. |
| 06 | `api::events::tests::frame_limit` | Oversized incomplete frame remained live without terminal result | Per-frame limit and >64-KiB multi-frame chunk pass; green owner suite passes 6 tests. |
| 07 | `api::events::tests::delivery_is` | Delivery 257 waited; caller still received queued creation instead of failure | Capacity 256 succeeds, overflow preempts backlog and drops body; green owner suite passes 7 tests. |
| 08 | `api::events::tests::connection_and` | Eight-second pending connection/readiness remained live | Shared submission deadline, delayed headers and heartbeat-without-ready cleanup pass; green owner suite passes 8 tests. |
| 09 | `api::events::tests::idle_deadline` | No terminal result after 45 seconds without bytes | Repeated heartbeat progress across total durations >8 seconds, partial comment progress and empty-chunk nonprogress pass; green owner suite passes 9 tests. |
| 10 | `api::events::tests` | Characterization; no production fix needed | 11 tests pass, including handshake/body drop cancellation, canceled next-wait reuse, EOF and transport error distinctions. Strict intermediate clippy also passes. |
| 11 | `api::events::http_tests` | Real-transport characterization | 3 tests pass: two authenticated real-server consumers/failures/escaping, no redirect forwarding, >8.2-second body and actual TCP cleanup. |
| 12 | `api::events` | Additional characterization | 16 tests pass: isolated initial ready validation/extra fields, one-byte input, 48-KiB surrogate escapes, unchanged ordinary eight-second body timeout. |
| 13 | `api::events::tests::malformed_known` | Duplicate entity ID collapsed through Value and yielded ChannelCreated | Original-JSON shared wire deserialization rejects duplicate recognized entity fields. |
| 14 | `api::events::tests::exact_frame` | Exact-bound characterization | 65,536 succeeds / 65,537 fails with each of LF/CR/CRLF; next frame resets the budget. |
| 15 | `api::events::tests::malformed_known` | A duplicate type discriminator ending in an unknown tag hid a supported event | Original-JSON discriminator validation terminates instead of ignoring the malformed supported envelope. |

Development corrections (not counted as behavioral acceptance): the first test helper assumed unavailable BackgroundExecutor blocking methods; replaced it with scheduler drain plus one finite poll of the public `next()` future. The first network fixture attempted to run GPUI's deterministic scheduler from Tokio worker threads; its explicit thread-affinity guard rejected this. Network tests now use the existing controlled scheduler on a current-thread Actix runtime while exercising the unchanged production reqwest adapter/binding/parser. The initial ordinary-timeout fixture blocked that current-thread runtime by joining a server thread before Hyper could finish cleanup; awaiting a blocking join lets transport cleanup run. These fixture corrections required no production-runtime changes. The successful bounded commands/results above were rerun after correction.

#### Installed transport evidence

Read/fetched [official reqwest 0.12.28 ClientBuilder documentation](https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html), plus installed source under `/home/reno/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/reqwest-0.12.28/`:

- `src/config.rs:69-79`: an absent request timeout falls back to the client's timeout; clearing `Request::timeout_mut()` is **not** a streaming escape hatch.
- `src/async_impl/client.rs:299-314,2629-2635`: default timeout is absent; configured total timeout is installed from resolved request/client configuration.
- `src/async_impl/response.rs:315-331`: incremental `chunk()` reads body data without full-body collection or enabling the separate stream feature; response construction retains total/read timeout wrappers.

The parent had already read the official [WHATWG SSE standard](https://html.spec.whatwg.org/multipage/server-sent-events.html) and supplied the UTF-8/BOM/CR/LF/multiline/comment/EOF rules. This implementation follows those rules and links the authoritative source in generated client documentation. No external snippet was treated as an instruction or sole API evidence.

#### Final verification and handoff

All passed from repository root:

- `cargo fmt --manifest-path client/Cargo.toml --check` (after `cargo fmt --manifest-path client/Cargo.toml`)
- `cargo clippy --manifest-path client/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path client/Cargo.toml --locked` — **189 passed**, none failed/ignored; all 171 prior tests plus 18 new stream tests.
- `cargo build --manifest-path client/Cargo.toml --locked`
- `cargo fmt --manifest-path server/Cargo.toml --check`
- `cargo fmt --manifest-path server/migration/Cargo.toml --check`
- `cargo clippy --manifest-path server/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path server/Cargo.toml --locked` — **39 passed**, none failed/ignored.
- `cargo test --manifest-path server/Cargo.toml --locked --test contract` — **2 passed** (included in full server count).
- `cargo run --manifest-path server/Cargo.toml --locked --quiet --bin generate-openapi > /tmp/hamlet-epic-71/68-final-openapi.json`, followed by `cmp server/openapi.json /tmp/hamlet-epic-71/68-final-openapi.json` — identical.
- `cargo fmt --manifest-path protocol/Cargo.toml --check`
- `cargo clippy --manifest-path protocol/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path protocol/Cargo.toml --locked` — **4 passed**.
- `cargo clippy --manifest-path protocol/Cargo.toml --locked --all-targets --features openapi -- -D warnings`
- `cargo test --manifest-path protocol/Cargo.toml --locked --features openapi` — **5 passed**.
- Three consecutive additional runs of `cargo test --manifest-path client/Cargo.toml --locked api::events` — **18 passed** each run, including actual >8-second transport/ordinary-deadline tests.
- `git diff --check` — clean. HEAD remains the supplied baseline; dependencies and lockfiles are unchanged.

Final logs: `/tmp/hamlet-epic-71/68-final-client-{fmt,clippy,test,build}.log`, `68-final-server-{fmt,clippy,test}.log`, `68-final-contract.log`, `68-final-protocol.log`, `68-repeat-{1,2,3}.log`. Generated operation documentation: `llm-docs/client/LIVE-UPDATES.md`; architecture adds the API owner without claiming desktop activation.

Handoff: no material blocker found. Parent must review/commit/update trackers; the worker performed none of those actions. #69 can consume `api::{EventStream, LiveEvent, StreamError}`, own/drop one attempt, apply session/attempt identity and readiness/read reconciliation, and replace polling there. Preserve the bounded transport delivery path when forwarding to its coordinator. Exceptional server owned-write panic/abort still uses the previously approved restart-required latch; it was not modified. Native acceptance and measured #70 fanout remain out of scope.

### #69 implementation checkpoint (in progress)

Worker baseline: `4e59b42ae81984352c9c2adf84b5b81710b9d1b6`, initially clean. Sole writer; no commits, tracker mutations, native automation, keyring, server/legacy or human-documentation edits. Read complete #69 issue, plan, root/client instructions, full client architecture, glossary and TDD/tests/mocking guidance. Approved seams: pure owner-local lifecycle/state/coordinator, controlled authenticated API/time, real-route two-client scenarios and semantic headless controls. Prerequisites #64/#68 are verified/closed per parent evidence above.

Implementation proceeds in red/green vertical slices. Polling and its controls are not removed until replacement coverage passes. This appended entry is progress evidence, **not a claim that #69 or the epic is complete**. Logs use `/tmp/hamlet-epic-71/69-*`; later entries record exact checkpoint scope and next steps.

#### #69 checkpoint A — verified foundations, NOT desktop cutover

**Resume required.** Nine red/green slices are implemented and all desktop checks pass, but production still uses polling. The pure live policy is compiled but not instantiated by the coordinator. Do not close #69, claim live synchronization, remove old scenario coverage, or treat this checkpoint as final acceptance. No commits or tracker actions were performed. This is a safe, buildable uncommitted checkpoint rather than an unfinished cutover with broken tests.

Implemented scope:

- `client/src/conversation/live_updates.rs`: owner-local pure connecting/loading-baseline/live/retrying/closed policy. Generation-bearing attempt IDs reject other sessions and abandoned attempts. Readiness is accepted once; channels must complete before the current history can complete recovery; empty channels need no history. Retargeting invalidates only history readiness and preserves the completed channel baseline/attempt. Coordinator request-serial validation remains required, especially selection A→B→A.
- Recovery buffers at most **256 deliveries**, independently of API and executor queues. Overflow is sticky and cannot yield a synchronized baseline. Before/during-read creations drain only after the current baseline is complete. `finish_baseline()` drains for synchronous merging but deliberately does **not** mark live: call `synchronized()` only after shared entity reconciliation succeeds. That acknowledgement alone resets backoff. The coordinator must not yield between drain, merging and acknowledgement.
- One pure retry path takes supplied monotonic time and a full-range `u32` jitter sample. Exponential base is **1, 2, 4, 8, 16, 30 seconds**, then capped; equal jitter chooses `[base/2, base]`. Duplicate failure signals cannot extend the retry deadline. Readiness or drained-but-unreconciled baselines do not reset failure count. `retry()` yields one new attempt only when due. Minimal status strings are tested, but the production status still comes from polling until cutover.
- `client/src/conversation/delivery.rs`: the **actual existing coordinator bridge is now bounded**, replacing its unbounded async channel. Ordinary deliveries have capacity **256**; a separate **one-result terminal lane** is polled first. Existing HTTP completions await capacity, preserving their results. Future stream forwarding must use `try_send()` and end the attempt through `send_terminal()` on Full; it must not await ordinary event capacity or silently continue after dropping a creation. The terminal lane is present/tested but not yet fed by a production stream.
- `client/src/conversation/state.rs`: recovery preserves only the selected loaded history as display-only retention while clearing authoritative histories, cursors and older-page state. The view reads through `history_for_display()`; loading, failures, repeated resets and obsolete reads do not blank those retained rows. Accepted replacing history removes retention (including an authoritative empty page); an authoritative empty channel list also clears retention because no history request will follow. Full session clear removes retention. This does not alter polling's ordinary behavior; the coordinator does not yet call recovery reset.
- `client/src/views/conversation/message_history.rs` uses the display accessor for row IDs, layout and rendering. Explicit reset-revision consumption, reconnect notice wiring and Refresh removal are **not implemented** yet. Existing healthy anchoring/select/copy controls still pass.
- Temporary documented dead-code allowance on the staged lifecycle module and the bridge's stream-only producer methods must be removed during coordinator activation. Existing #68 stream allowances and #64 reset/revision allowances remain because their production consumers are still pending. No API operation/server latch/dependency/lockfile changes.

#### Checkpoint A red/green evidence

All filters below append to `cargo test --manifest-path client/Cargo.toml --locked`. Each red was run before its corresponding implementation. Logs are `/tmp/hamlet-epic-71/69-NN-{red,green}.log`.

| Slice | Red filter / observed failure | Green command/result |
| --- | --- | --- |
| 01 | `conversation::live_updates::tests::readiness`: missing lifecycle types/API | Same filter: 1 passed |
| 02 | `conversation::live_updates::tests::creations`: missing bounded creation delivery API | `conversation::live_updates`: 2 passed |
| 03 | `conversation::live_updates::tests::navigation`: missing retarget API | `conversation::live_updates`: 3 passed |
| 04 | `conversation::live_updates::tests::one_capped`: missing retry policy and post-reconciliation acknowledgement | `conversation::live_updates`: 4 passed; strict clippy passed |
| 05 | `conversation::live_updates::tests::failed_attempts`: missing closed-lifetime gate | `conversation::live_updates`: 5 passed |
| 06 | `conversation::state::live_tests::recovery_retains`: missing retained-display accessor | `conversation::state::`: 40 passed; `views::tests::history`: 4 passed |
| 07 | `conversation::delivery`: missing bounded/priority mailbox | Same filter: 1 passed |
| 08 | `conversation::live_updates::tests::connection_notice`: missing persistent status policy | `conversation::live_updates`: 6 passed |
| 09 | `conversation::state::live_tests::recovery_discards`: behavioral red, empty channel baseline retained stale rows forever | `conversation::`: 61 passed, including pre-existing route/polling scenarios; strict clippy passed |

Intermediate `cargo check --manifest-path client/Cargo.toml --locked` ran after slices 02 and 07. Slice 07's check found one test-only import warning; gated it. Slice 08's strict lint found a large synchronous `TrySendError<ConversationUpdate>` result; the bounded producer now returns `TrySendError<()>` (the failed stream event is intentionally not retryable), and slice 09/full strict lint pass. These initial warnings are not counted as passing strict checks.

#### Checkpoint A full verification

All passed from repository root:

- `cargo fmt --manifest-path client/Cargo.toml --check`
- `cargo clippy --manifest-path client/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path client/Cargo.toml --locked` — **198 passed, 0 failed, 0 ignored** (all 189 prior tests plus 9 new tests; polling scenarios deliberately retained).
- `cargo build --manifest-path client/Cargo.toml --locked`
- `git diff --check`

Logs: `/tmp/hamlet-epic-71/69-checkpoint-{fmt,clippy,test,build}.log`. No native automation/keyring access. Server/protocol checks were not rerun for this client-only checkpoint; their sources and dependencies are unchanged. Existing desktop real-route tests still exercise polling; their success is **not** evidence of #69's no-read streaming acceptance.

#### Acceptance mapping at checkpoint A

Criterion numbers follow the 13 checkbox bullets in the full #69 issue.

| Criterion | Current evidence / remaining work |
| --- | --- |
| 1. One session stream and pure lifecycle | Pure policy tested; coordinator stream ownership/activation and lifetime scenarios still missing. |
| 2. Ready/read/buffer/reconcile | Pure gate/bounded buffer/explicit merge acknowledgement tested; full API/coordinator handoff, entities already in snapshot and empty-list integration still missing. |
| 3. Recovery navigation | Pure retarget preserves channels/attempt; actual read cancellation and A→B→A request gates still missing. |
| 4. Common failures/backoff | Pure bounded exponential+jitter and synchronization-only reset tested; all StreamError/baseline/uncached-selection paths still need coordinator routing; older-page local retry remains unchanged. |
| 5. Separate bounds/terminal delivery | API bound exists from #68; read staging exists from #64; recovery policy bound and production bridge bound now tested. Stream forwarding/overflow-to-recovery integration still missing. |
| 6. Identities/cancellation/session ending | Pure generation/attempt rejection and closed gates tested; prior request/session coverage preserved. Stream/read cancellation, queued old-attempt and authoritative stream rejection integration still missing. |
| 7. Recovery/local work/stale display | Existing #64 reset preservation retained; selected stale display now tested and accessor wired to view. Actual coordinator recovery, write-response races and eventual synchronization still missing. |
| 8. Event merge/order/reset revision | #64 shared merge/ordering coverage retained. No events applied by coordinator; reset revision not yet consumed by history presentation. |
| 9. HTTP/event/read/reset races | Not cut over; polling confirmation/catch-up still active. Must replace with direct shared merges and retain operation identities across attempt resets. |
| 10. Outage writes/uncertainty | Existing operations/local work coverage preserved, but no live recovery integration; uncertain-send read scheduling still must be removed. |
| 11. Focus/polling/Refresh/timers | Not cut over: all polling/manual Refresh/catch-up remains intentionally. Expiry coverage remains green. |
| 12. Minimal connection UI | Pure strings/status persistence tested; workspace wiring and semantic view assertions still missing. |
| 13. Replacement coverage/full checks/no-read second user | Current checkpoint checks pass and old coverage is intact. Real-route no-read two-client plus controlled live coordinator/headless replacement scenarios remain required. |

#### Exact resume frontier

1. Start with a test-first **controlled API/coordinator stream-first scenario** under `client/src/conversation/tests/`; shared fixtures go in that owner's `tests/support/`, not production or sibling files. Reuse API's existing `StreamAdapter`/`StreamResponse` test seam and `Execution::controlled`. The current coordinator fixtures only substitute ordinary requests; they must supply stream readiness explicitly during migration. Do not introduce a separate test-only workflow.
2. Replace `Coordinator.polling` with `LiveUpdates`, own one stream-forwarding `Work`, and add attempt-bearing stream/read updates. Use the new bounded mailbox: readiness/creations via `try_send`; stream terminal/bridge overflow via `send_terminal`. On failure, cancel stream + channel/history reads, invalidate read identities and call `reset_for_recovery`, **not** `clear`; leave `create_task`, sends, operation IDs and local work intact. Check expiry/rejection via existing session-ending behavior. Retain the timer for expiry/retry only. Generate jitter at the execution boundary and supply it to the pure policy.
3. Dispatch channels only after `ready`; accept their serial/attempt before updating policy. Dispatch only the current selection's newest baseline. Recovery selection changes cancel/invalidate only selected history, with no repeated channels after success. Do not leave a `Load::Loading` request marked pending if navigation/create occurs before readiness and its read is intentionally deferred; adjust the owner-local selection/read transition as necessary. Gate queued late results before handling auth errors. Preserve writes' generation/operation identity independently of stream attempts.
4. On baseline completion, merge the bounded drained creations using shared state entity merges, check all reconciliation-overflow flags, and only then acknowledge `synchronized`. Healthy creations use the same merges, never follow-up reads; unloaded channels allocate no history and remote channels never select.
5. Test-first remove polling-specific state: catch-up traversal, Refresh state/entry points, confirmation-boundary queues, confirmed/uncertain follow-up schedulers. HTTP confirmations must merge directly, preserve confirmations across recovery/read races, clear only their own draft/form, and never let matching text/events settle uncertainty. Keep older-page cursor merges/local retry and meaningful session/draft/pagination coverage.
6. Wire lifecycle status to workspace, remove both Refresh controls/manual entry points, ignore focus for connection lifetime, and consume `recovery_reset_revision()` in MessageHistoryView even when entity IDs are unchanged. Cover healthy anchoring versus recovery reset through semantic headless controls. Remove temporary lifecycle/bridge/API/reset dead-code allowances once consumers exist.
7. Add real-route two-authenticated-client coordinator scenarios proving creations appear without another healthy-stream read, then full failure/bounds/session/view-lifetime acceptance coverage. Migrate old polling tests rather than simply deleting their useful draft/storage/uncertainty/pagination assertions. Run focused/check/clippy during slices and all four locked desktop verification commands before final completion. Append new evidence to this ledger; do not rewrite historical results.

Parent handoff: all changes are uncommitted on the supplied branch/baseline. This checkpoint needs continued implementation, not a completion review/issue closure. The approved restart-required exceptional server latch is unchanged; no design blocker requires user input.

### #69 desktop cutover — implementation evidence

Parent completion: committed as `7c75c3b` plus `6671da9` review fixes. Initial full-diff Spec review found permanent confirmed-message retention violating newest-only recovery; regression-first fix retires reconciled overlays. Standards shared view-support placement fixed; inert focus interface removed. Complete fix reviews on both axes passed. All186 desktop tests and required checks passed again. Tracker CLOSED/COMPLETED verified and parent progress posted. Following entries preserve pre-commit worker evidence.

Continued checkpoint A as sole writer on unchanged HEAD `4e59b42ae81984352c9c2adf84b5b81710b9d1b6`. This entry supersedes **the resume frontier**, not the historical evidence above. Production now uses the authenticated stream; this is no longer a foundations-only checkpoint. No commit, tracker mutation, server/legacy/native/keyring/README change, dependency change or lockfile change was made. Ready for the parent's independent reviews and commit; this worker did not close #69.

#### Implemented cutover

- `conversation/mod.rs` owns one cancelable stream forwarder and executes `LiveUpdates`. Ready precedes channel/history reads; channel and history completions carry attempt plus request/session identity. Buffered creations are synchronously reconciled before acknowledgement. Remote entities merge directly without healthy-stream follow-up reads or selection changes.
- Failed attempts cancel stream/channel/history work and invalidate read identities, **not writes**. Required baseline/uncached selection failures, EOF, transport/parser/deadline failure and overflow share one capped exponential/equal-jitter retry path. Jitter is supplied at the execution boundary; controlled execution is deterministic. The remaining one-second timer serves expiry and recovery only. Focus is inert for connection lifetime.
- Navigation during recovery retargets only history, including A→B→A. Before readiness/channel completion, deferred history is not left stuck Loading. Already-successful channels are not read again. Empty authoritative channels synchronize without history. Queued obsolete read rejection is checked before authentication handling; authoritative current stream/read rejection and expiry still close the session.
- API delivery, executor bridge, recovery buffering and replacing-read staging have independent bounds. The bridge terminal lane is prioritized; stream events never await capacity. Overflow abandons immediately, including staging overflow while an uncached selected history is loading. HTTP completions can wait for capacity and preserve outcomes.
- Direct HTTP confirmations and events share entity merges. Pending writes and confirmed/uncertain outcomes survive recovery; new sends/creates remain available during outages. Only their originating confirmed operations clear drafts/forms. Uncertainty causes no history read, reconnect or resend; matching text cannot settle it. Older-page cursor merging and local retry remain.
- Recovery retains only selected display rows, invalidates server history/cursors and signals an explicit revision. Navigating away discards that display-only retention rather than making it an inactive cache. History presentation consumes the revision even with identical IDs; healthy event/HTTP insertions preserve anchors. Initial Connecting and persistent Reconnecting notices are wired through the existing workspace status surface; initial resource failures do not leak a separate failure taxonomy.
- Removed polling module/tests, catch-up traversal, confirmation-boundary/refresh queues and schedulers, both Refresh controls and manual-refresh handle entry points. The pure state's internal `refresh_channels` helper now serves only authoritative baselines; it is not a public/manual refresh workflow. Removed the staged lifecycle/API/bridge/reset dead-code allowances.
- Updated generated client architecture/overview/live-update documentation. Human-authored documentation remains untouched.

#### Red/green and migration evidence after checkpoint A

Commands below use `cargo test --manifest-path client/Cargo.toml --locked`; all log paths start `/tmp/hamlet-epic-71/`.

| Slice | Evidence |
| --- | --- |
| 10 | `69-10-red.log`: stream-first coordinator scenario failed because a snapshot was dispatched before readiness. `69-10-green.log`: production stream/read/buffer/reconcile scenario passed, including healthy/focus no-read assertions. An intermediate compile failure in the bridge test's changed delivery shape was fixed; it is not passing evidence. |
| 11 | `69-11-red.log`: outage/retarget/write race retained the polling catch-up warning. `69-11-green.log`: direct confirmation and recovery scenarios passed. Intermediate check warnings for obsolete scheduling methods were eliminated during removal. |
| 12–13 | `69-12-route.log`: real second-user message/channel arrived without another GET. `69-13-live.log`: 5 live coordinator scenarios passed, including both HTTP/event orders, uncertainty, terminal priority and separate bridge/recovery overflow. These were additional regression proofs, not invented red runs. |
| 14 | `69-14-red.log`: unchanged retained IDs failed to move the history viewport to newest after loss. `69-14-green.log`: explicit revision consumption passed the semantic headless anchor/reset scenario. |
| 15 | `69-15-green.log`: 42 conversation tests passed after obsolete polling/catch-up workflows were removed and useful state/coordinator/route cases migrated. Earlier full-suite migration inventories intentionally failed old polling assumptions; those are not final verification. |
| 16 | `69-16-red.log`: initial pending history incorrectly displayed Reconnecting. `69-16-green.log`: initial Connecting, persistent retained-content notice through failed/retried baseline, draft/form preservation and no Refresh controls passed. |
| 17 | View fixture/scenario migration covered remote channels, fallback selection, focus-independent delivery, direct confirmation anchoring, stream/HTTP race, protected binding and session cleanup. `69-17-views.log` initially had one obsolete binding request-count assertion; switched that scenario to a real uncached channel selection, preserving its credential-binding assertions. Strict clippy passed in `69-17-clippy.log`. |
| 18 | `69-18-red.log`: a live uncached-read staging overflow did not immediately abandon the attempt. `69-18-green.log`: 6 live coordinator tests passed after the shared overflow gate. |
| 19 | `69-19-red.log`: initial required-read failure leaked resource-specific Failed state into recovery UI. `69-19-green.log`: 9 live coordinator tests passed, also covering readiness timeout, EOF/malformed/transport/idle/uncached failures, older-page local retry, and newly issued outage writes. `69-19-clippy.log`: strict lint passed. |
| 20 | `69-20-red.log`: navigating away kept inactive display-only rows as a cache. `69-20-green.log`: all 23 state tests passed after dropping that retention on selection change. Additional queued A→B→A rejection coverage passes in final verification. |

The final suite has **183 tests**, not checkpoint A's 198: obsolete polling/catch-up suites and repetitive polling-specific state tests were replaced/consolidated rather than retained as dead workflows. Preserved/replacement coverage includes session and request binding, independent observers/view recreation, drafts and per-channel pending sends, uncertainty, creation inputs, expiration/rejection, older pagination/local retry, copy/selection and variable-height anchoring. There are no ignored tests or duplicate test-only coordinator dispatch paths. Shared controlled SSE fixtures supply bytes to the same authenticated API binding/parser as production.

#### Acceptance mapping after cutover

| #69 criterion | Evidence |
| --- | --- |
| 1. Session stream/pure policy | `live_updates` pure suite; `live_coordinator::readiness_precedes_baseline...`; coordinator observer/close tests; focus/recreation view scenarios. |
| 2. Ready/read/buffer/reconcile | Readiness baseline scenario includes entities before/during a snapshot and already present in it; empty-channel recovery scenario; explicit merge-before-ack policy tests. |
| 3. Recovery navigation | Outage A→B→A retarget scenario checks canceled reads, one stream/no extra channels; queued same-attempt A→B→A rejection scenario gates obsolete serials. |
| 4. Common failure/backoff | Pure capped/jitter/backoff-reset suite; coordinator terminal/required-read table, initial readiness/baseline failures and older-page local retry. |
| 5. Separate bounds/terminal priority | API #68 suites retained; bounded delivery test; bridge/recovery overflow coordinator scenario; uncached replacing-read overflow; state staging overflow tests. |
| 6. Identities/cancellation/ending | Coordinator queued rejection/close/current rejection/expiry cases; session surviving-handle loss suite; protected-bound-client views; API cancel/drop/binding coverage. |
| 7. Recovery/local work | Outage in-flight and newly issued writes; state reset preservation and inactive-retention disposal; semantic persistent stale-content/draft/form notice. |
| 8. Merge/order/reset revision | Shared state dedupe/parsed-time/ID ordering tests, real-route burst plus older pages, remote-channel selection controls; healthy anchors versus identical-ID recovery reset. |
| 9. HTTP/event/read/reset races | Both event-before-response and response-before-event; confirmation during recovery/read; direct insertion anchor test; real composer race scenario without follow-up GET. |
| 10. Outage writes/uncertainty | New/pending outage writes remain accepted; matching-text creations never settle uncertainty; per-channel send timeout has no read/replay; direct confirmation clears only origin. |
| 11. Focus/polling/Refresh/timers | Removed polling/manual controls/schedulers. Semantic focus test keeps read/stream counts unchanged while unfocused events arrive; expiry/session timer tests remain. |
| 12. Minimal UI | `views/tests/live_updates.rs`: initial Connecting, retained Reconnecting through failure/readiness/loading, draft/form preservation, no Refresh/catch-up controls; clear only after synchronization. |
| 13. Replacement coverage/no-read proof/full checks | Both real-route coordinator tests and real-route view journey pass. The two-user coordinator proof holds application time fixed and asserts unchanged ordinary GET count while both creations appear. All final checks below pass. |

#### Final verification and remaining handoff

All passed from repository root after the last source change:

- `cargo fmt --manifest-path client/Cargo.toml --check` — `69-cutover-fmt.log`.
- `cargo clippy --manifest-path client/Cargo.toml --locked --all-targets -- -D warnings` — `69-cutover-clippy.log`.
- `cargo test --manifest-path client/Cargo.toml --locked` — **183 passed, 0 failed, 0 ignored**, `69-cutover-test.log`.
- `cargo build --manifest-path client/Cargo.toml --locked` — `69-cutover-build.log`.
- `git diff --check` — clean. HEAD remains the supplied baseline.

No implementation frontier is knowingly left open for #69. Remaining parent work: independent standards/spec reviews, any resulting fixes, and the parent's explicitly authorized commit/tracker workflow. Native desktop acceptance and real-wallet/IME/accessibility checks remain outside this consent/scope; automated headless/real-route evidence is not native acceptance. Server/protocol checks were not rerun as standalone suites because their code/dependencies are unchanged; desktop real-route tests did run.

Exact changed paths (including preserved checkpoint A work):

- Production/API/runtime: `client/src/api/client.rs`, `client/src/api/events.rs`, `client/src/api/mod.rs`, `client/src/runtime.rs`.
- Conversation production: `client/src/conversation/mod.rs`, `client/src/conversation/state.rs`, `client/src/conversation/delivery.rs` (new), `client/src/conversation/live_updates.rs` (new), `client/src/conversation/polling.rs` (deleted).
- Conversation tests: `client/src/conversation/tests/coordinator.rs`, `client/src/conversation/tests/state.rs`, `client/src/conversation/tests/live_state.rs`, `client/src/conversation/tests/route.rs`, `client/src/conversation/tests/delivery.rs` (new), `client/src/conversation/tests/live_coordinator.rs` (new), `client/src/conversation/tests/live_updates.rs` (new), `client/src/conversation/tests/support/live.rs` (new), `client/src/conversation/tests/polling.rs` (deleted).
- Cross-feature test support/session tests: `client/src/test_support/mod.rs`, `client/src/test_support/live.rs` (new), `client/src/session/tests/coordinator.rs`.
- View production: `client/src/views/channel_sidebar.rs`, `client/src/views/conversation/message_history.rs`.
- View tests: `client/src/views/tests/bound_auth.rs`, `client/src/views/tests/channels.rs`, `client/src/views/tests/composer.rs`, `client/src/views/tests/composer_lifecycle.rs`, `client/src/views/tests/execution.rs`, `client/src/views/tests/history.rs`, `client/src/views/tests/history_lifecycle.rs`, `client/src/views/tests/journeys.rs`, `client/src/views/tests/login.rs`, `client/src/views/tests/mod.rs`, `client/src/views/tests/protected_binding.rs`, `client/src/views/tests/session_lifecycle.rs`, `client/src/views/tests/workspace.rs`, `client/src/views/tests/live_updates.rs` (new), `client/src/views/tests/polling.rs` (deleted/replaced).
- Generated documentation: `llm-docs/client/ARCHITECTURE.md`, `llm-docs/client/OVERVIEW.md`, `llm-docs/client/LIVE-UPDATES.md`, `llm-docs/implement-epic/renodubois-hamlet-71.md` (this append; historical evidence unchanged).

### #69 review fixes — confirmation retirement, test support, inert focus plumbing

Sole-writer follow-up on committed cutover HEAD `7c75c3b662b4d0dd00f3582ce4bfdd93c7d9a838`. Only #69 review findings addressed. No commit, tracker mutation, dependency/lockfile, server/latch, legacy, human documentation, native automation or real-keyring changes.

#### Precise confirmation-retirement invariant

`Conversation.confirmed` is an unresolved write/read overlay, not a session-long message cache. A valid HTTP confirmation merged directly into `Load::Ready` history (selected or inactive) needs no overlay. A confirmation without authoritative loaded history remains available across failed/overflowed/canceled/obsolete reads, navigation and repeated recovery resets. The originating channel's first current, successful replacing history consumes and merges that overlay, then releases its bodies. Another channel's baseline cannot settle it. Later recovery takes only the new newest page plus genuinely unresolved writes/concurrent creations; reconciled old confirmations are not resurrected outside that page. Pending identities, draft clearing, uncertainty and feedback transitions are unchanged. Inactive unresolved writes remain pending reconciliation until their own first successful baseline or teardown, not as reusable history after reconciliation.

Tests use the existing owner-local pure conversation-state seam. New behavioral regressions cover healthy A falling outside a later newest page; outage A surviving two obsolete-read resets and a failed baseline before reconciliation, then disappearing outside a later page; and confirmations completing while inactive, both loaded/direct-merge and canceled/retargeted-read variants. The pre-existing reset preservation scenario expected a healthy, already-reconciled confirmation absent from its synthetic new snapshot to be resurrected; its expected rows now correctly exclude that entity while preserving pending/outage writes, drafts and uncertainty.

#### Red/green and focused checks

Logs are under `/tmp/hamlet-epic-71/`; focused commands use `cargo test --manifest-path client/Cargo.toml --locked <filter>`.

- `69-review-01-red.log`: `healthy_confirmation_is_not_reinserted_outside_a_later_newest_page` failed with `[100, 10]` instead of `[100]`. `69-review-01-green.log`: passed after stopping overlay retention for direct authoritative merges.
- `69-review-02-red.log`: `outage_confirmation_survives_repeated_resets_only_until_its_baseline_reconciles` preserved the unresolved write correctly, then failed because the reconciled old entity reappeared in the later page. `69-review-02-green.log`: all **51 conversation tests** pass after consuming the overlay at successful replacement, adding inactive/retarget coverage, and correcting the obsolete healthy-confirmation expectation above. Existing staging-overflow, pending identity, duplicate event/HTTP ordering, uncertainty and route scenarios pass.
- `69-review-03-views.log`: all **51 view tests** pass after moving only the new shared `open_controlled`/`open_with_streams` helpers into `views/tests/support/mod.rs`. Parent test-module imports retain suite access; support still calls production `app_shell::open`. An unused parent import exposed by this move was removed before strict final lint.
- Removed the inert `ConversationHandle::set_focused`, the ignored `start` argument, and AppShell's activation subscription/handle field used only by it. Updated callers and misleading coordinator-only focus assertions/names; the real window-focus scenario is unchanged. `69-review-04-focus.log`: actual GPUI window activation/event delivery test passes. `69-review-04-expiry.log`: **7 expiry tests** pass. After import cleanup, `69-review-04-conversation.log`, `69-review-04-views.log`, `69-review-04-session.log`: **51 conversation**, **51 view**, **7 session-coordinator** tests pass.

#### Final verification

All commands passed from `client/` after the last source edit:

- `cargo fmt --check` — `69-review-final-fmt.log`.
- `cargo clippy --locked --all-targets -- -D warnings` — `69-review-final-clippy.log`.
- `cargo test --locked` — **186 passed, 0 failed, 0 ignored**, `69-review-final-test.log`.
- `cargo build --locked` — `69-review-final-build.log`.
- `git diff --check` — clean. No native acceptance claimed.

Exact changed paths for this follow-up:

- `client/src/conversation/state.rs` — retain only unresolved confirmations; consume reconciled overlays.
- `client/src/conversation/tests/live_state.rs` — three retirement/race regressions and corrected healthy-confirmation expectation.
- `client/src/conversation/mod.rs` — remove no-op focus interface.
- `client/src/conversation/tests/coordinator.rs`, `client/src/conversation/tests/live_coordinator.rs`, `client/src/conversation/tests/route.rs`, `client/src/conversation/tests/support/live.rs` — no-argument start callers; replace assertions/names that implied a nonexistent focus seam.
- `client/src/session/tests/coordinator.rs` — no-argument start/no-op removal.
- `client/src/views/app_shell.rs` — remove inert activation subscription and its now-unneeded stored handle; retain session ownership and delivery loop.
- `client/src/views/tests/composer_lifecycle.rs`, `client/src/views/tests/history_lifecycle.rs`, `client/src/views/tests/workspace.rs` — no-argument start callers.
- `client/src/views/tests/mod.rs`, `client/src/views/tests/support/mod.rs` (new) — move shared construction helpers into test-only support without widening production visibility.
- `llm-docs/implement-epic/renodubois-hamlet-71.md` — this append only.

Handoff: verified uncommitted fixes, ready for parent review/commit. No blocker; no broader runtime or recovery redesign.

### #70 integrated verification — verified uncommitted handoff

Baseline `d7dc5754b46aef52ad50c4f9941ec10cc4e84e79`, branch `live-updates`, initially clean. Sole writer; scope only #70. Read full issue/design, historical #63–#69 evidence, repository/component instructions, architectures, glossary, TDD/tests/mocking guidance. Approved seams remain real registered routes/two client coordinators, semantic headless views, controlled policy clocks, deterministic transport barriers, real loopback restart/disconnect and bounded owner-local fanout measurement. No commit, tracker mutation, branch operation, dependency/lockfile change, native automation, real keyring, README/human/legacy edit or production behavioral change.

This append supersedes #70's **Planned** execution status above without rewriting historical evidence. Implementation is verified and ready for parent review; #70/#71 tracker/commit decisions remain with the parent. Complete acceptance mapping, exact commands, version/cancellation findings, measurement method/results and manual limitations are in [`llm-docs/LIVE-UPDATES-VERIFICATION.md`](../LIVE-UPDATES-VERIFICATION.md).

#### New evidence

- `client/src/conversation/tests/restart.rs::real_route_http_event_orders_reconcile_once_without_healthy_reads`: two separately signed-up users/coordinators against real loopback routes. Hold the actual POST response to force event-first; independently hold actual SSE bytes to force HTTP-first. Both message and channel creation exercise both arrival orders. Draft/operation identity, ID deduplication, remote channel/no selection theft/no cache allocation, and **zero healthy follow-up GETs for both clients** are asserted. Local confirmed channel creation deliberately selects its uncached history: exactly one navigation read per local creation is explicitly accounted for, with zero reads/cache allocation/selection change at the remote observer and no extra duplicate-event reads. Test-only transport relay uses a bounded one-chunk queue, finite chunk/read/barrier budgets and explicitly aborted/awaited ownership; it never invents response/event payloads.
- `two_authenticated_coordinators_recover_after_actual_server_restart`: real Actix server stopped and awaited; new listener/AppState/pool/hub at the same origin/database. Both actual TCP streams fail visibly; controlled clocks hold retries while 55 new messages and a channel are created. Exactly two recovery reads/client rebuild newest 50, discard old pages/inactive histories/cursors, preserve selected/inactive drafts and selection, and advance reset revision. New older cursors reach 100 matching rows. Before restart, concurrent Alice/Bob sends and remote channel/unloaded-history events converge without reads. This is an actual server lifecycle restart, not controlled EOF; no external binary/SIGKILL durability claim.
- `server/src/live_updates/tests/transport.rs::measured_bounded_fanout_at_ten_changes_per_second`: **8 healthy TCP readers + 1 deterministically paused registered HTTP body**, two workers, real message routes/file SQLite, **270 changes/27 s**. Every healthy reader receives every matching payload, slow body ends on change **257**, and eventual receiver/retention counts are zero. Measures real monotonic time, not accelerated policy time; mutation-side timing starts at the registered route, not a TCP POST.
- Audited/mapped existing independent literal protocol/HTTP/SSE compatibility fixtures, Last-Event-ID fresh ready/no replay, malformed/unknown/extra-field handling, server publication/nonmonotonic ordering, bounded parser/API/bridge/recovery/staging, baseline/navigation/older-page/write races, pending/uncertain preservation, view/session/credential lifetime and visible status/reset tests. Full suites rerun them. No earlier-child production regression requiring reopening was found.
- Updated only generated client VERIFY/PRESENTATION to remove obsolete polling/Refresh guidance and the whole-body manual proxy incompatible with SSE. Consent-dependent native relay/IME/accessibility/wallet checks remain explicit, not claimed passes.

#### Development and measurement results

Both new client scenarios are characterization tests of prior behavior and passed first execution; no fictitious behavioral red/green claim. Three additional consecutive runs passed both tests. No new behavior needed production changes.

The first fanout run passed payload/lag/retention assertions but failed an immediate zero-receiver assertion after server completion (four worker bodies had not yet been destroyed). A **test-harness** correction waits at most five seconds for actual worker/body cleanup; two subsequent measured runs and the full server suite pass. No production cancellation fix was needed. Failed diagnostic log: `/tmp/hamlet-epic-71/70-fanout.log`; successful captures: `70-fanout-green.log`, `70-fanout-repeat.log`.

Latest successful captured run:

- **27.000806201 s**, **9.999701416 changes/s**, **2,160** healthy deliveries.
- Alternating **128/4,000 Unicode scalars**, **128/7,000 UTF-8 bytes**; complete SSE frames **356–14,228 bytes**; **15,750,720** healthy change-frame bytes, excluding ready/heartbeat/HTTP/TCP overhead.
- Ready times **0.270536–2.635110 ms** (all eight values in report); route-to-complete-frame **p50 2.123006 / p95 6.257348 / max 7.226738 ms**.
- Write response **p95 2.432203 / max 2.974856 ms**; scheduling lateness max **1.442795 ms**.
- Shared retained frame payload peak **256 events / 1,866,752 bytes** (~1.78 MiB), counted once per shared allocation rather than per subscriber. Slow body terminal on **257**, retained count thereafter **0**, receiver count after bounded worker cleanup **0**.
- Earlier successful run: **9.999676811 changes/s**, latency p95 **6.526051 ms**, max **9.274190 ms**, retained peak **1,866,749 bytes**. IDs/timestamps account for small encoded-length variation.

This is not saturation, a total RSS/socket-buffer quota, desktop render latency or verified production capacity. The slow case deterministically pauses an HTTP body; an unread socket may still drain into kernel/Actix buffering. Report separates server 256-event shared retention from client 64-KiB frame and independent 256-delivery/API/bridge/recovery/read-staging bounds, plus the prioritized terminal lanes. 10/s and 256 remain tuning candidates.

#### Final checks

All passed from repository root after source changes:

- `cargo fmt --manifest-path protocol/Cargo.toml --check`
- `cargo clippy --manifest-path protocol/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path protocol/Cargo.toml --locked` — **4 passed**.
- `cargo clippy --manifest-path protocol/Cargo.toml --locked --all-targets --features openapi -- -D warnings`
- `cargo test --manifest-path protocol/Cargo.toml --locked --features openapi` — **5 passed**.
- `cargo fmt --manifest-path server/Cargo.toml --check`
- `cargo fmt --manifest-path server/migration/Cargo.toml --check`
- `cargo clippy --manifest-path server/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path server/Cargo.toml --locked` — **40 passed**, including the new measurement and existing real RST/SQLite-barrier cancellation tests.
- `cargo test --manifest-path server/Cargo.toml --locked --test contract` — **2 passed**, included in the 40.
- `cargo run --manifest-path server/Cargo.toml --locked --quiet --bin generate-openapi > /tmp/hamlet-epic-71/70-openapi.json`; `cmp server/openapi.json /tmp/hamlet-epic-71/70-openapi.json` — identical.
- `cargo fmt --manifest-path client/Cargo.toml --check`
- `cargo clippy --manifest-path client/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path client/Cargo.toml --locked` — **188 passed** (186 existing + 2 integrated), none ignored/failed.
- `cargo build --manifest-path client/Cargo.toml --locked`
- `cargo test --manifest-path client/Cargo.toml --locked conversation::restart_tests -- --nocapture`, plus three repeats without `--nocapture` — **2 passed** each run.
- `cargo test --manifest-path server/Cargo.toml --locked --lib measured_bounded_fanout -- --nocapture` — successful captures above; also passed within the full suite.
- `git diff --check` — clean.

Logs: `/tmp/hamlet-epic-71/70-{protocol*,server*,migration-fmt,contract,client*}.log`, `70-integrated.log`, `70-integrated-repeat-{1,2,3}.log`, measurement logs above. Versions unchanged: rustc/cargo **1.95.0**, Actix Web **4.15.0** / HTTP **3.13.6**, SeaORM **2.0.3**, SQLx SQLite **0.9.0**, Tokio **1.53.1**, reqwest **0.12.28**, GPUI Kit **0.6.1** / GPUI-pre **0.3.4**; Linux **7.2.7-arch1-1 x86_64**.

Cancellation conclusion unchanged: feature-owned write-plus-notify survives ordinary request RST even when SQLite commits after service cancellation. Unexpected owned-task panic/abort retains the approved delivery-only **restart-required latch**, including fresh subscriptions; no deeper redesign. Ordinary DB failure/EOF is not authoritative credential rejection. Existing FIN/heartbeat detection and bounded eventual worker cleanup limitations are documented.

Exact files changed:

- `client/src/conversation/mod.rs` — test-only module declaration.
- `client/src/conversation/tests/restart.rs` — new owner-local real-route orders/restart scenarios and finite helpers.
- `server/src/live_updates/tests/transport.rs` — owner-local bounded fanout/pressure measurement; existing transport test retained.
- `llm-docs/LIVE-UPDATES-VERIFICATION.md` — full acceptance evidence/results and limitations.
- `llm-docs/client/PRESENTATION.md`, `llm-docs/client/VERIFY.md` — correct generated live-update controls/manual guidance.
- `llm-docs/implement-epic/renodubois-hamlet-71.md` — this append, history preserved.

Handoff: verified uncommitted #70 work; no automated blocker found. Parent owns independent review, commits/tracker decisions. Native automation/keyring access and consent-dependent acceptance remain unperformed. No production capacity, external-process crash durability or future deletion compatibility claim.

### #70 Standards follow-up — real-route suite ownership

Baseline `dd21ebc`, initially clean; sole writer. Addressed the review finding that both real-route scenarios belong in the conversation feature's `tests/route.rs`, as required by `client/AGENTS.md` and `llm-docs/client/ARCHITECTURE.md`. This test-move refactor requires no behavioral red; no runtime behavior or production visibility changed.

- Moved `real_route_http_event_orders_reconcile_once_without_healthy_reads` and `two_authenticated_coordinators_recover_after_actual_server_restart`, together with their private helpers, into `client/src/conversation/tests/route.rs`. A byte-for-byte comparison confirmed both moved test bodies and helper implementations were preserved. All four route scenarios/assertions remain.
- Reused the suite-local `CountReads` adapter in the existing second-user scenario instead of retaining its duplicate function-local implementation; its original `reqwest::Client::new()` configuration remains unchanged. No cross-suite helper dependency or new abstraction.
- Removed `client/src/conversation/tests/restart.rs` and its test-only `restart_tests` registration in `client/src/conversation/mod.rs`. Existing owner-child privacy and `#[cfg(test)]` gating remain through `route_tests`.
- Updated current paths/commands and appended results in `llm-docs/LIVE-UPDATES-VERIFICATION.md`.

**Historical evidence annotation:** the preceding #70 paths and command logs intentionally retain `tests/restart.rs` / `conversation::restart_tests`, which were correct when executed. Those scenarios now live in `tests/route.rs` under `conversation::route_tests`; the old filter no longer selects tests. Use the current filter below, which includes the two pre-existing route tests as well as both moved scenarios.

Checks run from repository root, in order, all passed:

- `cargo test --manifest-path client/Cargo.toml --locked conversation::route_tests -- --nocapture` — **4 passed**, none failed/ignored.
- `cargo fmt --manifest-path client/Cargo.toml --check`
- `cargo clippy --manifest-path client/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path client/Cargo.toml --locked` — **188 passed**, none failed/ignored.
- `cargo build --manifest-path client/Cargo.toml --locked`
- `git diff --check` — clean.

Logs: `/tmp/hamlet-epic-71/70-standards-route.log`, `70-standards-client-{fmt,clippy,test,build}.log`. Prior server/protocol measurements/checks were not rerun for this client-only refactor.

Exact files changed:

- `client/src/conversation/mod.rs` — remove obsolete test registration only.
- `client/src/conversation/tests/route.rs` — own all four real-route scenarios and their private helpers; reuse GET counter.
- `client/src/conversation/tests/restart.rs` — removed after move.
- `llm-docs/LIVE-UPDATES-VERIFICATION.md` — current paths/filter and follow-up evidence.
- `llm-docs/implement-epic/renodubois-hamlet-71.md` — this append; historical evidence unchanged.

Handoff: verified uncommitted Standards fix; no blocker. No commit, tracker mutation, server/README/native change, desktop launch or real keyring access. Native/manual limitations and parent review/commit authority remain unchanged.
