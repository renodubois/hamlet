# GPUI client full rearchitecture plan

Status: planned; implementation has not started. This plan covers conversion of the entire existing client, not just extraction of the login form. The stages are buildable migration checkpoints within that full conversion, not separately deferred architecture goals.

Architecture authority: [client-gpui/ARCHITECTURE.md](../../client-gpui/ARCHITECTURE.md). Runtime behavior and previous verification: [README](../../client-gpui/README.md), [PRESENTATION](../../client-gpui/PRESENTATION.md), and [VERIFY](../../client-gpui/VERIFY.md). Historical prototype planning is background, not the current source-layout authority.

## Objective and non-goals

Replace the monolithic root view and flat client modules with the documented views, session, conversation, API, storage, runtime, and theme modules. Preserve all current workflows and security/correctness properties against the unchanged rewrite server.

No UI redesign, new feature, server change, dependency upgrade, credential/configuration migration, or network-protocol change is part of this work. Do not add an offline cache, durable drafts, queued writes, push, token refresh, multiple active servers, or a generic framework. Do not promise memory zeroization or exactly-once message delivery.

## Current implementation map

At planning time, `src/main.rs` has about 1,620 lines before its tests. `Hamlet` combines UI state, rendering, credential workflows, request dispatch, polling, list anchoring, and lifecycle cleanup. Existing pure behavior and adapter code provide useful foundations; retain their algorithms rather than rewrite them gratuitously.

| Current location/responsibility | Target owner |
| --- | --- |
| `main.rs::main` | Startup-only `main.rs` |
| `main.rs::theme`, inline icon choices | `theme.rs` |
| `runtime`, `bounded`, dispatch/executor plumbing | Small internal `runtime.rs`; deadline policy stays with its caller |
| `Hamlet` screen choice and app-level subscriptions | `views/app_shell.rs` |
| Login input entities, signup mode, input subscriptions, form rendering | `views/login.rs` |
| Pane layout | `views/workspace.rs`, `views/conversation/mod.rs` |
| Channel input and list/create controls | `views/channel_sidebar.rs` |
| History focus/list, scroll callbacks, row reconciliation, selection | `views/conversation/message_history.rs` |
| Message-row rendering | `views/conversation/message_row.rs` |
| Composer entity/subscription, keyboard/focus, displayed text synchronization | `views/conversation/composer.rs` |
| `AppSession` transitions | `session/state.rs`, without stored editable form/password copies |
| Auth dispatch, expiry, active context and revocation | `session/mod.rs` |
| `Deletion`, save/restore/deletion identities, storage feedback/retry orchestration | `session/saved_login.rs` |
| `Conversation` and existing reconciliation algorithms | `conversation/state.rs` |
| Channel/history/send dispatch, cancellation, completion routing | `conversation/mod.rs` |
| `Polling`, focus/backoff integration | `conversation/polling.rs` plus conversation coordinator |
| `AuthApi`, `HttpAuth`, URL validation, errors, wire decoding | `api/`, with accurately named server/authenticated client interfaces |
| `User`, `Channel`, `Message`, `Page` server data | `api/types.rs`; do not move workflow state into API types |
| `persistence.rs` worker/provider/configuration | `storage/` |
| Large inline test modules | Owning feature suites and `views/tests/` |

Methods spanning responsibilities must be decomposed, not assigned wholesale. For example, `finish_history` currently applies a response, updates polling, decides subsequent reads, and adjusts the viewport: the first three belong to conversation coordination, the last to the history view.

## Required interface and lifetime decisions

These are design constraints for implementation; exact Rust method/type names can evolve without changing ownership.

### Server and authenticated clients

