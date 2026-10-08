# Hamlet desktop client

Linux desktop client built with GPUI/Kit. It supports signup/login, text channels, paginated conversation history, per-channel drafts, message publication, session-owned live updates and saved sessions through Linux Secret Service.

## Linux prerequisites

Install Rust, a C compiler and pkg-config, plus development libraries for D-Bus, fontconfig, freetype, xkbcommon, Wayland/X11 and a Vulkan loader/driver. Run in a graphical X11 or Wayland session. Other platforms are not verified; persistent sessions require the Linux Secret Service backend.

For saved sessions, provide a running session D-Bus with an unlocked `org.freedesktop.secrets` provider such as GNOME Keyring or KWallet. The client uses `keyring`'s `sync-secret-service` backend, not a plaintext or mock fallback. A missing, locked or unavailable provider can leave the session memory-only or its persistence unconfirmed.

## Run

Start the [server](../server/OVERVIEW.md), then run from `client/`:

```sh
cargo run --locked
```

The login form defaults to `http://127.0.0.1:8081`. For that URL, start the server with `HAMLET_BIND=127.0.0.1:8081`; otherwise enter its actual base URL. Remote servers require HTTPS; HTTP is allowed only for localhost/loopback. Certificates are validated, redirects are not followed, and HTTP requests have an eight-second timeout.

Select **New user? Sign up** to create a user and immediately enter a session, or log in with an existing username/password. Usernames accept 3–32 ASCII letters, digits, `_` or `.`; passwords are 8–256 bytes. Invalid credentials remain editable. **Log out** clears local authenticated state immediately and separately requests saved-credential deletion and server revocation.

## Development rebuild/restart

From the repository root:

```sh
# Install once (or install Watchexec through your package manager).
cargo install watchexec-cli --locked

./client/dev.sh
```

`client/dev.sh` requires Bash, Rust/Cargo, Watchexec and `setsid` (util-linux on Linux). It works from any working directory and builds on startup, then watches client source/manifests and shared protocol source/manifest. Saves are debounced; changes during compilation queue another build. The current window remains usable while compiling and after build errors. Only a successful build stops the old client and starts the new one. Restarts reset in-memory state, including drafts. Ctrl+C stops the watcher, build and client processes owned by that invocation. Closing the client window does not stop the watcher; the next successful build opens it again.

The script forces worktree-local configuration at `client/.env.dev-config` and native debug build output under `client/target/<host-target>/debug`, regardless of inherited `XDG_CONFIG_HOME`, `CARGO_TARGET_DIR` or `CARGO_BUILD_TARGET`. It neither starts nor stops the server. Run a separate worktree-local server on an unused loopback port, then set the client's Server URL to that endpoint. Start with fresh configuration; never copy session metadata or pending deletions from another environment. Different endpoints have different credential keys, but configuration isolation alone does not isolate Secret Service. Agent-driven desktop automation or real-keyring access still requires separate consent and the [native safety gate](VERIFY.md#native-safety-gate).

## Channels and conversations

- Choose **Create text channel** to open a dialog, enter **Channel name**, then create it. The dialog stays open while creating and on errors, and closes on confirmed success; **Cancel** or Escape discards its input without canceling an already submitted request. Names are trimmed, accept 1–64 bytes of ASCII letters, digits, spaces, hyphens or underscores, and are unique ignoring ASCII case. Confirmed creation selects the channel; uncertain creation is not replayed automatically.
- Select a channel to load its newest history. Scroll upward for older cursor pages; **Retry older messages**, **Start of conversation** and **Jump to latest** distinguish traversal states. Messages retain line breaks, author and timestamp; text is selectable/copyable.
- **Enter** sends; **Shift+Enter** inserts a newline. Drafts are held per channel in memory and cleared on logout, expiry or restart. A pending send locks only its originating composer.
- One authenticated stream remains active while unfocused. Initial reads do not wait for stream readiness. Creations merge by identity into loaded channels/history without follow-up reads; events for unloaded data or during a replacing read are ignored. No polling, Refresh controls or manual connection retry remain.
- Delivery loss reconnects after a fixed three seconds, without catch-up reads or state resets. Loaded history, older pages/cursors, drafts, selection and reading position remain; navigation and HTTP writes stay available. **Live updates disconnected — reconnecting.** persists until the new stream is ready. This is best-effort delivery: creations missed while disconnected or overlapping a replacing read may remain absent permanently; readiness resumes future delivery, not synchronization. Older-page failures still offer local retry.
- Writes are never retried automatically. An uncertain send retains its draft and warning: check the conversation before deliberately resending. Exactly-once publication is not guaranteed without server idempotency.

## Saved sessions

The selected login's bearer credential lives in Secret Service under `org.hamlet.session.v1`. `$XDG_CONFIG_HOME/hamlet/session.json` (or `~/.config/hamlet/session.json`) contains only server URL, public user identity, expiry and pending deletion intents; never passwords, tokens, message history or drafts. One selected login is saved per profile.

Startup pre-fills the selected server and verifies a saved credential through `GET /api/v1/me` before opening conversations. Timeout leaves restoration retryable; authoritative rejection or known expiry requests deletion. Server changes invalidate local conversations and schedule cleanup of the previous selected credential.

Save, restore, deletion and revocation report confirmed, failed or unconfirmed outcomes distinctly. The blocking storage worker serializes operations so deletion follows an in-flight save, even after a UI timeout. **Retry saved-login restoration** and **Retry saved-login deletion** are available when applicable; timeout alone does not prove invalidity or successful deletion.

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for module ownership and dependency rules, and [PRESENTATION.md](PRESENTATION.md) for editing locations and semantic controls.

`src/main.rs` handles startup. `views/` owns controls and subscriptions; `session/` owns authentication, restoration, expiry and cleanup; `chat/` owns session-scoped requests, stream reconnect, history and drafts. `api/` owns bound HTTP clients, `storage/` owns the ordered provider/configuration worker, `runtime.rs` provides shared execution/time support, and `theme.rs` installs the One Dark theme and defines palette constants/icons. Edit `theme.rs`'s `config()` role assignments to tune backgrounds, text, primary/secondary actions, and status colors; unspecified settings use Kit's dark defaults, with hover/pressed colors derived automatically.

`views/chat.rs` is a small composition module; sidebar, conversation and account-footer views live directly under `views/`, with their internals kept private. Its constructor receives existing session/chat handles; it does not change their lifetimes or request behavior.

Tests live in the owning feature's `src/<feature>/tests/` directory, including `src/views/tests/` for chat composition and `src/views/conversation/tests/` for conversation-local suites. Cross-screen/application journeys stay under `src/views/tests/`; cross-feature fixtures live in `src/test_support/`. Follow the required [test layout](ARCHITECTURE.md#test-layout-required).

## Verification

From `client/`:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked

# Focused suites
cargo test --locked views::
cargo test --locked production_wheel_requests_older_and_keeps_reader_at_same_viewport_y
cargo test --locked production_message_is_selectable_and_copyable
cargo test --locked route_tests
```

Tests use controlled execution/time, real Kit controls, loopback HTTP, isolated files/fake providers and disposable server databases. Building does not launch the client. Automated checks are not native desktop acceptance; IME, accessibility, precise delayed-prepend anchoring and locked/slow real-wallet behavior need separate verification. See [VERIFY.md](VERIFY.md) for the native checklist and [safety gate](VERIFY.md#native-safety-gate). Do not launch desktop automation or access a real keyring without separate consent.

No packaging, offline/queued writes, persistent history/drafts, rich formatting or cross-platform support is provided.
