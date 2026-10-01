# Client presentation

The client uses Kit Root, semantic IDs, labels and tooltips. Change presentation at the owning view, not in startup or a feature
coordinator. [ARCHITECTURE.md](ARCHITECTURE.md) describes the dependency rules.

| Editing responsibility | Location | Stable controls |
| --- | --- | --- |
| Startup/window/Kit Root | `src/main.rs` | No feature controls or workflows |
| Screen composition, shared session/storage feedback and retry | `src/views/app_shell.rs` | `session-status`, `logout`, `auth-feedback`, `storage-status`, `retry-storage` |
| Login/signup fields, form mode, focus and sensitive cleanup | `src/views/login.rs` | `server-url`, `username`, `password`, `auth-mode`, `login`, `signup`, `auth-feedback` |
| Pane composition and connection-status presentation | `src/views/workspace.rs`, `src/views/conversation/mod.rs` | `connection-status` |
| Channel list and creation input | `src/views/channel_sidebar.rs` | `channels`, `channel-{id}`, `channel-name`, `create-channel`, `channel-feedback`, `refresh-channels`, `channels-refresh-error` |
| List/focus, wheel, viewport anchoring and traversal controls | `src/views/conversation/message_history.rs` | `history-pane`, `history`, `refresh-history`, `catchup-incomplete`, `retry-older`, `jump-latest` |
| Plain selectable message, author and timestamp | `src/views/conversation/message_row.rs` | `message-{id}`, `message-text-{id}`, `text-{id}` |
| Textarea, keyboard, focus, displayed-draft synchronization | `src/views/conversation/composer.rs` | `composer-panel`, `composer`, `send-message`, `send-feedback` |
| Colors and bundled Hash/Send icons | `src/theme.rs` | Keep meaningful labels/tooltips |

## Behavior boundaries

Views issue intentions to `SessionCoordinator` or `ConversationHandle`; they do
not dispatch HTTP, interpret pagination continuity, hold bearer tokens or own
saved-login deletion identities. `session/` owns login/signup, verified restoration,
expiry, rejection and independent cleanup/revocation. `session/saved_login.rs`
retains old-identity cleanup and retry across screens/newer sessions. `storage/`
owns the ordered blocking provider/configuration protocol; feedback must distinguish
memory-only, confirmed and unconfirmed outcomes.

`conversation/mod.rs` owns requests, task cancellation and completion identities.
`conversation/state.rs` owns server-order selection, history continuity, deduplication,
creation, per-channel drafts and send uncertainty. `conversation/polling.rs` owns
focused 3s/15s scheduling and independent backoff to 60s. Manual refresh, focus
return, scheduled reads and safe post-send catch-up share the same workflow;
neither views nor runtime implement a second dispatch path. `api/` alone builds
routes/headers and decodes responses. HTTP 8s, protected read/send 9s and combined
storage/restoration 10s deadlines remain distinct.

Each child owns its input/list entities and subscriptions. Feature invalidations
also arrive after timer completions and while children are hidden. Recreation
hydrates current authoritative state without issuing requests. The shell never
reads a child's password, textarea or list. Invalidation synchronously closes
protected activity and clears all authoritative history/drafts; retained hidden
controls then clear through their notification/window-update path. This is not
memory zeroization.

The history view splices chronological stable row IDs, preserving variable-height
anchors for prepend and middle insertion. It follows only when appropriate;
**Jump to latest** explicitly resumes bottom-follow. Selection/copy uses Kit
`SelectableText` and Root. The composer keeps only its displayed-channel/text
projection; drafts remain in conversation ownership. Hydration must not overwrite
queued edits, move the caret on unchanged notifications, or let an old Enter send
the newly selected channel's draft. Enter sends unchanged text; Shift+Enter adds
a line. A pending send locks only its originating composer. Uncertain sends retain
draft/warning and never automatically replay; equal text does not prove delivery.

## Checks and native boundary

Run from `client/`:

```sh
cargo test --locked views::
cargo test --locked history
cargo test --locked composer
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

Cross-view suites under `src/views/tests/` mount Kit Root and drive real controls,
keyboard, wheel and clipboard by semantic IDs; they do not navigate private child
fields or assert a fixed presentation tree. They cover production wheel/prepend
anchoring and exact multiline selection/copy.

[VERIFY.md](VERIFY.md) describes the native checklist and outstanding checks.
The latest recorded desktop attempt was blocked by a securely locked desktop
after isolated login-form rendering. Private provider registration alone does
not verify credential operations; native acceptance remains incomplete.

IME candidate/Enter, assistive technology/physical keyboard, precise delayed
native prepend anchoring and locked/slow real-wallet checks remain unverified.
Headless results are not native acceptance. Obtain separate consent, establish
isolated provider resources/profiles and check the desktop is already unlocked
**before** launching automation; follow the [safety gate](VERIFY.md#native-safety-gate).
Do not bypass a desktop lock or use the ordinary wallet to complete a demo.