- One server client binds a validated server URL and reusable transport. Login/signup return an authenticated client/context with public user and expiry information.
- Protected calls take operation arguments only, such as channel ID, cursor, or text. No view or conversation request descriptor carries a bearer token or reconstructs an endpoint.
- The authenticated client remains bound to its original server and credential. A new session gets a new context; clones share the connection pool/context without reading mutable global credentials.
- Restoration holds a candidate privately until current-user verification, identity comparison, expiry checks, and session-generation checks succeed.
- Keep a controlled adapter for request tests. Both adapters use the same bound client interface; do not fake away the very URL/credential binding being introduced.
- API types and errors do not depend on session/conversation implementation. Wire DTOs remain private. Authentication-specific feedback is not used as the generic description of channel/message errors.
- Credential access for saving is restricted to session/storage coordination. Secret-bearing values have redacted/no debug output; no new credential/body logging.

### Session lifecycle

- A session coordinator owns pure session state, current authenticated context, expiry work, and saved-login workflows. It exists for the application lifetime, outside either screen.
- Login view owns editable credentials. Validation consumes submitted values; pending network work retains only what it needs for that operation. Do not keep a second editable password in session state.
- Login/signup success clears/drops the form's password state and activates a workspace for that session generation. Failure leaves recoverable input available. Server changes clear the password and invalidate pending auth/restore work as they do now.
- Logout, expiry, and current-session authoritative rejection synchronously invalidate the active generation, stop protected dispatch, and clear conversation/draft state. Removing a rendered view alone is not sufficient cleanup.
- Cleanup and server revocation then complete asynchronously, with separate outcomes. Capture the old client for revocation before removing it from active use.
- Old-session rejections/results cannot invalidate or populate a new session. Saved-login deletion identities have a separate lifetime and must survive newer authentication without deleting a newer credential.

### Conversation lifecycle

- One coordinator per active session owns the bound client, pure conversation state, polling state, and owned tasks. Channel switches reuse it and preserve per-channel in-memory data/drafts.
- UI actions call narrow operations (select/create channel, edit draft, send, refresh, request older). UI observes feature state without mutating internal maps or passing the whole session model.
- Completion identities include the session and relevant request/channel identity. Eliminate tokens from request descriptors, not their stale-result safeguards.
- Centralize current-session authentication rejection through the session interface. Conversation state must not directly mutate `AppSession`.
- All selected-history triggers use the same coordination path: older pages, manual refresh, focus return, scheduled polls, and safe post-send catch-up.
- Closing/invalidation cancels cancellable reads/timers and makes remaining completions inert. Cancellation or timeout of a write never proves it did not reach the server and never triggers replay.

### Views and notifications

- Child views own their input/list/focus entities and subscriptions. Parent views never read their password, textarea, or list state to drive workflows.
- Login view uses the session interface; authenticated views use the conversation interface. The shell chooses screens and performs narrow cross-feature lifecycle wiring, not endpoint-specific dispatch.
- Shell forwards native window activation; conversation coordination decides polling behavior. Session/storage warnings and retry actions remain available on either screen.
- Notifications update history/composer presentation when feature state changes, including changes caused by timers or authentication invalidation rather than a user click.
- History view distinguishes channel replacement from same-channel insertion. It preserves stable message identity/viewport anchoring and bottom-follow behavior without interpreting pagination continuity.
- Composer treats conversation drafts as authoritative across view recreation/channel changes. Guard programmatic input updates so subscriptions do not overwrite the wrong channel or submit unexpectedly.
- Preserve Kit `Root`, stable semantic control IDs, labels/tooltips, and real keyboard/selection behavior.

### Runtime and storage

- Move existing executor bridging out of views. Inject controllable time/execution for tests rather than duplicating dispatch algorithms under `cfg(test)`.
- Keep pure transition functions supplied with time. Identify each existing deadline before relocating it: HTTP currently uses eight seconds, selected read/send orchestration uses nine-second bounds, and secure-store workflows use a ten-second deadline. Preserve observable timeout semantics, including combined restoration work, rather than silently substituting one universal timeout.
- Future execution must remain nonblocking on the GPUI thread. Preserve nonblocking storage queue submission and dedicated provider work.
- Keep one ordered storage protocol for credential writes, metadata commit/rollback, invalidation, and deletion. `preferences.rs` is not an independently scheduled writer competing with the credential worker.
- Preserve `org.hamlet.gpui.rewrite.session.v1`, account-key construction, config path/format, selected-server identity, and pending deletion records. Do not change URL normalization in a way that silently loses access to existing credentials.

