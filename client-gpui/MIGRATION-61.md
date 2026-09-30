# Final ownership contraction — #61

Pinned start: `093c9ac76e3889d0c87fc5476854d8227741d303` on `rewrite`.
Scope: [#61](https://github.com/renodubois/hamlet/issues/61) only; native acceptance
remains #62. Live issue has no comments and is labelled `ready-for-agent`.
Repo instructions, architecture, approved plan, #47 baseline and all #48–60
migration/removal inventories were read. Parent owns fresh parallel Standards/Spec
reviews after the scoped commit, and any issue action.

## Test contraction decisions and equivalents

The two probe removals and cross-view assertion replacements were documented here
before removing them. The final suite has **158 scenarios**, not 160: no count-padding
suite replaces the two redundant primitives. Every other starting scenario remains
(one renamed). The immutable #47 inventory still describes its original 97-test run.

| Removed/replaced implementation detail | Behavioral equivalent retained at owned seams |
| --- | --- |
| `HistoryProbe`, `varied_height_history_preserves_reader_on_prepend` | `production_wheel_requests_older_and_keeps_reader_at_same_viewport_y`: real wheel, variable-height overlapping prepend, failure/retry, exact same row Y; also `confirmed_middle_insertion_keeps_reader_anchor` |
| `selectable_history_copies_line_breaks` | `production_message_is_selectable_and_copyable`: actual production row, exact multiline drag selection and clipboard through Kit Root; `history_shutdown_clears_selected_text` adds teardown |
| Shell-private selected channel, channel order and empty-history assertions | Real creation/pending controls, bound history request identity, channel bounds/order and empty rendered history in `create_controls_confirm_order_selection_and_empty_history`; real absence of workspace controls after logout/late creation |
| Shell-private history/refresh maps and message counts | Visible row IDs and refresh labels in history/polling/journey tests; multi-page boundary/oldest rows remain visible after navigation; exact identity/count oracles remain in `conversation::state::tests` and `second_confirmation_during_catchup_waits_for_fresh_contiguous_read` |
| Shell-private draft reads during outage | Actual textarea copy through Kit Root in `focused_polls_pause_resume_and_recover_without_losing_draft`; existing composer lifecycle and owned draft tests retain originating-channel assertions |
| `bob_activity_arrives_through_hamlet_poll_at_and_real_rewrite_routes` name | Renamed `bob_activity_arrives_through_scheduled_polling_and_real_rewrite_routes`; same actual timer, real route, two-user and control assertions |
| API-owned manual session/storage workflow fixture | `session::route_tests::signup_uses_shared_persistent_session_and_verified_restore_against_rewrite_routes` now runs production session submission, saving, startup verification, local logout, deletion and revocation through opaque updates, production execution, fake provider and unchanged loopback routes; no manual `finish_restore`, save/delete or revocation dispatch |
| API-owned conversation/polling integration | Two existing real-route scenarios in `conversation/route_tests.rs` now drive `ConversationHandle` with controlled execution/time and opaque delivery. Production coordination owns initial loading, focused polling, catch-up continuation and older-page traversal against unchanged routes; tied timestamps, identity/order/continuity, pagination exhaustion and two-user discovery remain covered |

Pure suites were mechanically relocated to `session/state_tests.rs`,
`conversation/state_tests.rs` and `conversation/polling_tests.rs`. Their test bodies
compare equal to the pinned start after dedenting and canonical `ApiError` renaming.
Session binding transition tests now belong beneath private session state; their
supplied-outcome policy/identity assertions remain, without exporting `AppSession`
or retaining a test-only verification facade. Owned coordinator/control suites
exercise actual execution, deadlines and private candidate publication separately.

No difficult timing, security, race, scrolling, keyboard or storage scenario was
removed. `/tmp/hamlet-orchestrator/review-61/scenario-coverage.json` and
`test-inventory.txt` enumerate the complete starting/final correspondence and all
final names, including all #47 anchors and documented equivalents above.

## Removal inventory closure (#48–60)

| Inventoried migration structure | Final disposition |
| --- | --- |
| Original flat `http.rs`, `persistence.rs`, `session.rs`, `conversation.rs`, `polling.rs` | Removed in earlier stages; canonical directory modules remain |
| `api as http`, `storage as persistence`, `AuthApi`, `HttpAuth`, test `Login` and API legacy adapters | API facade removed in #52; final test-only storage alias removed here. `AuthError` naming aliases replaced by canonical `ApiError`; API data imports no longer route through feature exports |
| Wildcard state exports, test-only `AppSession`/`User`, `ConversationIdentity`/`ReadRequest` exports | Removed; private state imports and explicit observable state types only. Conversation transition methods/request identities are restricted to conversation ownership; session client/save access stays inside session ownership |
| Monolithic `Hamlet`, shell `Deletion`, forms/list/drafts, endpoint dispatch | Responsibilities extracted in #54–60; thin shell renamed `AppShell`. No monolith, deletion queue, form fields, viewport internals or feature request decisions remain in it |
| `views/tests/legacy_fixture.rs`, test-only root constructor, descendant private field assertions | Removed. `views/tests/mod.rs` is an independent sibling suite with production `open`, explicit dependencies, real Kit Root/control observation and controlled wire responses. Redundant `TestAuth`, `fixture_api`, and duplicate auth startup helper removed |
| Root-local timers/dispatch and separate test send/history workflows | Removed by #49/#54/#57. Runtime remains the shared generic bridge; tests vary execution/time or bound HTTP responses, not workflows. No feature dispatch lives in runtime/startup/views |
| Shell-facing saved-login restore/save hooks and generation-gated `client_for` fixture hooks | Saved-login workflow hooks removed in #55; remaining test-only client/verification facade hooks removed here. Session owns accepted context, private candidate and cleanup across screens |
| Unsplit API/storage owners and shared provider hidden in storage tests | Split in #50/#53; provider lives in intentional `test_support/storage.rs`. One HTTP adapter and one ordered worker remain |
| Original list feasibility probes and fixture-only Tokio history bridge | Bridge removed in #49; probes removed here only after documenting production equivalents |
| Obsolete history outcome viewport hint | Unused `prepend` flag removed. History view owns stable-ID reconciliation/anchoring; state still returns meaningful transition counts/next-page outcomes |

## Final ownership, visibility and security audit

- **Startup/shell:** `main.rs` constructs dependencies/window/Kit Root. `AppShell`
  constructs the application-lifetime session, chooses screens, displays shared
  feedback/retries, forwards activation and delivers opaque feature updates.
  It never reads child inputs or interprets HTTP/storage outcomes. Workspace and
  conversation layout compose children; each child owns its entities/subscriptions.
- **Dependency direction:** API, storage and runtime have no view/feature imports
  (including API/storage tests). Pure state/polling have no async, file, GPUI or
  task side effects. API data/errors have a single home; no session/conversation
  type cycle, compatibility exports or original flat modules remain.
- **Single authority:** Session creates one `ConversationHandle` per accepted
  generation. Clones share one owner; `read()` is immutable. Only the coordinator
  mutates drafts/history, dispatches requests and interprets completions. History
  row IDs and composer displayed text are presentation projections, not second
  authoritative stores. All history triggers use the same selected-read path.
- **Secrets/binding:** Protected callers/descriptors contain neither URLs nor bearer
  tokens. The only production `credential_for_session` caller copies an accepted
  credential directly into the ordered storage operation. Session client/save
  methods are not exposed to views. Bound contexts are immutable; candidates are
  private until origin/identity/expiry/generation checks pass. Secret-bearing Debug
  remains redacted/absent; no diagnostic/body logging or plaintext fallback added.
- **Teardown/generations:** Session invalidation/drop closes activity synchronously,
  drops its dispatch client, clears every draft/history/pending state, closes
  delivery and aborts owned read/write/timer waits. Already queued completions are
  inert; cancelled writes are not proof of non-delivery and are never replayed.
  Captured revocation and bounded storage waits may finish independently. Shell
  delivery loops hold weak view handles and exit on channel closure; child tasks
  and GPUI subscriptions are owned and dropped with their entities. Hidden retained
  controls observe invalidation; old children cannot adopt a newer session.
- **Storage compatibility:** `storage/` is byte-identical to the pinned start.
  Service `org.hamlet.gpui.rewrite.session.v1`, exact stored-server account keys,
  config path/fields, omitted pending-intent defaults, FIFO/nonblocking submission,
  rollback and durable cleanup protocol remain unchanged. Preferences are written
  only by that worker, not independently scheduled. Old deletion identities outlive
  screens/generations without deleting newer credentials.
- **Timing/transport:** HTTP 8s, protected read/send 9s per operation/page and
  secure-store 10s remain distinct. Restoration has one combined read-plus-`/me`
  budget. Polling stays focused 3s/15s with independent backoff to 60s. Remote HTTPS,
  loopback-only HTTP, normal certificates, no redirects, no-proxy and explicit
  localhost resolution remain unchanged. No automatic write retry or delivery,
  cancellation, zeroization or native-safety claim was introduced.

Static/byte-comparison audit results are in `ownership-audit.txt`. Remaining uses
of “legacy” in test names/config fixture keys describe **persisted-format
compatibility**, not legacy application scaffolding; these must not be removed.

## Preservation scenarios at the final interfaces

| Risk group | Final coverage |
| --- | --- |
| Login/signup validation, editable failure, duplicate pending, stale auth/rejection/revocation | Session state/coordinator suites; real authentication/login controls; old/current generation and bound-client tests |
| Private restoration candidate, combined deadline, memory-only/unconfirmed storage, old-identity retry across screens/new sessions | Session binding/saved-login suites; execution/storage and saved-login views; real-route session lifecycle; storage protocol/compatibility suites |
| Bound origin/credential isolation and security/deadline/uncertainty | Four API binding scenarios, 16 API route/transport scenarios and protected-binding view journey |
| Ordered channels, creation conflicts/uncertainty, retained selection/fallback, selected-only reads | Conversation state/coordinator and channels/workspace controls |
| Opaque pagination, tied timestamp order, overlap dedup, multi-page continuity, failure/recovery | Conversation state and real-route suites; production history and polling controls |
| Variable-height prepend/middle anchor, conditional follow/jump, plain multiline selection/copy | Four production history controls plus independent history lifecycle/selection suite |
| Drafts, actual Enter/Shift+Enter/mid-caret/trailing newline, per-origin pending locks, uncertainty/no replay, send/poll races | Composer controls/lifecycle, conversation state/coordinator, execution 9s test and cross-feature journeys |
| Workspace shutdown, surviving old handles, child recreation, timer notifications and hidden cleanup | Session-loss/closed-activity tests; login/workspace/history/composer lifecycle suites, queued-Enter guard and expiry/rejection/relogin journeys |
| Focus pause/return, 3s/15s polling, backoff and outage recovery | Pure polling, delayed-tick coordinator, actual controlled timers and real two-user loopback journey |

## Fresh automated verification — 2026-09-30 UTC

Linux x86_64, rustc/Cargo 1.95.0; existing lockfile and build cache. From
`client-gpui/`, all commands were freshly invoked:

| Command | UTC start → end | Outcome |
| --- | --- | --- |
| `cargo fmt --check` | 18:24:30 → 18:24:30 | exit 0 |
| `cargo clippy --locked --all-targets -- -D warnings` | 18:24:30 → 18:24:30 | exit 0 |
| `cargo test --locked` | 18:24:30 → 18:24:39 | **158 passed, 0 failed/ignored** |
| `cargo build --locked` | 18:24:39 → 18:24:42 | exit 0; compiled, not launched |

During contraction: five all-target typecheck checkpoints (final two clean),
view suite (53), route suite (3), session suite (30), conversation suite (40),
state suite (40) and authentication controls (5) passed. Strict preflight Clippy
passed. `SEED=1,2,3` each passed production history (4), polling views (3) and
real-route feature tests (3). `git diff --check` passed.

This is behavior-preserving contraction/mechanical relocation at the pre-approved
seams, not a new product-behavior red/green claim. The first changed view run found
two incorrect test oracles: the semantic composer wrapper does not itself expose
the textarea's aria label. Assertions were corrected to real control presence plus
bound history identities, not by changing production presentation. Intermediate
checks also caught fixture imports, owned storage-feedback types, noncloneable
worker construction and unused generation bindings after facade removal; all were
corrected, no warning waived. Failed and successful command logs are retained.

## Standards P2 correction — 2026-09-30 UTC

Review of candidate `2c8ab83c6974973226982a6a172862ca432ffd7e` found that
relocating the conversation route tests had left independent state/polling and
manual continuation dispatch in those tests. Both now use the already-approved
`ConversationHandle`/execution seam. The host only pumps opaque updates; it never
interprets request identities, HTTP outcomes or next-page decisions. Real bound
HTTP, isolated rewrite routes/database and timestamp-tie fixtures are retained.
Pure supplied-outcome transition tests remain separate and unchanged.

Sensitivity checks omitted production tick polling and catch-up continuation in
turn: each old route test still passed, while its replacement failed. Both final
tests also failed with both omissions, then passed with production restored
byte-for-byte. No production code or new seam was needed. Pagination assertions
now observe 50 initial messages, 155 after the 105-message burst, then all 158 after
requesting the remaining three older messages, with unique identities/texts,
timestamp/ID order and exhausted pagination. Bob's message/channel are discovered
through actual coordinator timers advanced to 3s/15s, not manual refresh dispatch.

Fresh full format/strict-Clippy/test/build gates passed at 18:45:10–18:45:24 UTC;
**158 passed, 0 failed/ignored**, unchanged count. See the appended correction
record in [VERIFY.md](VERIFY.md). Three all-target typechecks, the conversation
suite (40), feature route suite (3), and both corrected route tests at
`SEED=1,2,3` passed. Initial failed test-oracle attempts and mutation logs are
retained separately under `/tmp/hamlet-orchestrator/review-61/correction/`;
earlier verification records and original document snapshots remain untouched.

## Preservation, approved documents and handoff

New user consent explicitly permits committing the entire pre-existing README and
ARCHITECTURE plus required #61 updates. Original contents are captured under
`/tmp/hamlet-orchestrator/review-61/original/client-gpui/`; `original.diff` records
starting tracked changes. `README.md.implementation.diff` and
`ARCHITECTURE.md.implementation.diff` distinguish this implementation's edits from
that approved pre-existing content. Historical README slices, #47 evidence, #48–60
records and all original VERIFY content are preserved; VERIFY only gains a clearly
identified post-migration record.

The four protected files are byte-identical and never staged: `.gitignore`,
`CONTEXT.md`, `docs/plans/client-gpui-prototype.md`, and
`docs/plans/client-gpui-rearchitecture.md`. Their original copies and SHA-256 check
are in the same evidence directory. No server, dependency/lockfile, persisted-format,
control-ID, layout or product feature change. No credentials/profile/keyring access,
desktop automation, issue mutation, push, branch switch, stash or reset.

No implementation blocker. Parent review is pending, not an implementation blocker.
**Native #62 is not performed:** fresh isolated two-client smoke, IME candidate
confirmation, assistive technology/physical keyboard, precise delayed native prepend
anchoring and locked/slow real-wallet behavior remain pending separate consent and
the relevant desktop/devices/provider. Historical native checks do not verify the
new entity/subscription wiring. No packaging/non-Linux acceptance is claimed.

Artifacts against the original pin: `changes.diff`, `commits.txt`, `issue.json`,
`handoff.md`, check logs, complete scenario inventory and ownership/preservation
records under `/tmp/hamlet-orchestrator/review-61/`.
