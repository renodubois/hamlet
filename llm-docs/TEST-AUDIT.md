# Test-value and feedback-loop audit

Audited 2026-10-07, starting from `a60a92b` plus the existing working-tree changes.

## Bottom line

The concern is partly justified: some client scenarios duplicate lower-level coverage, some assertions have outlived the controls they tested, and a few tests no longer prove what their names promise. However, **deleting large numbers of tests is not the best first move**.

The biggest measured slowdown was test-server teardown. In an isolated copy, changing three fixture shutdowns reduced the full client suite from approximately **35 seconds to 8.25 seconds without deleting any tests**. Disabling dialog animations in two unreliable scenarios then produced three consecutive full-suite passes.

Much of the inexpensive state, security, persistence and protocol coverage is worth keeping. Targeted bug injection demonstrated that a tiny state test catches destructive draft loss, and a coordinator test catches an early timeout that two much larger UI tests miss.

## Scope and evidence

- Inventoried all Rust test declarations, including the optional benchmark and protocol feature-gated test.
- Inspected test implementations and their production owners across client, server and protocol; individually profiled every default client/server test case. The ignored benchmark remained ignored.
- Ran the server suite twice, protocol suite twice with all features, and client suite repeatedly.
- Used a temporary source snapshot for teardown, animation and three deliberately incorrect production-code probes. Each production mutation was restored before the next probe.
- Left repository source/tests and pre-existing edits untouched. Only this report was added. Build artifacts were used/rebuilt as usual.
- Automated tests used fake providers, temporary databases/files and ephemeral loopback listeners. No desktop client or real keyring was accessed.

The working tree was being edited during the audit. Initial inventory and timings describe that WIP, not pristine `HEAD`; later experiments used a frozen copy. Line references below identify the original source locations, before probe-only edits. These are warm-cache measurements on this machine, not cold-build benchmarks or CI guarantees.

### Inventory

| Area | Test declarations | Test/support Rust lines, approximately | Assessment |
| --- | ---: | ---: | --- |
| Client API | 40 | 2,145 | Strong boundary/security coverage; some repeated real-server validation |
| Client session | 30 | 1,882 | Valuable stale-result, expiry and saved-login races |
| Client storage | 15 | 719 | High-value persistence and ordering coverage; very inexpensive |
| Client workspace | 38 | 2,400 | Strong state/coordinator coverage; expensive teardown in two route scenarios |
| Client views | 57 | 5,200 | Mixed: genuine UI regressions plus duplicated/orphaned assertions |
| Client theme | 2 | 45 | Small integration smoke checks, not a meaningful speed problem |
| Client cross-feature support | — | 103 | Shared fixtures |
| Server | 38 | 3,767 | Mostly useful HTTP/database/stream behavior; one benchmark ignored |
| Protocol | 13 | 254 | Cheap, independent wire-contract examples; keep |

The client total is **182 tests**. The server normally executes **37**, with one ignored fanout measurement. Protocol executes **13 with all features**, or 12 without `openapi`.

Counts understate scenarios: several functions contain substantial matrices or long workflows. Conversely, test LOC alone is not evidence that a test should be deleted.

### Measured feedback time

| Check | Wall time | Result |
| --- | ---: | --- |
| Server, two warm runs | 7.03 s / 7.03 s | 37 passed, 1 ignored each |
| Protocol, all features, two runs | 0.114 s / 0.115 s | 13 passed each |
| Client, original runs | 34.98 s / 35.64 s | 181 passed, 1 failed each |
| Client, teardown-only snapshot, two runs | 8.25 s / 8.26 s | Each had one dialog-test failure |
| Client, teardown plus reduced motion, three runs | 8.25 s each | All 182 passed each |
| Teardown-only snapshot excluding two wall-clock HTTP probes | 5.63 s | 180 passed; 2 filtered out |

Initial Cargo-reported preparation was 0.41 s for the client and 0.12–0.19 s for the server. The observed delay was execution, not recompilation. Building the temporary client copy took 18.6 s initially; later probe rebuilds took roughly 4–7 s. This does not establish cold-build cost.