## Behavior-preservation checklist

Use this checklist throughout the migration; test counts alone are not sufficient.

### Authentication and persistence

- [ ] Login/signup validation, editable failure feedback, duplicate-submit prevention, and immediate signup session.
- [ ] Server changes clear sensitive form state and invalidate pending/active old-server work.
- [ ] Remote HTTPS, loopback-only development HTTP, normal certificate verification, no redirects forwarding credentials; preserve existing transport configuration.
- [ ] Startup prefill, verified saved-login restoration, identity mismatch/expiry/rejection versus connectivity retry.
- [ ] Honest memory-only save failures and unconfirmed save/delete timeouts; no plaintext fallback.
- [ ] Save/delete FIFO ordering, rollback behavior, durable deletion intent, cleanup across restart and newer login.
- [ ] Immediate local logout/expiry/rejection cleanup, independent bounded revocation, and distinct already-invalid response.
- [ ] No passwords/tokens in metadata or new diagnostic output.

### Channels and history

- [ ] Initial server-ordered channel list and first selection; empty/loading/failure states.
- [ ] Creation validation/conflict/uncertainty, no automatic replay, selection on confirmation only.
- [ ] Refresh preserves valid selection; disappearing selection and cached fallback remain correct.
- [ ] Selected-channel reads only, cancellable navigation, and stale-result rejection.
- [ ] Newest page, opaque older cursors, overlapping identity deduplication, server order for timestamp ties.
- [ ] Multi-page catch-up establishes continuity; failures retain existing contiguous history and distinguish incomplete state.
- [ ] Variable-height anchoring for prepend and middle insertion; follow only when appropriate, jump-to-latest, plain multiline selection/copy.

### Drafts, sends, and polling

- [ ] Drafts survive channel changes but not session invalidation/restart.
- [ ] Real Enter/Shift+Enter and send-button interaction; native IME remains separately verified/unverified.
- [ ] Only the originating channel's composer locks while its send is pending; other channels remain usable.
- [ ] Confirmed results affect the originating channel and merge by server identity after safe reconciliation.
- [ ] Failed/uncertain sends preserve drafts and warnings; no automatic write retry or text-equality deduplication.
- [ ] Confirmation during polling/catch-up requires the correct fresh read; no duplicate message or hidden intervening messages.
- [ ] History/channel polling remains 3/15 seconds while focused, pauses when unfocused, reconciles on return, and backs off independently up to 60 seconds.
- [ ] Manual refresh, recovery, and focus-return scheduling do not duplicate pending work or erase history/drafts during outages.

## Migration stages

Each stage ends with a working client and migrated tests. Prefer reviewable commits separating mechanical moves from ownership changes. Temporary shims may keep intermediate stages compiling, but list and remove them before completion; do not keep dual state owners or parallel legacy/new clients.

### 0. Establish a reproducible baseline

**Work**

- Inspect current git status and preserve unrelated work. At planning time `.gitignore`, `CONTEXT.md`, and `docs/plans/client-gpui-prototype.md` have pre-existing changes. The only planning addition to `.gitignore` is an exception to track this document; preserve all earlier edits. `CONTEXT.md` and the prototype plan are not migration targets.
- Run the full automated checks from `client-gpui/` and record toolchain, commands, outcomes, and test inventory. `VERIFY.md` records a previous 97-test run, not a fresh baseline.
- Map existing test scenarios to the preservation checklist. Identify private-field-heavy tests that will need rewriting through owned interfaces.
- Capture current UI/control IDs and the saved configuration/provider identity with synthetic fixtures, never real credentials.
- If baseline checks fail, distinguish pre-existing/environment failures from migration regressions before proceeding.

**Gate:** baseline evidence and scenario inventory exist; no current failure is silently attributed to the refactor.

