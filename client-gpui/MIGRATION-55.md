# Saved-login session ownership checkpoint — #55

Scope: [issue #55](https://github.com/renodubois/hamlet/issues/55), pinned start
`e0d3be56a909437135dfc9941f93bda796bf8405` on `rewrite`. Live body, comments (none)
and label (`ready-for-agent`) were fetched. This implements saved-login ownership
only; **login-view extraction (#56) and later conversation/view work are not included**.
The approved architecture/plan, baseline and prior migration records are unchanged.

## Ownership and interfaces

- `src/session/saved_login.rs` owns candidate selection, startup prefill, retained
  cleanup selections, save serials, deletion identities, restore/save/delete
  dispatch and completion policy, deadlines, feedback and retries. Its state is
  owned exclusively by the application-lifetime `SessionCoordinator`, outside
  either rendered screen. It contains no GPUI controls or rendering.
- Session construction accepts configuration and the existing ordered worker,
  resumes durable deletion intents and starts eligible restoration. Exact stored
  server spelling still determines prefill/candidate eligibility. Startup username
  is an immutable prefill snapshot (including expired candidates), not an editable
  form copy. Passwords are not added to session state.
- `initial_username`, `storage_feedback`, `storage_retry` and `retry_storage` are
  the shell's narrow saved-login surface. The shell only renders status/action
  labels and forwards the retry intention. Its existing opaque session-update
  delivery loop now also delivers storage outcomes; it interprets no storage IDs,
  results, credentials or timeout policy.
- Removed shell `Deletion`, persistence/credential/retained-cleanup fields, serials,
  startup restoration/deletion calls and every saved-login workflow method. Removed
  temporary coordinator `begin_restore`, `restore_server`, `finish_restore` hooks
  and the shell-facing `Lifecycle::Authenticated { save }` decision. Candidate
  verification is private to session ownership; pure-state test compatibility
  remains test-only. The old `persistence` namespace alias is now test-only.
- Authentication success starts saving inside session. Logout, expiry, server
  change and authoritative rejection invalidate active access and storage work
  synchronously; the shell consumes the lifecycle in the same application update
  to clear conversation/composer/password state before screen removal. Storage
  submission remains nonblocking. No workflow waits for a view to request cleanup.

## Identity, feedback and ordering

- Auth generations gate verification; save serials gate save completions; deletion
  IDs survive authentication changes. Retries cannot target a newer active login's
  account. Old provider commands precede newer saves on the unchanged FIFO worker.
- Deletions deduplicate exact selections only within the originating cleanup batch.
  Identical public metadata after a newer login does not prove an old deletion
  removed the newly written credential. A late old result cannot settle a newer
  logout. Confirmed deletion retires earlier attempts for that account, never later
  identities. A stale worker outcome alone is not reported as confirmed deletion.
- Retain the previous exact selection even for same-account replacement. Failed or
  invalidated saving can roll back to that older metadata identity; logout queues
  cleanup of both the attempted and retained selections. Storage's existing stale
  selection guard and FIFO mechanics decide which may actually delete the entry.
- Current save/restore feedback and older cleanup feedback are retained separately,
  composed in the existing `storage-status` control. New restoration/saving cannot
  erase an old deletion warning; late cleanup cannot erase memory-only/unconfirmed
  save feedback. Server revocation still has its distinct `auth-feedback` outcome.
- Each secure-store workflow retains its independent **10s** budget. Remember/save
  replies share one budget; worker read plus `/me` share one combined budget.
  Provider work survives a timed-out wait. HTTP **8s** and protected read/send **9s**
  bounds are unchanged. No replay, credential migration, plaintext fallback,
  zeroization or exactly-once claim was introduced.

`storage/`, API production, runtime, pure session/conversation algorithms, server
routes, dependencies/lockfile, configuration format/path and provider identities
are unchanged. Login controls and authenticated rendering remain in the temporary
shell for their own tickets; this is not a claim of completed rearchitecture.

## Behavioral evidence

All 128 starting scenarios remain; **nine new scenarios** bring the suite to 137.
The two legacy saved-login control journeys now use production `open`, Kit `Root`,
real controls/stable IDs and explicit dependencies, not private shell fields or
manual workflow invocation. The restored-coordinator lifetime test likewise uses
real saved metadata/provider startup rather than removed lifecycle hooks.

New owned-session tests cover save/restart/verify/logout without a screen, expired
prefill, delayed failed deletion followed by the same identity's newer save, all
five immutable #47 configuration fixtures (including omitted pending intents,
distinct server spelling and logged-out restart), old cleanup warnings surviving
restoration/restart, failed manual same-account replacement after connectivity
failure, and delayed old cleanup versus newer identical-metadata logout.

New real-control journeys cover restoration failure, deliberate restoration retry,
timeout then manual login with obsolete verification unable to activate; and old
account deletion failure/retry on both login and newer workspace screens without
removing the new credential. Existing combined-deadline, unconfirmed save/delete,
FIFO/rollback, separate revocation, stale verification and real-route tests remain.

TDD evidence: initial owned-interface tracer failed on the missing API; expired
prefill caught constructor-startup cleanup erasing the prefill; restart feedback,
rollback identity and identical-metadata late-cleanup tracers each failed before
focused fixes. These are session-policy corrections required by #55, not changes
to storage mechanics. Characterization/control migrations retained existing
behavior. An intermediate check caught fixture imports and transport construction;
an initial control assertion assumed empty Kit input value was `Some("")` rather
than `None`. Fixed those fixtures; no warning or failed gate was waived.

## Checks and preservation

From `client-gpui/`, existing lockfile/cache, 2026-09-29 UTC:

- Repeated `cargo check --locked --all-targets`: passed.
- Targeted session, saved-login and execution/storage suites: passed. Saved-login
  (11 matching scenarios) and execution/storage (4) also passed with `SEED=1,2,3`.
- `cargo fmt --check`: passed (18:36:09–10).
- `cargo clippy --locked --all-targets -- -D warnings`: passed (18:36:10–11).
- `cargo test --locked`: **137 passed, 0 failed/ignored** (18:36:11–14).
- `cargo build --locked`: passed (18:36:14–16); compiled, not launched.
- `git diff --check` and all six pre-existing SHA-256 checks: passed.

Original modified `.gitignore`, `CONTEXT.md`, `client-gpui/README.md` and untracked
`client-gpui/ARCHITECTURE.md`, `docs/plans/client-gpui-prototype.md`,
`docs/plans/client-gpui-rearchitecture.md` are byte-preserved and excluded from
staging. No real credentials/keyring/profile, desktop automation, server/dependency
change, issue mutation, push, branch switch, stash or reset. Native limitations in
`VERIFY.md` remain unverified; automated tests are not native acceptance.

Evidence and review artifacts: `/tmp/hamlet-orchestrator/review-55/`. Parent owns
mandatory fresh parallel Standards/Spec review **after the scoped commit**, then
any issue closure. No nested reviewers were launched. No substantive blocker;
pending parent review is not an implementation blocker. Do not begin #56 here.

## Review correction — two P2 findings (2026-09-29)

Fresh correction session started at `bee2e53bd4f3c9ef096cd9601dd109baaeef39ff`;
review baseline remains `e0d3be56a909437135dfc9941f93bda796bf8405`. Live #55 is
still **OPEN**. Both findings were reproduced before their respective fixes at the
approved session/storage interface, with controlled execution/time, isolated files
and the existing fake provider. No private workflow-state assertions were added.

1. **Noncandidate replacement cleanup:** the immutable
   `last-successful-server-differs-after-failed-save` fixture reached the saved-with-
   cleanup-warning outcome but never exposed coordinator deletion feedback/retry.
   Startup had discarded its saved metadata solely because the selected server
   differed. Retain that metadata independently of restoration eligibility; exact
   server matching still gates username prefill, automatic restore and restore retry.
   The existing replacement path now receives the old selection and owns its cleanup
   identity. Regression covers warning/retry while authenticated, logout, restart
   from durable intents and eventual successful deletion.
2. **Partial cleanup success:** after old-account deletion failed, a newer account
   logged in and then logged out with the provider unlocked. Its successful deletion
   left feedback stuck at `Removing saved login from Secret Service…` while the older
   deletion remained retryable. Recompute feedback after deletion starts/completions
   and successful saves that retire identities. Remaining unconfirmed work takes
   precedence over in-flight progress; removal is confirmed only when none remain.
   Regression verifies the warning and retry after newer logout, then final cleanup.

Red evidence: `correction-1-red.log` failed on missing cleanup feedback (5s bound);
`correction-2-red.log` failed on the literal stuck `Removing` status. Each focused
fix passed its tracer (`correction-{1,2}-green.log`). No storage protocol, screen,
API, runtime, dependency or server code changed. Existing real-control journeys
continue to verify the unchanged shared feedback/retry controls on both screens.

Correction checks: `cargo check --locked --all-targets` passed twice; the nine owned
saved-login tests passed; all 13 saved-login matches passed at `SEED=1,2,3`; 19
storage matches and the four execution/storage cases passed. One initial targeted
filter matched zero tests; the correct full module filter was then run (4 passed).
Required `cargo fmt --check`, strict all-target clippy, full `cargo test --locked`
(**139 passed, 0 failed/ignored**) and `cargo build --locked` passed at
18:46:38–44 UTC. `git diff --check` and all six original file hashes passed.
Artifacts remain in `/tmp/hamlet-orchestrator/review-55/`; parent reviews the
correction commit against the original baseline before any issue action. Native
limitations and all preservation/scope constraints above remain unchanged.