Individual-case timings include process startup and must not be added to predict parallel full-suite wall time. The median client case took approximately 16 ms.

## Findings, in priority order

### 1. Fix fixture teardown before removing coverage

**Evidence:**

| Test | Original individual run | Forced fixture shutdown |
| --- | ---: | ---: |
| `workspace::route_tests::second_user_creations_arrive_on_session_stream_without_followup_reads` | 30.78 s | 0.82 s |
| `workspace::route_tests::server_history_traverses_multiple_pages_with_timestamp_ties` | 30.84 s | 0.82 s |
| `views::tests::journeys::bob_activity_arrives_through_session_stream_and_real_server_routes` | 16.38 s | 1.32 s |

The probe changed only `stop(true)` to `stop(false)` at the end of those fixtures:

- `client/src/workspace/tests/route.rs:411` and `:630`.
- `client/src/views/tests/journeys.rs:336`.

These tests assert delivery/history behavior, not graceful server shutdown. Their SSE teardown needlessly waits for network/worker draining. A bounded forced shutdown of the test-owned server preserves their relevant assertions. The route suite already has a bounded non-graceful teardown helper near `route.rs:98`.

**Recommendation:** centralize that fixture cleanup and use it consistently. Keep cancellation/resource-release behavior in its dedicated API/server transport tests; do not replace those assertions with forced shutdown.

### 2. Stabilize modal interaction tests instead of dismissing their coverage

The original full client runs failed `reconnect_retains_rows_draft_and_creation_input_without_http_reads`: expected `unfinished room`, observed an empty dialog input. The same test passed five isolated reruns, and every client case passed when individually profiled.

The teardown-only snapshot also intermittently failed `create_controls_confirm_order_selection_and_empty_history`, observing `Create text channel` rather than `Creating channel…` immediately after a click.

Disabling motion only in those two tests produced **three consecutive 182/182 passes**, at the same 8.25-second suite time. This supports moving dialog hit targets as the cause of those failures; it is not a guarantee that all flakes are eliminated. The new WIP context-menu test already disables motion for exactly this reason.

**Recommendation:** make reduced motion part of ordinary behavioral UI fixture setup. Explicitly opt into animation only when animation is the behavior under test. Retaining dialog input through reconnect is a real user-data regression worth protecting.

### 3. Consolidate the send-timeout UI tests; their exact-time assertion is ineffective

Overlapping scenarios:

- `client/src/views/tests/execution.rs:106`: `send_times_out_at_nine_seconds_without_replay_or_late_draft_loss`.
- `client/src/views/tests/composer.rs:234`: `stalled_send_times_out_and_late_completion_cannot_clear_the_draft`.
- `client/src/workspace/tests/coordinator.rs:162`: per-channel pending sends and timeout behavior.

**Bug probe:** changed `READ_SEND_DEADLINE` from nine seconds to **one second**.

- Both UI scenarios still passed, taking approximately 1.72 s and 0.67 s.
- The coordinator scenario failed immediately, taking 0.016 s, because the originating send was no longer pending at second eight.

In the execution UI test, the second-eight check only asserts that `send-message` is absent. That control was removed altogether, so the assertion does not distinguish pending from timed-out behavior.

The UI tests still protect eventual draft retention, visible uncertainty and rejection of late completions. They are not wholly worthless; they simply overlap and do not establish their claimed exact timing.

**Recommendation:** keep the coordinator's precise timing/per-channel matrix. Merge the two UI scenarios into one wiring check that proves the composer is actually locked while pending, later editable with uncertainty, retains its text, and rejects late success/rejection. Remove repeated assertions about a permanently absent send button; retain at most one intentional check of the Enter-only interface.

### 4. The “unconfirmed at ten seconds” storage UI test no longer checks that claim

`client/src/views/tests/execution_storage.rs:253` contains `delayed_save_and_delete_are_unconfirmed_at_ten_seconds_without_blocking_logout`.

Its storage-status assertions were removed in commit `c40206a`, but the clock advances and name remain. It now checks login/logout visibility, provider serialization and eventual cleanup—not observable unconfirmed status at second ten.

