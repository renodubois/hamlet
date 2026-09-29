# Session-scoped conversation checkpoint — #57

Scope: [issue #57](https://github.com/renodubois/hamlet/issues/57), pinned start
`9626894bad4d9fb4771eb04842fb8b6807c964f0` on `rewrite`. Live body, comments
(none), and label (`ready-for-agent`) were fetched; blocking #54 is closed.
Repo instructions, target architecture, approved plan, baseline #47 and all prior
migration records were read. **Authenticated child-view extraction (#58) is not
implemented here.** The six pre-existing documents remain untouched.

## Ownership and lifetime

- `src/conversation/mod.rs` owns one coordinator for each accepted session,
  allocated by `SessionCoordinator::activated`. Its cloneable `ConversationHandle`
  shares that one owner, not a copy of conversation state. The owner holds the
  original bound client, pure state, token-free originating generation/expiry,
  pure polling schedule, cancellable reads/timer and tracked creation/send tasks.
- Views issue select/create, edit-draft, send, channel/history refresh, older/retry
  and focus intentions. `read()` exposes a read-only borrow; no view mutates maps
  or supplies the session model. Channel navigation reuses the owner and cached
  history/drafts. `start` is idempotent; a closed handle cannot reopen activity.
- Session logout, expiry, server change, rejection and drop synchronously close
  the owner, remove its dispatch client, clear every channel's drafts/history and
  pending state, close delivery and abort owned tasks. This happens inside session
  invalidation, before any shell lifecycle consumer or workspace removal. Aborting
  a write wait does not prove non-delivery; no write replay was introduced.
- `state.rs` no longer imports session or mutates `AppSession`/`SessionCoordinator`.
  A pure `Identity` carries the originating generation/expiry and rejection result;
  it is not another authenticated context. The coordinator reports `SessionEnd`
  with that generation; the session interface alone invalidates authentication.
  Old generations cannot invalidate a new session. The temporary `SessionAccess`
  trait is removed. Generation-gated client access remains test-only for existing
  pure-state/real-route integration fixtures, not a production view dispatch path.
- Executor deliveries are opaque. The shell's small bridge applies them to their
  captured conversation handle and forwards an optional session-end signal, like
  its existing session-update bridge. It does not inspect endpoint responses,
  request/channel IDs, cancellation serials, or polling decisions. There is no
  alternate production/test workflow implementation.

## Selected history, polling and presentation

Every initial selection, manual refresh, older/retry page, focus-return read,
scheduled poll, recovery and safe post-send catch-up uses the same selected-history
dispatch/application lifecycle. Navigation aborts the old read and advances task
identity; pure request/channel/session guards remain. Channel reads are now also
owned/cancelled on shutdown. Already-queued outcomes remain inert after closure.

Existing reconciliation algorithms are retained: opaque cursor continuity,
overlap deduplication by server ID, server order for timestamp ties, contiguous
history retained with incomplete feedback, per-origin send locks/results, retained
failed/uncertain drafts and warnings, staged confirmations and fresh reads when
confirmations overlap polling/catch-up. Equal message text never confirms a write.
Recovery first proves continuity; a retained confirmation can then require its
own fresh read. No offline/durable drafts or queued writes were added.

Polling retains focused selected-history/channel intervals of **3s/15s**, pause
while unfocused, focus-return reconciliation, independent backoff to **60s**, and
the existing first-tick startup reconciliation. A one-second owned timer is aligned
to the activity clock, so executor-delivery latency cannot cumulatively shift its
wakeups. The pure polling module is unchanged. HTTP **8s**, protected reads/send
**9s per operation/page**, and session/storage **10s** budgets remain distinct;
creation retains only its existing HTTP bound.

The shell retains authenticated rendering, textarea/creation input, focus, list,
and subscriptions for #58. `present_conversation` only reconciles chronological
stable IDs, channel replacement, viewport anchoring/bottom-follow, and displayed
input values against observable state. Its ID vector is a row-index/viewport
snapshot, not a second message store. It never interprets pagination continuity
or decides subsequent requests. Equal composer text is not rewritten on ticks,
preserving editing position. Layout, control IDs, labels and Kit Root are unchanged.

## Behavioral evidence

All **143** starting scenarios remain, with **seven** added owned-interface cases:

1. Navigation reuses cached histories and independent drafts without a view.
2. Queued pages, creations, sends and 401s cannot repopulate closed activity;
   surviving handles cannot dispatch or restart timers.
3. Current rejection reports its originating generation and clears activity before
   the host processes the session signal.
4. Multiple confirmations overlapping a paginated catch-up require fresh reads;
   failure preserves contiguous history, retry preserves intervening messages and
   merges equal-text publications once by server identity.
5. Per-channel pending locks, originating results, the 9s uncertain-send timeout,
   retained draft/warning and selected-only recovery without replay.
6. Session loss through logout, expiry, server change and rejection closes a
   surviving handle synchronously without any view; old queued 401s and a retained
   old bound client cannot reopen or invalidate the newer session.
7. Delayed timer delivery cannot move the 15-second channel wakeup.

Legacy polling/journey fixtures now advance the shared controlled clock rather
than invoke removed private `poll_at` methods. The middle-insertion scenario sends
through real composer/button controls and a bound response instead of manually
mutating send state/completing requests. Existing state/real-route fixtures now
supply explicit identity, and remaining legacy read assertions borrow the new
read-only owner. Their viewport/composer private assertions remain for the later
view extraction; no new scenario uses private child fields. No scenario was deleted.

TDD: the initial owned-interface tracer failed on the missing handle and then
passed. During final verification, the real-route polling journey intermittently
failed to discover a channel (also reproduced in 1 of 2 isolated repeats).
Temporary timing-only instrumentation identified one-shot timer rearming from
*delivery time*, accumulating Tokio/GPUI handoff latency. The deterministic delayed-
tick tracer failed before the clock-aligned fix and passed after it; the original
loopback journey then passed **12 consecutive runs**. Instrumentation is removed.
Other intermediate failures were fixture migration mistakes (including expecting
successful delivery to a newly cancelled channel read), a recovery oracle that
omitted the existing additional confirmation read, and one strict Clippy warning;
all were corrected, not waived. Original failed logs remain in the evidence folder.

## Checks and preservation

From `client-gpui/`, existing lockfile/cache, 2026-09-29 UTC:

- Repeated `cargo check --locked --all-targets`: passed.
- Targeted pure-state (27), views, coordinator and lifecycle checks: passed.
- Final `SEED=1,2,3`: owned coordinator (6), polling matches (10), and session-loss
  lifecycle (1) each passed. Fixed real-route journey: 12 consecutive passes.
- `cargo fmt --check`: passed (19:37:12).
- `cargo clippy --locked --all-targets -- -D warnings`: passed (19:37:12–13).
- `cargo test --locked`: **150 passed, 0 failed/ignored** (19:37:13–16).
- `cargo build --locked`: passed (19:37:16–19), compiled but not launched.
- `git diff --check` and all six original SHA-256 checks: passed.

Original modified `.gitignore`, `CONTEXT.md`, `client-gpui/README.md`, and untracked
`client-gpui/ARCHITECTURE.md`, `docs/plans/client-gpui-prototype.md`, and
`docs/plans/client-gpui-rearchitecture.md` are byte-preserved and never staged.
No server, dependency/lockfile, API production, storage mechanics, runtime,
baseline or prior migration-document changes. No desktop automation, real
credentials/keyring, issue mutation, push, branch switch, stash or reset.
Native IME, accessibility/physical input, precise delayed native anchoring and
locked/slow real-wallet limitations in `VERIFY.md` remain unverified.

Artifacts: `/tmp/hamlet-orchestrator/review-57/`, including pinned start, live
issue JSON, preservation hashes, checks, commit list, diff and handoff. Parent
performs required fresh parallel Standards/Spec reviews **after the scoped commit**,
then any issue closure. No nested delegation. No substantive blocker; parent
review pending is the expected handoff. Do not begin #58 here.