### 1. Establish file ownership and test placement

**Work**

- Introduce module roots from the architecture document. Mechanically move the existing root into `views/app_shell.rs` as an explicitly temporary monolith; slim `main.rs` to startup.
- Extract `theme.rs`, including bundled icon mapping, and the existing execution bridge into `runtime.rs` without changing scheduling behavior yet.
- Move existing pure session/conversation/polling implementations under their target directories, with temporary compatibility imports as needed.
- Relocate root-level GPUI tests to `views/tests/`, grouped by auth, channels, history, composer, polling, saved login, and cross-feature journeys. Keep an explicit legacy fixture only until new ownership replaces it.
- Keep one binary crate unless a concrete test/tool consumer requires a library target; do not add public exports merely to preserve private-field assertions.

**Gate:** behavior and test scenarios remain unchanged. `main.rs` has no application workflow or large test suite. This is scaffolding, not completion of view extraction.

### 2. Build the bound server-client interface

**Work**

- Move server data types and errors into `api/`; break existing `session -> conversation -> session` type coupling.
- Replace misleading `AuthApi`/`HttpAuth` names with server/authenticated client interfaces. Move endpoint operations and private wire representations into their documented files.
- Bind URL once per server client and credentials once per authenticated context. Preserve transport policy, deadlines, error distinctions, and write uncertainty.
- Adapt current session/conversation callers to the bound client. Strip server/token plumbing from protected operation descriptors while retaining generation/channel/request identities.
- Preserve a narrow controlled adapter for tests; keep all real-route and loopback transport coverage. No production default method should panic because a fixture did not implement an operation.
- Add binding checks using two server/credential contexts: calls use the intended origin/token, clones retain identity, and old handles never adopt newer credentials.

**Gate:** all production network construction is under `api/`; protected callers do not pass tokens/URLs. Existing contract/security tests and new binding checks pass against unchanged server routes.

### 3. Separate storage mechanics without changing the protocol

**Work**

- Split the provider/account-key implementation into `storage/credentials.rs` and token-free config/file operations into `storage/preferences.rs`.
- Keep serialized worker ordering and its public outcomes in `storage/mod.rs`. Preserve bounded queue behavior, stale-save invalidation, metadata rollback, and durable cleanup intents.
- Move controlled provider fixtures out of another module's tests into an intentional test-support location if shared.
- Add or retain fixtures proving an existing config/key identity can be read, restored, and deleted by the new implementation, including pending deletion after restart.

**Gate:** no storage format/key/path change; all save/replace/rollback/delete ordering and failure tests pass. Provider timeouts are never reported as confirmed success or cancellation.

### 4. Extract session coordination and the login view

**Work**

- Move authentication request execution, expiry, current authenticated context, and revocation out of the shell into `session/mod.rs`.
- Move saved-login workflow state/methods out of `Hamlet` into `session/saved_login.rs`, retaining cleanup identities independently of screen lifetime and auth generations.
- Reshape `session/state.rs` to accept submitted form values and typed outcomes instead of holding input entities or duplicate editable credentials.
- Create `LoginView`, moving input entities, form rendering, mode switching, subscriptions, and password cleanup together.
- Let the shell observe session transitions, render session/storage feedback and retry actions, and temporarily connect those transitions to the legacy authenticated view.
- Add same-process logout/login, changed-server, failed restore followed by manual login, and late cleanup tests through the new interfaces.

**Gate:** the shell owns no username/password/server input entity and implements no authentication/storage workflow. Login removal does not discard pending cleanup. Existing real-control login/signup/restoration tests pass.

### 5. Extract session-scoped conversation coordination

**Work**