**Bug probe:** extended only save/delete coordination budgets from ten to **100 seconds**, leaving restoration unchanged. This UI test still passed in 0.62 s.

The session-level `delayed_failed_deletion_cannot_remove_a_new_save_of_the_same_identity` did detect the missing deadline outcome. However, after printing its assertion failure, that test did not finish within an external 90-second limit. Failure-path fixture cleanup also needs attention; the exact hang mechanism was not traced.

**Recommendation:** either shorten/rename the UI scenario to its remaining useful property—logout stays responsive during blocked provider work—or replace it with a genuinely observable deadline assertion if that remains a product requirement. Keep detailed save/delete ordering in storage/session tests, and make blocked-provider fixtures panic-safe and bounded on failure.

### 5. Reduce duplicated business-rule matrices at upper layers

`client/src/api/tests/http.rs:440–721` repeatedly starts real servers and checks signup, channel naming, conflicts, message history and revocation. These overlap substantially with `server/tests/{auth,login,channels,messages}.rs`, specialized client adapter tests, and client feature route scenarios.

Some overlap is intentional and valuable: the client must map actual server statuses/entities correctly. But every layer does not need to re-prove the complete validation matrix.

**Recommendation:**

- Server owns exhaustive request validation, SQL ordering/constraints and authorization outcomes.
- Protocol owns independent literal JSON/schema compatibility examples.
- Client API owns request construction, response validation, error mapping, redirects and transport behavior.
- State/coordinators own race, retry, timeout and stale-result matrices.
- Views own input/focus, disabled controls, rendering, copy/selection and notification wiring.
- Real-route journeys establish that these boundaries work together; retain distinct restart/order/pagination scenarios rather than repeating all rejection cases.

Consolidate the broad API happy-path smoke with overlapping focused route cases only after listing their unique conversion/error-mapping assertions. This is primarily a maintenance improvement: most of these individual adapter integrations cost well under two seconds, so deleting them is not the large measured speed win.

### 6. Strip implementation/presentation coupling selectively

Concrete candidates:

- `server/tests/contract.rs:58–108` parses Rust source strings, handler names and the exact count of seven `web::resource` calls. Reformatting or changing registration style can break it without an HTTP contract change. Conversely, its fixed source-file list is not comprehensive discovery of future route modules. Keep OpenAPI artifact drift and actual HTTP/security checks; eventually obtain route inventory from registration metadata or another less source-spelling-dependent mechanism. This is a documented workaround, not grounds to delete contract coverage outright.
- `client/src/views/tests/channels.rs:123–134` mixes creation behavior with button-width and label-alignment checks. Keep channel ordering/selection, but separate optional style checks from feature correctness. Width relative to a hard-coded 24-pixel margin is a design choice, not a channel-creation invariant.
- The WIP `channel_context_menus_are_presentation_only` repeats rename/delete flows across two channels and four dismissal modes—16 dialog cycles. Its individual run took 3.67 s. Preserve the correct-channel prefill and “placeholder actions issue no request” guard, but shrink the cross-product unless each variant has a distinct regression risk. When backend operations arrive, retire the intentionally temporary no-op expectation rather than treating it as a permanent behavior contract.
- The WIP makes workspace `status()` and the `disconnected` flag test-only. Assertions about the old connection-notice text no longer protect production presentation. Keep real retry/read-count/state-preservation assertions and observe connection lifecycle without maintaining obsolete notice-only state.

Do **not** remove all pixel assertions. Reader viewport anchoring, preserved text selection, and a footer staying fixed while channels scroll are observable UI behavior, unlike incidental padding/alignment. Likewise, the two theme checks are only 45 lines and approximately 48 ms combined: pruning them is not a useful feedback-loop optimization.

## Coverage worth explicitly retaining

