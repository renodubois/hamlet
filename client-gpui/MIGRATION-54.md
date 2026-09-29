# Application-lifetime session checkpoint — #54

Scope: [issue #54](https://github.com/renodubois/hamlet/issues/54), pinned start
`3038737403e4f710b23870de0111fba1f2370519` on `rewrite`. Live body, comments (none)
and label (`ready-for-agent`) were fetched. This extracts session coordination
only: **saved-login workflows (#55) and child login-view ownership (#56) remain
separate**. The approved architecture/plan and prior migration records are unchanged.

## Ownership and lifecycle

- `src/session/mod.rs::SessionCoordinator` is constructed once with the application
  shell, outside either rendered screen. It exclusively owns the pure session
  state, transport, accepted context, generation transitions, authentication
  dispatch/completion, cancellable expiry work and captured-client revocation.
  It neither renders nor depends on views. Dropping it cancels its expiry work.
- The shell forwards form intentions and observes status/lifecycle. Its one GPUI
  delivery loop passes opaque `SessionUpdate` values to the coordinator, then
  consumes the lifecycle change **within the same application update**. Only the
  coordinator interprets authentication results, timer identities and revocation
  outcomes. This is a session-specific executor bridge, not another workflow owner
  or a generic event framework.
- `state.rs` retains ordinary Rust validation/transitions, with no executor,
  transport factory or side effects. Validation receives submitted username and
  password values; there is no second editable form copy. Pending completion
  identities retain only generation/server, not a cloned username/password.
  The API future takes the submitted values for that operation. Pure state is
  private to production session ownership; legacy transition fixtures retain
  test-only access.
- Read-only status, generation-gated `client_for`, rejection and lifecycle methods
  replace shell mutations of session fields. Conversation transitions consume the
  narrow `SessionAccess` interface rather than a mutable `AppSession`; channel,
  history and send algorithms/request identities are unchanged. Form cancellation
  and restoration cannot change an active context's generation and orphan expiry.
- Logout, expiry, changed server and current authoritative rejection invalidate
  the generation immediately. `session_changed` then cancels the selected read,
  clears conversation state (including every channel's drafts and pending work),
  resets polling/list presentation, and clears the actual textarea/password before
  storage cleanup or screen removal. Server subscriptions and history completions
  now have a Window for this cleanup; rendering no longer performs deferred wipes.
- Accepted authentication clears the real password control. Recoverable failures
  leave real form inputs editable. Username/mode/control ownership stays in the
  shell until #56. Neither a zeroization guarantee nor a UI redesign is claimed.
- Logout captures the old immutable bound client before removing active access.
  Its asynchronous revocation retains the API's existing **8s HTTP bound**, with
  distinct confirmed, already-invalid and unconfirmed-failure outcomes. Old auth,
  queued expiry, protected rejection and revocation outcomes cannot affect a newer
  generation. Captured clients never adopt the newer credential/origin.

## Explicit temporary interfaces / later removal

Saved-login selection/deletion identities, worker waits, feedback, retries and
save/restore/delete decisions remain in the shell for **#55**. They use explicit
`begin_restore`, `restore_server`, `verify_saved`, `finish_restore`, accepted-context
saving and `Lifecycle::Authenticated { save }` hooks. Restoration publishes only
an identity/origin/expiry/generation-checked candidate through the same owner and
arms that owner's expiry timer. No second active context exists in saved-login
code. Storage's ordered worker, configuration/provider identities and formats are
unchanged. Delete retries still outlive authentication generations.

The shell must consume lifecycle changes immediately after synchronous session or
protected-completion operations, or after applying an opaque executor update. It
must clear workspace state before `invalidate_storage`. #55 replaces the saved-login
hooks/decisions, #56 moves form controls, and the later conversation ticket replaces
the remaining shell-owned protected request/polling orchestration. The shell's
remaining `Hamlet` name and legacy fixture are not claims of completed rearchitecture.

`runtime.rs` only exposes its existing cancellable `start` operation crate-wide so
session can own its timer; its implementation and scheduling are unchanged. HTTP
8s, protected read/send 9s and the **single combined storage + verification 10s**
budget remain distinct. No retries/replays, endpoint or protocol changes were added.

## Behavioral evidence

All 121 starting scenarios remain; seven new cases bring the full suite to **128**:

- Six session-owned controlled-execution scenarios cover screen-independent login,
  immediate local invalidation, inert duplicate/obsolete auth, old/current rejection,
  queued old versus current expiry, form/restore lifetime guards, verified restore
  followed by active-server change, bound old-client revocation and its three outcomes.
- One real-control journey exercises current history rejection with drafts in two
  channels, then same-process login and empty composers on both channels.
- The existing expiry/composer journey no longer manually expires private root
  state or clears its conversation. It uses the production timer and real controls,
  stores drafts in two channels, expires, logs in again and checks both via typing/copy.
- Existing delayed auth, old-server responses/rejections, same-process logout/login,
  editable failures, saved-login save/restore/delete/retry, controlled 10s deadlines,
  pure conversation races and real loopback HTTP route/deadline tests still pass.

Fixture changes supply submitted values explicitly and stop asserting nonexistent
model password copies. Real-control failure and sensitive-control assertions retain
those behaviors; no scenario was deleted. TDD's initial owned-interface tracer failed
on the missing coordinator, then passed. The active-lifetime guard tracer failed on
the missing guarded restore interface, then passed. Other added cases characterize
preserved behavior. Intermediate typechecks caught fixture signature/import changes
and an unused render argument; no warning was waived.

## Checks and preservation

From `client-gpui/`, existing lockfile/cache, 2026-09-29 UTC:

- Repeated `cargo check --locked --all-targets`: passed.
- Targeted session, authentication, saved-login, execution/storage, conversation and
  view suites: passed. New rejection and rewritten expiry journeys also passed with
  `SEED=1`, `2`, `3`.
- `cargo fmt --check`: passed (17:56:52).
- `cargo clippy --locked --all-targets -- -D warnings`: passed (17:56:52–54).
- `cargo test --locked`: **128 passed, 0 failed/ignored** (17:56:54–57).
- `cargo build --locked`: passed (17:56:57–59); compiled, not launched.
- `git diff --check` and all six pre-existing SHA-256 checks: passed.

### Recovery verification

After the prior worker's provider interruption, the next worker inspected the full
uncommitted source diff, new tests, migration record and original evidence. The
original `full-checks.log` is complete: all four commands have `exit=0`, including
128 passing tests and the completed build. No source correction was needed.
Fresh targeted checks independently passed: all-target typecheck, coordinator (6),
execution/storage (8), rejection cleanup (1), expiry/relogin (1), saved-login (2).
The final four-gate rerun is recorded separately in `recovery-full-checks.log`;
original logs and `preservation-before.sha256` remain intact. The live issue was
refetched (open, no comments, `ready-for-agent`). Review artifacts and commit range
still use the original pinned start, not a recovery-time baseline.

Original modified `.gitignore`, `CONTEXT.md`, `client-gpui/README.md` and untracked
`client-gpui/ARCHITECTURE.md`, `docs/plans/client-gpui-prototype.md`,
`docs/plans/client-gpui-rearchitecture.md` are byte-preserved and excluded from
staging. No server, dependency/lockfile, API production, storage mechanics, layout,
semantic ID, baseline or prior migration-document changes. No real credentials,
profile/keyring, desktop automation, issue comment/closure, push, branch switch,
stash or reset.

Evidence and parent review artifacts: `/tmp/hamlet-orchestrator/review-54/`.
Parent owns mandatory fresh parallel Standards/Spec reviews after the scoped commit,
then any issue closure. No nested reviewers were launched. No substantive blocker;
pending parent review is not an implementation blocker. Native IME, accessibility,
physical-input, delayed native anchoring and locked/slow real-wallet limitations in
`VERIFY.md` remain unverified. Do not start #55 from this checkpoint.