- Move channel/create/history/send dispatch, task ownership, completion routing, and polling integration into `conversation/mod.rs`.
- Keep current history/draft/send algorithms in `state.rs`, replacing mutable `AppSession` access with explicit session identity and rejection outcomes.
- Create an authenticated workspace lifecycle: allocate conversation activity for an accepted session, invalidate it synchronously on session loss, then remove/drop authenticated views and stop timers/tasks.
- Route manual refresh, focus-return, polling, pagination, and send reconciliation through the single coordinator.
- Move scheduling to the shared execution seam, removing separate production/test workflow branches. Controlled timers and delayed responses must still exercise the actual coordinator path.
- Keep the existing authenticated rendering temporarily as a consumer of this interface, rather than also retaining its old dispatch implementation.

**Gate:** no HTTP/task-completion/polling workflows remain in views. Old-session 401s, late creates/sends/pages, focus-return during reads, and confirmation/catch-up races retain their behavior.

### 6. Split the authenticated views

**Work**

- Introduce `WorkspaceView` and the conversation layout view; extract `ChannelSidebarView` with its creation input and subscriptions.
- Extract `MessageHistoryView` with its list/focus/scroll state, height hints, stable-ID reconciliation, selection, and older/refresh/jump controls. Extract message-row rendering without forcing an entity per row.
- Extract `ComposerView` with textarea, keyboard/focus handling, displayed draft synchronization, and pending/error presentation.
- Subscribe each view to the state it renders and issue intentions through feature interfaces. Parent views do not synchronize child input fields or maintain duplicate selected-channel/draft/history state.
- Verify subscription lifetime and initial hydration: cached history and drafts render correctly after channel switches or child view recreation, and invalidation clears hidden controls.
- Preserve Kit `Root`, semantic IDs, accessible names/tooltips, and unchanged basic layout. Move corresponding tests with their owners; cross-view scenarios stay under `views/tests/`.

**Gate:** the shell is composition/lifecycle wiring only. History and composer state belong to child views. Actual wheel, clipboard, keyboard, focus, and pending-channel tests pass without navigating private child fields.

### 7. Remove migration scaffolding and verify the complete application

**Work**

- Remove the original flat modules, compatibility aliases, monolithic `Hamlet`, obsolete cross-module field access, and duplicate request-dispatch paths.
- Retain scenario coverage at the new interfaces. Replace obsolete implementation-only tests once equivalent behavioral coverage exists; do not simply delete difficult tests or keep redundant suites to inflate counts.
- Check module dependencies, public visibility, task teardown, and secret exposure. No feature-specific implementation accumulates in `runtime.rs`, `main.rs`, or `AppShell`.
- Update `README.md` and `PRESENTATION.md` with implemented editing locations and commands. Update architecture status only when the target ownership is actually implemented.
- Append a clearly identified post-migration verification record to `VERIFY.md`; do not rewrite historical results as evidence for the new client.
- Run full automated verification and a consented isolated native Linux smoke against the unchanged server. Record blockers/unverified checks honestly.

**Gate:** all completion criteria below are satisfied, or remaining verification blockers are explicitly reported rather than called complete.

## Regression suite and verification

Run from `client-gpui/` at the baseline and each completed migration stage:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

Run targeted cases while working, then the entire suite to catch cross-feature interactions. Representative current regression anchors (module paths can change; retain the scenarios):