| Coverage | Value / plausible regression it prevents |
| --- | --- |
| Workspace state and coordinator suites | Losing another channel's draft, stale outcomes changing a new session, duplicate rows, wrong-channel confirmation, retry replaying writes |
| Session lifecycle and saved-login races | Late restoration reopening a logged-out session, old deletion removing a newer credential, expiry leaving protected work active |
| Storage persistence/protocol suites | Plaintext credential fallback, metadata pointing at a failed save, delete/save reordering, cross-account cleanup |
| Client HTTP/SSE boundary suites | Credentials forwarded on redirects, ordinary requests hanging indefinitely, streams dying under an ordinary body timeout, malformed framing/entities, unbounded backlog |
| Server auth, pagination and publication suites | Plaintext token persistence, revoking the wrong session, gaps/duplicates at timestamp ties, phantom events from failed writes, slow subscribers blocking writes |
| Protocol literal wire examples | Client and server changing together to an incompatible external JSON contract unnoticed |
| UI composer/history/lifecycle checks | Enter corrupting text, lost caret/drafts, hidden controls retaining sensitive state, broken copy, prepends moving the reader |
| Timestamp formatting/minute refresh | Local-date/year-boundary mistakes, stale displayed timestamps, refresh disturbing reading position |

**Positive bug probe:** replacing the successful-send cleanup `self.drafts.remove(id)` with `self.drafts.clear()` caused `confirmed_entities_merge_immediately_and_only_originating_draft_is_cleared` to fail in **0.008 s**. This is exactly the kind of cheap test to preserve, even though it exercises a feature-private state module.

Assertions about request counts/order are also justified when they establish a product/security property: no automatic write replay, no polling/catch-up reads, or serialized credential deletion. They are not automatically bad mocking.

## Recommended sequence

1. **Harness-only change:** bounded fixture teardown; reduced-motion behavioral fixtures; failure-safe provider gates. Re-run all original scenarios. The first two changes were validated in the isolated copy with all 182 client tests passing three times.
2. **Small coverage-preserving cleanup:** merge the two send-timeout UI scenarios; shorten/rename the orphaned storage deadline scenario; remove dead-control assertions and incidental geometry from behavior tests. Keep the inexpensive owning-layer matrices.
3. **Selective integration consolidation:** inventory unique assertions in the broad API routes/journeys; move repeated rejection matrices to their owner and retain boundary smoke coverage. Do not impose a test-count reduction quota.
4. **Two feedback lanes:** focused owner tests while editing; full suites before merge. Optionally separate the two real-wall-clock HTTP probes from the rapid lane, but keep them required in the full lane: they catch real reqwest configuration mistakes that controlled-clock tests cannot establish.

Useful current commands, relative to repository root:

```sh
(cd client && cargo test --locked workspace::state::)
(cd client && cargo test --locked workspace::coordinator_tests::)
(cd client && cargo test --locked session::)
(cd client && cargo test --locked storage::)
(cd server && cargo test --locked)
(cd protocol && cargo test --locked --all-features)

# Optional rapid client lane; not a substitute for full validation.
(cd client && cargo test --locked -- \
  --skip ordinary_http_still_has_an_eight_second_total_body_deadline \
  --skip production_stream_has_no_eight_second_total_timeout_and_drop_closes_tcp_body)
```

The rapid-lane timing above assumes teardown improvements; the command alone does not remove the existing graceful-shutdown delay.

For each new test, require a specific answer to: **What plausible bug does this catch that the existing owning-layer tests do not?** Test at the lowest seam that faithfully reproduces it, and add higher-layer coverage only for a distinct boundary or UI risk.

## Limits

This audit does not measure historical bugs prevented, exhaustive mutation coverage, production coverage percentage, cold compilation, or CI flake frequency. Three targeted mutations and a handful of reruns provide specific evidence, not a suite-wide quality score.

Headless tests also do not establish native IME, accessibility, physical keyboard/clipboard behavior or real locked/slow-wallet acceptance. Those remain separate verification gaps under `llm-docs/client/VERIFY.md`'s consent/isolation gate. Deleting headless tests would not close those gaps.

Local logs, individual-case timings and the disposable snapshot were captured under `/tmp/hamlet-test-audit.0lug2n/`; that directory is ephemeral, not a required repository artifact.
