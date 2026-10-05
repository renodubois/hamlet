# GPUI client architecture

This document is the canonical guide to client module ownership and where to add functionality. Keep it current as the implementation evolves. The client is one Rust crate. See [VERIFY.md](VERIFY.md) for automated checks, native verification guidance and outstanding checks.

## Goals

- Make it obvious where to add a view, server request, behavior, or storage operation.
- Keep state, interaction, and rendering together rather than distributing a root monolith across files.
- Keep complex behavior behind small module interfaces. Parent views compose children without manipulating their internals.
- Preserve current functionality, security properties, asynchronous behavior, and meaningful test coverage.
- Avoid replacing the root view with another all-purpose owner or introducing a generic application framework.

## Implemented directory structure

```text
client/src/
├── main.rs                     # Startup, dependency construction, window creation
├── runtime.rs                  # Internal async execution/timer bridge and task lifetime support
├── theme.rs                    # Shared colors and bundled icon mappings
│
├── views/                      # Rendering and local interaction state
│   ├── mod.rs
│   ├── app_shell.rs             # Chooses login vs authenticated workspace
│   ├── login.rs                 # Login/signup inputs, focus, and form interaction
│   ├── workspace.rs             # Composes sidebar and conversation
│   ├── channel_sidebar.rs      # Channel navigation and creation form
│   └── conversation/
│       ├── mod.rs              # Conversation view and layout
│       ├── message_history.rs  # Scrolling, selection, viewport anchoring
│       ├── message_row.rs      # Message presentation
│       └── composer.rs         # Textarea and keyboard interaction
│
├── session/                    # Authentication and session lifecycle
│   ├── mod.rs                  # Coordinates login, logout, expiry, active context
│   ├── state.rs                # Pure authentication state and transition rules
│   └── saved_login.rs          # Credential save/restore/delete workflows
│
├── conversation/               # Conversation behavior and in-memory state
│   ├── mod.rs                  # Coordinates requests and applies their results
│   ├── state.rs                # Channels, history, drafts, send/reconciliation rules
│   ├── live_updates.rs         # Pure readiness and fixed reconnect policy
│   └── delivery.rs             # Bounded executor bridge with terminal priority
│
├── api/                        # Canonical home for server communication
│   ├── mod.rs                  # Public server-client interface and exports
│   ├── client.rs               # ServerClient, AuthenticatedClient, shared HTTP transport
│   ├── auth.rs                 # Login, signup, current user, logout requests
│   ├── channels.rs             # Channel requests
│   ├── messages.rs             # History and message requests
│   ├── events.rs               # Bound SSE attempt, parsing, deadlines and bounded delivery
│   ├── error.rs                # Typed request failures
│   ├── types.rs                # Client-facing server data: user, channel, message, page
│   └── wire.rs                 # Private wire request/response representations
│
└── storage/                    # Durable local storage mechanics
    ├── mod.rs                  # Storage interface and serialized worker coordination
    ├── credentials.rs          # Secret Service adapter and account key construction
    └── preferences.rs          # Token-free configuration and file operations
```

Test files/directories are omitted from this tree; their placement is described below. `runtime.rs` owns execution/time support; `api/types.rs` owns client-facing server data.

## Ownership and dependency rules

### Views

A view owns its rendering and local interaction state: GPUI input entities, focus, subscriptions, selection, scroll position, and local display choices. A substantial stateful view normally has its own file and entity. A rendering helper such as a message row does not need its own entity merely because it has a file.

- `AppShell` observes session status and creates/removes the appropriate screen. Window activation does not affect live delivery. It displays session/storage feedback, including retries that remain relevant after logout. Its small delivery loops apply opaque session/conversation updates; only the coordinators interpret results or decide subsequent requests. Workspace recreation does not create a second delivery loop.
- `LoginView` owns editable server/username/password inputs and login/signup mode. It submits values through the session interface; it does not own authentication, bearer tokens, or secure storage.
- `WorkspaceView` composes authenticated UI against a session-scoped conversation module. It does not implement HTTP dispatch or history merging.
- `ChannelSidebarView` owns its creation input and presents channel state. Selection, ordering, and creation policy belong to conversation behavior.
- `MessageHistoryView` owns the GPUI list, anchoring, selection/copy, and jump-to-latest interaction. It requests older data through conversation behavior rather than fetching pages itself.
- `ComposerView` owns the textarea and input subscriptions. The conversation module owns per-channel drafts, pending sends, and uncertainty. Synchronizing the displayed text must not create an independent authoritative draft store.

The login form's editable password must be cleared on accepted authentication and on server changes, and must not remain in a hidden retained view. This is lifecycle cleanup, not a guarantee of memory zeroization. Form values are supplied at submission; the session model must not maintain a second long-lived editable password copy.

### Session

The session module owns authentication validation/transitions, current user/expiry, session generation, and the active authenticated client. It coordinates server requests, expiry, restoration, secure saving, cleanup, and revocation.