| Risk | Existing scenario anchors |
| --- | --- |
| Pending login and stale authentication | `login_validation_and_pending_button_are_visible_and_inert`, `stale_outcomes_cannot_replace_or_invalidate_newer_session` |
| Saved-login lifecycle across UI and worker | `signup_controls_save_and_restore_through_view_and_controlled_worker`, `headless_logout_dispatches_deletion_warns_and_retries` |
| Credential ordering and replacement | `save_completing_after_invalidation_is_cleaned_before_deletion_reply`, `delayed_failed_deletions_can_retry_without_touching_new_user`, `failed_replacement_preserves_previous_selection_and_token` |
| Form/composer cleanup | `expiry_wipes_hidden_composer_before_a_new_login`, `composer_uses_real_textarea_keyboard_button_focus_and_channel_drafts` |
| Keyboard and timeout behavior | `enter_at_mid_caret_and_after_shift_enter_sends_unchanged_text`, `stalled_send_times_out_and_late_completion_cannot_clear_the_draft` |
| Scroll/selection integration | `production_wheel_requests_older_and_keeps_reader_at_same_viewport_y`, `confirmed_middle_insertion_keeps_reader_anchor`, `production_message_is_selectable_and_copyable` |
| Send/poll races | `polling_and_send_confirmation_share_one_headless_history_without_duplicate`, `second_confirmation_during_catchup_requires_a_new_safe_read` |
| Focus, navigation, invalidation | `poll_catches_up_multiple_pages_without_duplicate_work_and_switch_cancels_late_read`, `focus_return_during_read_catches_up_once_and_logout_stops_selected_reads` |
| Outage/recovery | `focused_polls_pause_resume_and_recover_without_losing_draft` |
| Real server contract | `rewrite_routes_signup_login_logout_and_errors`, `rewrite_history_traverses_multiple_pages_with_timestamp_ties`, `second_user_activity_is_found_by_focused_polling_against_unchanged_routes` |
| Transport safety and uncertainty | `rejects_login_redirect_without_contacting_destination`, `ambiguous_write_timeout_does_not_replay_and_malformed_success_is_uncertain` |

Additional checks specifically motivated by new ownership:

- Bound client server/token isolation across clones, server changes, logout and subsequent login.
- Candidate restoration cannot expose an authenticated workspace before verification; late verification cannot reopen one.
- Dropped/recreated child views hydrate from current feature state without duplicate subscriptions, lost drafts, or extra POSTs.
- Workspace shutdown stops new requests/timers; late results remain inert even if an old client handle remains alive.
- Session/storage feedback and old-account deletion retry remain visible across screen changes and newer sessions.
- The shared execution seam preserves deadlines and deterministic race coverage; no test-only branch bypasses production request coordination.

### Native smoke

Follow the existing isolated two-client procedure in [VERIFY.md](../../client-gpui/VERIFY.md): signup/login, create/discover channel, multiline send, channel drafts, focus polling and burst catch-up, older-page scrolling, selection/copy, outage/recovery, uncertain/pending send during navigation, logout/rejection, and secure save/restart/restore/delete.

Use disposable server data, credentials and profiles; no desktop automation or real keyring access without consent. The rewrite-server source and contract must remain unchanged. Historical native results do not prove new entity/subscription wiring works.

Native IME composition, accessibility/physical keyboard inspection, precise delayed-prepend pixel anchoring, and a locked/slow real wallet were previously outstanding. Keep those limitations explicit unless newly verified. Headless tests are not a substitute and this refactor must not silently claim them resolved.

## Completion criteria

- [ ] Target modules exist with the documented ownership, not merely moved `impl Hamlet` blocks.
- [ ] Adding a view has an obvious home under `views/`; parents compose it through a small interface.
- [ ] `main.rs` is startup only; `AppShell` owns no form fields, viewport internals, deletion queue, or request workflows.
- [ ] Login and authenticated child views own their rendering, local state, and subscriptions.
- [ ] Session and conversation coordinators own their workflows with pure transition/scheduling logic retained separately.
- [ ] All server communication is under `api/`; authenticated callers supply neither bearer tokens nor server URLs.
- [ ] Credential/configuration compatibility and ordered worker behavior are preserved.
- [ ] Runtime integration has one workflow path exercised by production and controlled tests.
- [ ] Preservation checklist and new lifetime/binding scenarios pass; obsolete tests have documented equivalent coverage where replaced.
- [ ] Full format/lint/test/build checks pass; native smoke evidence or explicit blockers are recorded.
- [ ] No server/dependency/product-behavior changes or permanent compatibility architecture were introduced.
- [ ] README, presentation guide, architecture status, and verification record describe the actual implemented client.

## Planning verification

This plan was prepared from the current client source, test inventory, dependency manifest, presentation guide, and recorded verification. No implementation, build, test execution, server operation, or native smoke was performed as part of writing it. Baseline execution is stage 0.
