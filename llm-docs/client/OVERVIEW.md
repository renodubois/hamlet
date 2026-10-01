# Hamlet desktop client

Linux desktop client built with GPUI/Kit. It supports signup/login, text channels, paginated conversation history, per-channel drafts, message publication, focused polling and saved sessions through Linux Secret Service.

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

## Channels and conversations

- Create a text channel with **Channel name** and **Create text channel**. Names are trimmed, accept 1–64 bytes of ASCII letters, digits, spaces, hyphens or underscores, and are unique ignoring ASCII case. Confirmed creation selects the channel; uncertain creation is not replayed automatically.
- Select a channel to load its newest history. Scroll upward for older cursor pages; **Retry older messages**, **Start of conversation** and **Jump to latest** distinguish traversal states. Messages retain line breaks, author and timestamp; text is selectable/copyable.
- **Enter** sends; **Shift+Enter** inserts a newline. The **Send message** button is another submission path. Drafts are held per channel in memory and cleared on logout, expiry or restart. A pending send locks only its originating composer.
- Focused clients poll selected-channel history about every 3 seconds and channels about every 15 seconds. Polling pauses while unfocused; focus return reconciles immediately. Read failures back off independently up to 60 seconds. **Refresh conversation** and **Refresh channels** remain available.
- Catch-up traverses cursor pages, deduplicates by message ID and retains server order for timestamp ties. Loaded history and drafts survive read outages; incomplete catch-up is reported separately.
- Writes are never retried automatically. An uncertain send retains its draft and warning: check the conversation before deliberately resending. Exactly-once publication is not guaranteed without server idempotency.

## Saved sessions

The selected login's bearer credential lives in Secret Service under `org.hamlet.session.v1`. `$XDG_CONFIG_HOME/hamlet/session.json` (or `~/.config/hamlet/session.json`) contains only server URL, public user identity, expiry and pending deletion intents; never passwords, tokens, message history or drafts. One selected login is saved per profile.

Startup pre-fills the selected server and verifies a saved credential through `GET /api/v1/me` before opening conversations. Timeout leaves restoration retryable; authoritative rejection or known expiry requests deletion. Server changes invalidate local conversations and schedule cleanup of the previous selected credential.

Save, restore, deletion and revocation report confirmed, failed or unconfirmed outcomes distinctly. The blocking storage worker serializes operations so deletion follows an in-flight save, even after a UI timeout. **Retry saved-login restoration** and **Retry saved-login deletion** are available when applicable; timeout alone does not prove invalidity or successful deletion.

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for module ownership and dependency rules, and [PRESENTATION.md](PRESENTATION.md) for editing locations and semantic controls.

`src/main.rs` handles startup. `views/` owns controls and subscriptions; `session/` owns authentication, restoration, expiry and cleanup; `conversation/` owns session-scoped requests, polling, history and drafts. `api/` owns bound HTTP clients, `storage/` owns the ordered provider/configuration worker, `runtime.rs` provides shared execution/time support, and `theme.rs` defines colors/icons.

Tests live in the owning feature's `src/<feature>/tests/` directory, with cross-feature fixtures in `src/test_support/`. Follow the required [test layout](ARCHITECTURE.md#test-layout-required).

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

No packaging, push transport, offline/queued writes, persistent history/drafts, rich formatting or cross-platform support is provided.
