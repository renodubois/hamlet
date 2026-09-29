# Composer ownership checkpoint — #60

Scope: [issue #60](https://github.com/renodubois/hamlet/issues/60), pinned start
`30c650262662f3f0e8d43bdcdfc84d0b045cd976` on `rewrite`. Live body, comments
(none), and label (`ready-for-agent`) were fetched; blocking #58 is closed. Repo
instructions, target architecture, approved plan, baseline #47 and migration
records #48–59 were read. **Final contraction/audit/documentation (#61) and native
verification (#62) are not implemented.** The six pre-existing files are untouched.

## Ownership and synchronization

- `src/views/conversation/composer.rs::ComposerView` owns the textarea, Kit focus
  and keyboard handling, input subscription, feature-notification task, displayed
  text synchronization, pending/read-only button/input state, and send feedback.
  Its parent-facing interface is construction against a conversation handle; no
  parent reads, synchronizes or clears its input fields.
- `src/views/conversation/mod.rs` now only constructs and lays out its independent
  history and composer children. There is no parent feature subscription, selected
  channel projection, draft projection, input or send action remaining there.
- The existing conversation coordinator remains the only authoritative per-channel
  draft/send owner. The child retains only its current displayed channel and last
  hydrated/forwarded text, so unchanged notifications cannot overwrite queued edits
  or move the caret. Construction/recreation hydrates current state without reads,
  send replay or a second executor-delivery loop.
- Navigation flushes any final edit to its originating channel before hydrating the
  new channel. Programmatic updates are guarded and update the projection before
  Kit change delivery; their subscription cannot overwrite the other draft. A new
  submission guard rejects Enter/click delivery from an old displayed channel when
  coordinator selection has already changed. Otherwise an old Enter could submit
  the *new* channel's retained draft. This was reproduced before the fix.
- Each child's notification task is owned by the entity and dropped with it. A
  retained hidden child continues to consume feature invalidations and clears its
  input/projection when activity closes. Conversation close synchronously removes
  authoritative drafts and blocks further work; display cleanup runs through the
  existing notification/window-update path, not a memory-zeroization guarantee.
  An old child never adopts a newer session's handle.
- Real Enter/Shift+Enter, mid-caret text, trailing/multiline text, send-button and
  textarea focus behavior, semantic IDs, labels, icon and tooltip remain. Pending
  sends lock only their originating channel; success clears only its draft, while
  failed/uncertain results retain its draft/warning without replay. Existing send,
  reconciliation, polling, 8s HTTP/9s read-send/10s storage policies are unchanged.

No shell/workspace/sidebar/history, session, conversation-coordinator/state, API,
HTTP route, runtime, storage, dependency or persisted-format changes. No scenario
was removed or rewritten, and no public child input accessor was added for tests.

## Behavioral evidence

All **156** starting scenarios remain; four independent composer scenarios in
`src/views/tests/composer_lifecycle.rs` use Kit Root, real textarea/buttons/clipboard,
semantic IDs, controlled bound responses/time and the owned conversation interface:

1. Hydrate two cached drafts, edit through the textarea, recreate repeatedly and
   switch channels without overwriting either draft or submitting during hydration.
2. Deliver Enter from the still-displayed old textarea after channel selection has
   changed: retain the originating edit and never submit the new channel's draft.
3. Recreate during a pending send; preserve its read-only text and prevent duplicate
   submission. Edit another channel while pending, confirm the hidden origin, then
   recreate during a second send and observe its 9s timeout, retained warning/draft
   and cancelled late completion without replay.
4. Hide/retain a pending composer, invalidate activity, remount/recreate it and
   verify no composer/send/error controls or drafts return and late delivery is inert.

Existing real-control keyboard/button, mid-caret/trailing-newline, uncertain-send,
expiry/relogin, rejection/relogin, workspace recreation, polling and history cases
pass unchanged. Headless controls do **not** establish native IME composition safety.

TDD: the hydration tracer first failed on the missing child interface, then passed
with extraction. The queued-Enter tracer failed on an actual unintended POST to the
new channel, then passed with the submission guard. Remaining lifetime scenarios
characterize preserved behavior. A preflight strict Clippy check found a needless
explicit fixture lifetime; it was removed, not waived. Failed and successful logs
are retained in the evidence directory.

## Checks and handoff

From `client-gpui/`, existing dependencies/lockfile/cache, 2026-09-29 UTC:

- Three `cargo check --locked --all-targets` checkpoints: passed.
- Targeted new tracers, composer (9 matching scenarios) and all views (55): passed.
- `SEED=1,2,3 cargo test --locked composer`: 9 passed each.
- `cargo fmt --check`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo test --locked`: **160 passed, 0 failed/ignored** (ended 20:25:53 UTC).
- `cargo build --locked`: passed (ended 20:25:55 UTC); compiled, not launched.
- `git diff --check` and all six starting SHA-256 fingerprints: checked at handoff.

Original modified `.gitignore`, `CONTEXT.md`, `client-gpui/README.md`, and untracked
`client-gpui/ARCHITECTURE.md`, `docs/plans/client-gpui-prototype.md`, and
`docs/plans/client-gpui-rearchitecture.md` are byte-preserved and never staged.
No desktop automation, real credentials/keyring, issue mutation, push, branch
switch, stash or reset. Native IME, accessibility/physical keyboard, precise delayed
native anchoring and locked/slow real-wallet checks remain unverified as recorded
in `VERIFY.md`; this checkpoint does not claim native acceptance.

Artifacts: `/tmp/hamlet-orchestrator/review-60/`, including pinned start, live issue
JSON, original copies/hashes, red/green and full-check logs, scoped committed diff,
commit list and handoff. Parent owns mandatory fresh parallel Standards/Spec reviews
**after the scoped commit**, and any issue closure. No nested delegation. No substantive
implementation blocker; parent review is pending. Do not begin #61 here.