Saved-login workflows outlive the login view and authenticated workspace. A deletion retry for an older account must not disappear merely because a new view or session is created. Secure-store outcomes and server-revocation outcomes remain separately observable.

`state.rs` stays ordinary Rust without GPUI, HTTP execution, or storage side effects. The coordinator may use GPUI entities/notifications to integrate with the application; it does not render. `saved_login.rs` owns workflow decisions, while `storage/` owns provider/file mechanics and ordering.

### Conversation

The conversation module owns loaded channels, selected channel, history, drafts, pending operations, uncertainty, and reconciliation. It belongs to one authenticated session, not to the currently selected channel's view. `ConversationHandle` clones share one coordinator; `read()` gives a read-only state borrow. Pure transition methods and request identities are visible only inside conversation ownership, not exported to views. API data comes directly from `api/`.

Its coordinator starts initial channel/history reads independently of stream readiness and owns one session stream. Creations merge by identity only into loaded channels/history; creations for unloaded data or during a replacing read are ignored, not staged. HTTP confirmations remain operation-aware. Failed stream attempts reconnect after a fixed three seconds without canceling HTTP work, issuing catch-up reads or resetting histories, pages, cursors, drafts, selection or reading position. Read identities belong to the session/request, not a stream attempt. Read failures remain read failures; older pages retain local retry. Uncertainty causes no reads or resends. Live updates are best-effort: disconnected and read-overlap creations may be missed permanently, while future delivery resumes after readiness. There is no replay, polling or manual Refresh path.

`state.rs` and `live_updates.rs` stay ordinary Rust. The coordinator can use GPUI notifications without moving rendering into this module. It reports authoritative authentication rejection with the originating session identity; it does not mutate session internals.

### API client

`api/` is the canonical place to understand what communication with a Hamlet server is possible and how it works. Endpoint paths, URL validation, headers, bearer attachment, serialization, decoding, redirect/TLS policy, and transport timeouts live here. No view, session workflow, or conversation workflow constructs HTTP requests.

The interface has two stages:

1. `ServerClient` binds a validated server URL to a reusable HTTP transport. It supports login and signup.
2. Successful authentication supplies an `AuthenticatedClient` plus public user/expiry information. The client binds the server and bearer credential for that session; callers request channels/history/send without resupplying URL/token.

Saved-login restoration constructs a private candidate client, verifies current user and expected identity/expiry, and only then publishes an active authenticated context. A timeout is not authoritative rejection.

Authenticated clients are immutable with respect to server and credential. Cloning a handle shares transport/context; it does not read a globally replaceable token. New authentication creates a new context. Dropping the active handle alone does not cancel all work or make old completions safe: session generations and request identities still gate results.

Only session persistence needs controlled access to credential material; views and conversation state do not. Secret-bearing types must not expose secrets through debug formatting or logging. There is no automatic replay of writes or implicit reauthentication.

`types.rs` contains small client-facing server data types, not session state or GPUI types. `wire.rs` remains private and may differ from those types. Consumers use exports from `api/`; the API module must not depend on `session/` or `conversation/` internals. Typed errors describe failures; feature-specific user feedback belongs to the feature, not to an auth-specific catch-all error description.

Keep a substitution seam for controlled request outcomes in tests, alongside the real HTTP adapter. Common client binding rules must apply to both. Do not expose raw transport details to application callers for the sake of tests.

The [authenticated live-update operation](LIVE-UPDATES.md) owns one cancel-on-drop stream attempt, validated ready/creation deliveries, separate stream deadlines and bounded delivery with prioritized terminal outcomes. It uses the same execution/time support and entity conversion as ordinary operations, but a separate HTTP client without a total-body timeout. It does not own reconnection or conversation state. The conversation coordinator owns the handle and executes the pure live lifecycle; views never own streams or reconnect decisions.

### Storage

`storage/` owns the dedicated blocking Secret Service worker, configuration format/path, account keys, and ordered storage operations. Splitting credential and preference code must not split the worker's ordered save/metadata/delete protocol into independent competing writers.

Store bearer credentials only in Secret Service; configuration files contain public metadata and pending deletion intents. Resume pending deletion work after restart without restoring the deleted identity. Message history and drafts remain in memory.

### Startup and runtime

`main.rs` constructs dependencies, initializes GPUI/Kit, creates the window/root, and starts the application. It contains neither workflows nor feature tests.

`runtime.rs` is a small internal home for the existing Tokio-to-GPUI execution bridge and controllable execution/time support. It knows nothing about login, messages, or views. Feature coordinators own tasks and decide what results mean. HTTP deadline policy belongs to `api/`, secure-store deadlines to saved-login coordination, and the fixed reconnect delay to `conversation/live_updates.rs`.

Use injected execution/time support where required for deterministic headless tests. Production and tests must exercise the same workflow code rather than maintain separate `cfg(test)` request-dispatch implementations. Do not expand this into a general event bus or command framework.

### Dependency direction

