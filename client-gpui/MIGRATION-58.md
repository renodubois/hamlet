# Workspace and channel-sidebar checkpoint — #58

Scope: [issue #58](https://github.com/renodubois/hamlet/issues/58), pinned start
`1853b216d5a62b399506847f4cf385323776ab71` on `rewrite`. Live body, comments
(none), and label (`ready-for-agent`) were fetched; #57 is closed. Repo instructions,
architecture, approved plan, baseline #47 and migration records #48–57 were read.
**Independent history/row and composer extraction (#59/#60) is not implemented.**
The six pre-existing documents remain untouched.

## Ownership

- `src/views/workspace.rs` composes a sidebar and a combined conversation-layout
  entity for one accepted session. It renders connection status but owns no
  selected-channel copy, input, requests, message merging or draft/history store.
- `src/views/channel_sidebar.rs` owns the channel-name InputState (including Kit
  focus), rendering, confirmation revision and feature subscription. Its create,
  select and refresh actions call narrow conversation intentions. Validation,
  ordering, pending protection, selection and uncertainty remain feature policy.
- `src/views/conversation/mod.rs` temporarily owns **the existing combined**
  history/composer presentation: list/focus, row-ID viewport projection, textarea
  and subscription. The old row rendering, anchoring and history controls remain
  together; no MessageHistoryView, message-row module or ComposerView was added.
  Those independent child interfaces remain #59/#60 work.
- `app_shell.rs` now chooses login/workspace, forwards native activation, renders
  session/storage feedback, and retains only the existing opaque feature-delivery
  bridges. It does not manipulate sidebar/history/composer controls. The session
  still owns the conversation lifetime and closes it before workspace removal.
  Workspace recreation neither starts activity nor adds an executor-delivery loop.

## Notifications and synchronization

`ConversationHandle::notifications` supplies independent, capacity-one coalesced
invalidations, separate from the single-consumer opaque request-delivery stream.
All intentions, completions and close notify after releasing their state borrow,
including early-return validation feedback. Consumers hydrate from the current
read-only feature state; notifications contain no second state or result dispatcher.
Each view owns its notification task, cancelled on drop; closed receivers are
pruned on subsequent notifications. Hidden retained views also observe shutdown.

Sidebar confirmation handling preserves changed input: only a newly observed
confirmed creation clears matching trimmed text. Recreation starts with an empty
local input and the current confirmation revision, while pending/error/selection
state hydrates from the owner. No automatic write replay is added. The original
initial server order/first selection, empty/loading/failure states, valid-selection
refresh preservation and disappearing-selection cached fallback are unchanged.

Splitting navigation from the combined layout required protecting queued Kit
input events. The layout tracks its last synchronized text and displayed channel
as presentation projections, not an authoritative draft store. Unchanged feature
notifications/renders cannot overwrite an unprocessed edit. A channel change
forwards a final edit with its originating channel before hydrating the new draft;
`edit_channel_draft` uses the same closed gate and pure pending-send edit rules.
Programmatic hydration remains guarded. This is boundary adaptation for the split,
not independent composer extraction or a second per-channel draft map.

## Behavioral evidence

All **150 starting scenarios remain**, with three new scenarios:

1. Independent feature observers receive initial work, completions, selection,
   validation and shutdown; dropping/replacing one does not consume another's
   notifications or request deliveries.
2. Real workspace controls preserve server order and per-channel history/drafts,
   hydrate cached selected content on recreation without extra requests, and clear
   retained hidden controls on close. Closed controls cannot restart requests.
3. Delayed creation remains single-submit; confirmation while hidden selects its
   returned channel without clearing a subsequently edited name. Recreation during
   another pending creation hydrates pending state without replay; uncertainty
   updates the recreated sidebar and does not navigate.

Existing channel creation validation/conflict/uncertainty, refresh/fallback, delayed
logout completion, expiry/rejection/relogin, polling, send and storage journeys
continue through the production shell and real Kit controls. Layout, Kit Root,
semantic IDs, labels/tooltips and original list/row rendering are retained.

Legacy composer/history assertions referencing removed shell fields were migrated
to actual typing, clipboard, caret keys, wheel input and semantic row bounds rather
than exposing private child fields. Existing authoritative conversation observations
remain for message counts/identity and reconciliation. The separate original list
feasibility probes remain. No scenario was deleted, and no production history,
pagination, send, polling or timeout algorithm was replaced.

TDD: the notification tracer failed on the missing subscription interface, then
passed; the workspace tracer failed on missing WorkspaceView, then passed after
extraction and the queued-edit boundary fix. Initial view runs exposed render-time
textarea overwrites; the migrated real-control scenarios caught them before the
fix. The recreation fixture now lets an ordinary input event settle before dropping
its view, matching native event boundaries. Intermediate checks caught Rope API
usage and migrated fixture imports; all were fixed, not waived. Original failed
logs are retained with the successful evidence.

## Checks and preservation

From `client-gpui/`, existing lockfile/cache, 2026-09-29 UTC:

- Repeated `cargo check --locked --all-targets`: passed.
- Targeted feature-notification, workspace and all-view checks: passed (48 views).
- `SEED=1,2,3`: workspace (2), composer (4), history (6) each passed.
- `cargo fmt --check`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo test --locked`: **153 passed, 0 failed/ignored**.
- `cargo build --locked`: passed; compiled only, not launched.
- `git diff --check` and all six original SHA-256 checks: passed.

Original modified `.gitignore`, `CONTEXT.md`, `client-gpui/README.md`, and untracked
`client-gpui/ARCHITECTURE.md`, `docs/plans/client-gpui-prototype.md`, and
`docs/plans/client-gpui-rearchitecture.md` are byte-preserved and never staged.
No server, dependency/lockfile, API, storage, session, runtime, pure conversation
state/polling, baseline or prior migration-document changes. No desktop automation,
real credentials/keyring, issue mutation, push, branch switch, stash or reset.
Native IME, accessibility/physical input, delayed native pixel anchoring and
locked/slow real-wallet limitations in `VERIFY.md` remain unverified.

Artifacts: `/tmp/hamlet-orchestrator/review-58/` (original start, live issue JSON,
preservation copies/hashes, logs, commit list, scoped diff and handoff). Parent owns
mandatory fresh parallel Standards/Spec reviews **after the scoped commit**, then
any issue closure. No nested delegation. No substantive blocker; parent review is
the expected handoff. Do not begin #59 here.
