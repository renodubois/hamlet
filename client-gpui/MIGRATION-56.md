# Login-view ownership checkpoint — #56

Scope: [issue #56](https://github.com/renodubois/hamlet/issues/56), pinned start
`324de991cc931f63032627a7df1fcfc2f68db261` on `rewrite`. Live issue body, comments
(none), and label (`ready-for-agent`) were fetched; #54–55 are closed. The approved
architecture/plan, immutable #47 baseline, and prior migration records were read
and remain unchanged. **Conversation coordination (#57) is not implemented here.**

## Ownership and lifetime

- `src/views/login.rs::LoginView` owns the server, username and masked password
  entities, login/signup mode, input subscription, session observation, native Kit
  focus/keyboard controls, pending presentation, validation feedback and rendering.
  Its interface is construction against a session entity and sensitive lifecycle
  cleanup; parents never read inputs or pass editable values back to the form.
- The single application-lifetime `SessionCoordinator` is now held in a GPUI
  `Entity`. Login calls its existing submit/change-server/cancel methods directly.
  The coordinator implementation, pure state, auth execution, saved-login work,
  expiry and rejection rules are unchanged. Hosts notify the entity after updates;
  form and shell observe it. No second auth implementation or editable password
  copy was introduced. Submitted credentials live only in the form and pending
  operation that needs them, as before.
- The shell still delivers opaque executor updates and consumes session lifecycle
  changes. It no longer owns login input entities, mode, subscriptions, submission
  or form rendering. It invokes the child's narrow cleanup operation before
  switching screens, while clearing existing authenticated activity synchronously.
  Conversation calls were mechanically adapted to borrow the same session entity;
  their dispatch, cancellation, reconciliation, polling and algorithms stay put.
- The login view observes accepted authentication even while retained but hidden.
  Passwords are cleared on acceptance, server edits and shell invalidation. The
  shell retains the cleared child to preserve existing username/mode behavior
  after logout; it retains no hidden password. Recreating the view hydrates public
  server/startup-prefill and current pending/feedback state, never old passwords.
  Subscriptions belong to the view, not the shell. No memory-zeroization claim.
- Validation remains session policy and is presented in the login view. Shared
  storage feedback/retry remains in the shell on either screen; authenticated auth
  feedback remains there too. Dropping/recreating the form cannot cancel saving,
  restoration or independent deletion retries. Kit Root, semantic IDs, input labels,
  button wording, focus behavior and form styling are retained.

API, session, storage and runtime implementation files, dependencies, rewrite-server
routes, transport policy, configuration/provider identity, and deadlines (8s HTTP,
9s protected reads/send, combined 10s saved-login workflows) are untouched.

## Behavioral evidence

All **139 starting scenarios remain**, with four added real-control scenarios under
`src/views/tests/login.rs`:

1. Initial hydration preserves exact saved server spelling/username, starts with
   no password, and keeps native Kit Tab focus traversal.
2. Both login and signup accept while the original form is hidden but retained;
   after logout/remount the same username/mode survives, the password is empty,
   and real submission cannot reuse the old secret.
3. Removing/recreating a pending form hydrates its pending button without a second
   POST or password; rejection updates the new form. New server edits clear its
   password, deliberate retry targets the new origin, and subsequent acceptance,
   logout and another recreation do not retain password state.
4. Login rejection leaves the password editable for a deliberate corrected-username
   retry; the outward bound request proves the same password survived the failure.

Existing authentication tests now use production `open`, real Kit controls and
semantic observations, not private shell login methods/fields. Their signup
validation, editable failure/uncertainty and inert duplicate-submit scenarios remain.
Two authenticated legacy fixtures only adapt session borrowing mechanically; their
private history/composer assertions await their owning tickets. Existing changed-
server auth/restore, expiry/rejection, saved-login cleanup across screens and real
HTTP/controlled-worker scenarios all continue to pass.

TDD's hydration tracer first failed on the missing login-view interface, then passed
once implemented. Subsequent lifecycle checks characterize the extracted behavior
at the approved view/session seams. Intermediate fixture errors (TestAppContext
executor access, glob-imported test macro, signup's required 201 response, feedback
wording) and Clippy's owned-string comparison warning were corrected, not waived.

## Checks and preservation

From `client-gpui/`, existing lockfile/cache, 2026-09-29 UTC:

- Four `cargo check --locked --all-targets` checkpoints: passed.
- Targeted authentication, all-view, hydration/retention/recreation and retry runs:
  passed. Final owned-login suite: **4 passed**; final views: **46 passed**.
- `SEED=1,2,3`: login/saved-login matches (**13** each) and execution/storage view
  matches (**9** each): passed.
- Final `cargo fmt --check`: passed (19:09:24).
- Final `cargo clippy --locked --all-targets -- -D warnings`: passed (19:09:24–26).
- Final `cargo test --locked`: **143 passed, 0 failed/ignored** (19:09:26–31).
- Final `cargo build --locked`: passed (19:09:31–34), compiled but not launched.
- `git diff --check` and all six original SHA-256 checks: passed.

Original modified `.gitignore`, `CONTEXT.md`, `client-gpui/README.md` and untracked
`client-gpui/ARCHITECTURE.md`, `docs/plans/client-gpui-prototype.md`, and
`docs/plans/client-gpui-rearchitecture.md` are byte-preserved and excluded from staging.
No real credentials/keyring, desktop automation, server/dependency change, issue
mutation, push, branch switch, stash or reset. Native IME, accessibility/physical
keyboard, delayed native anchoring and locked/slow real-wallet limitations in
`VERIFY.md` remain unverified; headless checks are not native acceptance.

Artifacts: `/tmp/hamlet-orchestrator/review-56/`. Parent performs mandatory fresh
parallel Standards/Spec review after the scoped commit, then any issue closure.
No nested reviewers were launched. No substantive blocker; parent review pending
is the expected handoff, not an implementation blocker. Do not start #57 here.
