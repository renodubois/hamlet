# Message-history ownership checkpoint — #59

Scope: [issue #59](https://github.com/renodubois/hamlet/issues/59), pinned start
`a8b3d0747f0b9893051baa8efe9091a21e042825` on `rewrite`. Live body, comments
(none), and label (`ready-for-agent`) were fetched. Repo instructions, architecture,
approved plan, baseline #47 and migration records #48–58 were read. **Composer
extraction (#60) is not implemented.** The six pre-existing documents are untouched.

## Ownership and preservation

- `src/views/conversation/message_history.rs::MessageHistoryView` owns the GPUI
  list, focus, wheel callback, height hints, chronological stable-ID projection,
  viewport reconciliation, refresh/older/retry/jump controls and notification task.
  Construction hydrates current cached state without starting activity or reads.
  The task belongs to the child and is dropped with it; hidden retained children
  still receive feature invalidations. No parent reads or adjusts its list/focus.
- `message_row.rs` renders plain multiline text, author and timestamp with the
  existing stable row/text IDs and Kit `SelectableText`. Rows are rendering helpers,
  not new view entities. The child owns the selectable subtree; Kit Root supplies
  selection/copy integration and clears selection when participants disappear.
  There is no competing application selection or message store.
- The existing stable-ID splice algorithm and 64-pixel offscreen height estimate
  move unchanged. Channel replacement resets/follows the new list; same-channel
  prepends and middle insertions preserve the variable-height reader anchor.
  Newest additions follow only when appropriate; explicit Jump resumes following.
  No timestamp sorting, cursor interpretation, HTTP or continuity decisions enter
  the view. Requests remain intentions on the one conversation coordinator.
- `conversation/mod.rs` composes the history entity and retains the existing
  textarea, keyboard/subscription and displayed-draft synchronization for #60.
  Only history responsibilities are removed from this parent. Padding/gaps and
  populated-list sizing are retained; empty/loading/failure content does not gain
  a new expanding spacer. Existing IDs, labels, tooltips and feedback remain.
- Feature timers, sends, user actions and invalidation use the existing independent
  notification stream. Each view hydrates from authoritative conversation state;
  recreation adds no executor-delivery loop, subscription-driven request, or
  duplicate history/draft state. Shutdown resets the retained child presentation.

No changes to shell/workspace/sidebar, session, conversation coordination/state,
polling, API, runtime, storage, dependencies, rewrite-server routes or deadlines.
No protocol, persistence, styling redesign or product feature changes.

## Behavioral evidence

All **153 starting scenarios remain**. Three new independent history scenarios
under `src/views/tests/history_lifecycle.rs` use Kit Root, semantic controls,
controlled bound responses/time and the owned conversation interface:

1. Hydrate a cache loaded before child construction, recreate without reads, observe
   channel replacement while hidden, and clear retained hidden history on shutdown.
2. Drag-select actual multiline message text, close activity, and observe selection
   cleared after notification/frame processing. This is not a synchronous memory
   zeroization claim or proof of native clipboard behavior.
3. Recreate repeatedly without reads; observe scheduled polling without parent
   intervention, preserve cached rows with incomplete feedback during an outage,
   hydrate that feedback on recreation, and recover through the real refresh control
   without duplicating an in-flight read.

The existing production wheel/older retry test, confirmed middle insertion,
refresh/jump/bottom-follow and exact multiline drag/Ctrl+C tests all pass unchanged.
They drive real controls, not private history list/focus fields. Original primitive
feasibility probes remain distinct from production evidence. Composer and cross-view
channel/send/expiry/rejection journeys also pass; no scenario was removed.

TDD: the independent hydration tracer failed on the missing child interface, then
passed after extraction. Additional selection and polling cases characterize the
preserved behavior. An initial selection fixture lacked a flex host and could not
lay out rows; the corrected real-host fixture passed without a production change.
An intermediate typecheck caught using `advance_clock` on VisualTestContext rather
than its injected background executor. Both fixture failures are retained in logs;
neither is reported as a product defect or a waived check.

## Checks and handoff

From `client-gpui/`, existing lockfile/cache, 2026-09-29 UTC:

- Repeated `cargo check --locked --all-targets`: passed.
- Targeted history (6), composer (4), independent history (3), and all views (51):
  passed. `SEED=1,2,3 cargo test --locked history`: **17 passed** each.
- `cargo fmt --check`: passed (20:14:26–27).
- `cargo clippy --locked --all-targets -- -D warnings`: passed (20:14:27–28).
- `cargo test --locked`: **156 passed, 0 failed/ignored** (20:14:28–36).
- `cargo build --locked`: passed (20:14:36–39); compiled only, not launched.
- `git diff --check` and all six original SHA-256 checks: passed.

Original modified `.gitignore`, `CONTEXT.md`, `client-gpui/README.md`, and untracked
`client-gpui/ARCHITECTURE.md`, `docs/plans/client-gpui-prototype.md`, and
`docs/plans/client-gpui-rearchitecture.md` are byte-preserved and never staged.
No desktop automation, real credentials/keyring, issue mutation, server/dependency
change, push, branch switch, stash or reset. Native IME, accessibility/physical input,
precise delayed native pixel anchoring and locked/slow real-wallet limitations in
`VERIFY.md` remain explicitly unverified; headless checks do not resolve them.

Artifacts: `/tmp/hamlet-orchestrator/review-59/`, including original start, live
issue JSON, preservation hashes, logs, commit list, scoped diff and handoff. Parent
owns mandatory fresh parallel Standards/Spec reviews after the scoped commit and
any subsequent issue closure. No nested delegation. No substantive implementation
blocker; parent review remains pending. Do not begin #60 from this checkpoint.