```text
main -> constructs runtime, API, storage and root (AppShell constructs the session)
views -> session/conversation interfaces and child views
session coordinator -> API, storage, session state, runtime
conversation coordinator -> authenticated API, conversation state, live updates, runtime
API/storage/runtime -> no view or feature-coordinator dependencies
pure state modules -> data and transition rules, no side effects
```

Cross-feature coordination uses narrow session-status/rejection interfaces. Do not pass the whole root view or mutable `AppSession` into child views or conversation state. Session invalidation synchronously prevents further protected work and clears authenticated state before asynchronous cleanup finishes.

## Adding functionality

| Change | Home |
| --- | --- |
| New screen, panel, or dialog | `views/<name>.rs` |
| Private subviews of a growing view | Promote to `views/<name>/`, with composition in `mod.rs` |
| Truly reused presentation | `views/shared/`, when there are actual users |
| Message layout or composer shortcuts | Relevant file in `views/conversation/` |
| History continuity or send uncertainty | `conversation/state.rs` |
| Stream lifecycle, reconnect, or older-page reads | Conversation coordinator/live updates |
| Server operation | Relevant `api/` request file and client interface |
| Authentication/restoration policy | `session/` |
| Credential provider or configuration mechanics | `storage/` |
| New longer-lived product behavior | A focused feature module beside `session/` and `conversation/` |

A feature may legitimately touch behavior, API, and views. The goal is predictable ownership, not one file per end-to-end feature. Avoid catch-all `utils`, `manager`, or generic local-data stores. Split growing state modules by cohesive behavior when necessary, not one file per method.

## Tests and verification

### Test layout (required)

All in-crate test suites live in a `tests/` directory within their owning feature, regardless of suite size. Do not add inline test suites, sibling `tests.rs` files, or `*_tests.rs` files beside production code. Name suite files for their subject, without a `_tests` suffix.

```text
src/
├── session/
│   ├── mod.rs
│   ├── state.rs
│   └── tests/
│       ├── coordinator.rs
│       ├── route.rs
│       ├── saved_login.rs
│       ├── binding.rs
│       └── state.rs
├── conversation/tests/         # coordinator, live updates, delivery, route, state
├── api/tests/                  # binding, HTTP
├── storage/tests/              # configuration, protocol, persistence
├── views/tests/                # view and cross-view scenarios
└── test_support/               # fixtures reused across features
```

Filesystem placement does not change Rust module ownership. Declare each suite from the production module it tests, with `#[cfg(test)]` and an explicit path when needed. For example, `session/state.rs` declares:

```rust
#[cfg(test)]
#[path = "tests/state.rs"]
mod tests;
```

This keeps tests as children of their owner with access to private items. Do not move them into an umbrella module or widen production visibility just to accommodate a directory move. A `tests/mod.rs` may group suites that already share the same owner, as in `views/`. Existing Rust module names such as `route_tests` may remain: the filename policy does not require changing test names or Cargo filters.

Keep single-suite helpers in that suite. Helpers shared within a feature belong in its `tests/support/`; fixtures shared across features belong in crate-private `src/test_support/`. Keep all support test-only. Narrow owner-local `#[cfg(test)]` re-exports and dependency-injection hooks may remain in production modules when required by privacy; they are not test suites and must not duplicate production workflows.

Reserve top-level `client/tests/` for external integration-test crates exercising a library's public API, not as a replacement for these in-crate suites. Feature scenarios using real server routes still belong in their feature's `tests/route.rs`.

When reviewing changes, check suite placement, test-only gating, preserved module ownership, and helper scope alongside behavior and coverage.

### Coverage and execution

- Pure transitions live in `session/tests/state.rs` and `conversation/tests/{state,live_state}.rs`; binding transitions stay under session state ownership. The best-effort connection policy is exercised through `ConversationHandle` in `conversation/tests/live_coordinator.rs`. Real-route feature integration lives in each feature's `tests/route.rs`, not in API tests.
- Feature tests exercise their owned interfaces, with controlled request outcomes and time.
- Cross-view GPUI tests belong under `views/tests/`, independently of `AppShell`; use real Kit controls, stable semantic IDs, and a Kit `Root`, not private child fields or a fixed presentation tree. Test construction calls the production `app_shell::open`; there is no test-only root constructor.
- API tests cover loopback HTTP fixtures and the server routes. Keep real decoding/header/redirect/deadline coverage in addition to controlled adapters.
- Storage tests use isolated files and a controlled provider, preserving worker ordering/race coverage.
- Production construction explicitly accepts dependencies; tests do not select a different application lifecycle.
- Preserve meaningful scenario coverage when reorganizing tests, rather than preserving every private-field assertion or test count.

Use the commands in [OVERVIEW.md](OVERVIEW.md) and the checks/native smoke guidance in [VERIFY.md](VERIFY.md). Native IME, accessibility, delayed pixel anchoring, and locked/slow real-wallet limitations remain explicit until independently verified.

## Scope

The client supports HTTP writes/history and best-effort authenticated SSE creations with automatic reconnect, in-memory history/drafts and a selected saved login. Offline storage, queued writes, multi-server navigation, packaging and cross-platform support are not implemented.
