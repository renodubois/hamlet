# Prototype-exit verification (#46)

## Results in this environment

Linux x86_64, kernel 7.2.7-arch1-1, rustc/Cargo 1.95.0. `DISPLAY` and `WAYLAND_DISPLAY` were unset; `tty` returned `not a tty`. Native development libraries available via pkg-config: dbus-1 1.16.2, fontconfig 2.18.3, freetype2 26.6.20, xkbcommon 1.13.2, wayland-client 1.26.0, X11 1.8.13, Vulkan 1.4.357. From `client-gpui/`, these commands **passed**:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked                  # 97 passed, 0 failed, 0 ignored
cargo build --locked
```

Before the full suite, `cargo fmt` and four new individually filtered `cargo test --locked <test-name>` runs passed, as did `cargo test --locked polling_and_send_confirmation_share_one_headless_history_without_duplicate`. The complete suite includes application-behavior, real Kit/GPUI control and scrolling tests, loopback transport fixtures, and HTTP tests starting the **unchanged** `hamlet::routes` on temporary SQLite databases and ephemeral ports. No externally running server, desktop, real Secret Service, or two physical clients were involved. `git diff --name-only -- server-rewrite` returned nothing; no rewrite-server source or contract was changed. Inspection of `client-gpui/src` found no `println!`, `eprintln!`, `dbg!`, `tracing::` or `log::` calls. The HTTP adapter does not enable request/body logging; config serialization includes server, public user identity, expiry and deletion intents but not password, token, message text or drafts. Tests check that signup's password/token are absent from metadata, but source inspection and tests are **not** an audit of every dependency, server/proxy log or native provider.

Controlled cross-feature coverage: poll-before-confirmation and confirmation-before-poll deduplicate by server message ID and require a safe post-send read; uncertain publication during a channel switch leaves the originating draft/warning intact, reconciles only on selection and never replays the POST; logout/expiry with pending polls and sends discard late outcomes; old-server 401s cannot invalidate a new session. The headless test drives actual login/composer/send controls and `poll_at` across the confirmation race. A headless integration test clicks the Kit signup controls, follows `Hamlet::submit` → `enter_authenticated` → `save_login` through a real persistence worker backed by a **controlled Store**, checks metadata and token separation, constructs a fresh view from the loaded selection, invokes `start_restore` and verifies the returned identity via its `AuthApi::current_user` (`/me`) fixture, then logs out and checks deletion/revocation. Separately, a real-route integration test uses `AppSession` (not the Kit controls), saves through a controlled worker, verifies the token via HTTP `/me`, restores through `finish_restore`, then logs out, deletes and revokes. No single test crosses both the GPUI controls and the actual rewrite-server `/me` route. Other tests exercise storage failures, deletion ordering, and stale restoration. These outcomes do not prove native Secret Service or IME behavior.

## Reproduce on a graphical Linux desktop (manual checklist — **not run here**)

Install Rust and a compiler/pkg-config plus distribution development packages for D-Bus, fontconfig, freetype, xkbcommon, Wayland/X11 and Vulkan loader/driver. Run under a working graphical X11/Wayland session with an unlocked `org.freedesktop.secrets` provider (e.g. GNOME Keyring/KWallet) on session D-Bus. `keyring` 3.0.4 is compiled with `sync-secret-service`, not a plaintext/mock fallback. Have a clipboard and an installed/configured IME for the native checks. Use only a disposable local database: the rewrite server has open signup and broad authenticated access, **not** production admission/security.

```sh
# Terminal 1, from server-rewrite/ (default database data/hamlet.db;
# optional HAMLET_DATABASE_URL=sqlite:///absolute/path?mode=rwc).
# Explicit bind: server-rewrite/src/main.rs currently defaults to 127.0.0.1:3001,
# although server-rewrite/README.md says 8081; the client's default is 8081.
HAMLET_BIND=127.0.0.1:8081 cargo run --bin hamlet
# Terminals 2 and 3, from client-gpui/, in the same graphical session:
cargo run --locked
cargo run --locked
```

Follow `server-rewrite/README.md` for database setup and prerequisites, but use the explicit `HAMLET_BIND` above to avoid its default-port documentation mismatch. If changing the server URL on the login form, both clients must use the **same** URL; non-loopback HTTP is rejected, remote use requires HTTPS. For separate stored identities, run clients with distinct desktop user profiles/Secret Service stores and `XDG_CONFIG_HOME` values: this prototype stores only *one selected saved login* per profile. Otherwise create Alice and Bob sequentially, use both windows while active, and expect the last save to replace the profile's saved selection. Do not paste tokens/passwords into issue logs or screenshots.

1. Sign up Alice in client A and Bob in B; check both enter a session without a second login. Log out/in again; check invalid credentials remain editable. Create `Test Room` in B and discover it in A via **Refresh channels** and foreground polling (within ~15 seconds); retain A's current selection. Check selected-channel history, author/timestamps, and empty/history loading states.
2. Send a multiline message in B (Shift+Enter then Enter or **Send message**) and observe it once in A via polling (~3 seconds focused), manual refresh and across channel switches. Send over 50 messages while A is unfocused, then refocus and check multi-page catch-up, no duplicates, the older-page scrollbar and **Jump to latest**. Keep distinct unsent drafts in two channels; switch and return. Check reader viewport stays on the same message while new/older variable-height rows arrive.
3. While A has a draft, stop the server. Try both refresh controls: status should report read trouble while loaded history/draft survive. Restart with the **same database**; refocus or refresh, verify recovery without replaying writes. For an uncertain send, interrupt/delay a POST response (stopping before submission alone demonstrates connectivity failure, not proven server-side acceptance). Verify draft/warning remain and check history *before manually resending*. Switch channels while the send is pending/uncertain; only the originating composer should lock, other channels remain usable. Simulate token rejection via server-side revocation or expiry and check login return, draft cleanup, and no late response resurrection.
4. With an unlocked real Secret Service, save after login **and after signup**; wait for the explicit success status, exit and restart, and check `/me` restoration into the same account. Log out: local state clears immediately; confirm both server revocation and removal of the selected item using the provider's credential manager; restart and confirm no restoration. Lock/disconnect the store and repeat to check **memory-only** save failure, unconfirmed timeout/deletion warning and retry behavior. A store timeout cannot guarantee whether a queued save/delete later succeeded; a revocation timeout cannot guarantee token invalidation. Never treat a timeout as authoritative session invalidity.
5. On the physical desktop, select multiline message text with mouse and Ctrl+C into another application; check line breaks. With an IME candidate *not yet committed*, press Enter: it must commit without POST; a subsequent plain Enter must send once. Check Shift+Enter, trailing newline, focus via keyboard/Tab, accessible names and tooltips on icon-first actions, and that slow HTTP/store operations do not freeze interaction. Headless key/clipboard events and controlled blocked-store tests **do not prove** native IME, selection/copy, accessibility, responsiveness or real store integration.

### Reproducible slow HTTP / uncertain-send drill (manual — not run here)

Use **only** a disposable local server database and a fresh desktop OS user/profile with a separate Secret Service store; use fake passwords and messages. Do not use production credentials, bind outside loopback, or capture/log headers/bodies. Keep the server on `127.0.0.1:8081`; this standard-library forwarding proxy listens on `127.0.0.1:8082` and never writes traffic to disk. In a separate terminal run the following (switch the environment variable between runs). The `drop_send` mode forwards the complete POST to the server, then closes the client connection without returning its response; `slow` delays **every** response by four seconds. Stop with Ctrl+C.

```sh
HAMLET_PROXY_MODE=slow python3 - <<'PY'
import http.client, os, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

class Relay(BaseHTTPRequestHandler):
    def log_message(self, *args): pass  # never log paths, headers or payloads
    def do_GET(self): self.relay()
    def do_POST(self): self.relay()
    def relay(self):
        length = int(self.headers.get('Content-Length', '0'))
        body = self.rfile.read(length)
        conn = http.client.HTTPConnection('127.0.0.1', 8081, timeout=15)
        try:
            headers = {k: v for k, v in self.headers.items()
                       if k.lower() not in ('host', 'connection', 'content-length')}
            conn.request(self.command, self.path, body=body, headers=headers)
            upstream = conn.getresponse()
            payload = upstream.read()  # server has accepted the write before dropping reply
            if (os.environ['HAMLET_PROXY_MODE'] == 'drop_send'
                and self.command == 'POST'
                and self.path.split('?', 1)[0].startswith('/api/v1/channels/')
                and self.path.split('?', 1)[0].endswith('/messages')):
                self.close_connection = True
                return
            if os.environ['HAMLET_PROXY_MODE'] == 'slow': time.sleep(4)
            self.send_response(upstream.status)
            for k, v in upstream.getheaders():
                if k.lower() not in ('transfer-encoding', 'content-length', 'connection'):
                    self.send_header(k, v)
            self.send_header('Content-Length', str(len(payload)))
            self.end_headers()
            self.wfile.write(payload)
        finally:
            conn.close()

ThreadingHTTPServer(('127.0.0.1', 8082), Relay).serve_forever()
PY
```

In the client set Server URL to `http://127.0.0.1:8082` **before signup** (the proxy URL is the session identity); sign up two disposable users. In `slow` mode, click Refresh channels and Refresh conversation, type into the composer and switch channels while responses are delayed. Record whether controls remain responsive, existing messages/drafts stay visible, and each response completes after ~4 seconds. Restart the proxy in `drop_send` mode (same URL and database), send a uniquely identifiable message once, wait for the uncertain-send warning, switch away/back, refresh history, and check the message is present exactly once **without clicking Send again**. A dropped reply does not prove acceptance in every failure scenario; this fixture reads a completed upstream response first. Record both the warning and the fetched server message ID; never attach request dumps. Stop the proxy and server after the drill. Avoid `tc` on the host loopback interface: it would delay unrelated local processes.

### Slow/locked secure-store drill (manual — not run here)

In that **disposable desktop OS user** (not the normal keyring), open Seahorse (Passwords and Keys) and create/unlock a test Login keyring before launching the client. Sign up with a fake password, wait for the explicit saved-login status, exit/restart and verify the same identity. In Seahorse lock the Login keyring (right-click **Login → Lock**, if provided by the installed provider); with the store locked, perform another disposable login/signup and record the UI's save status and elapsed time, including whether a prompt, immediate failure or ~10-second unconfirmed timeout occurred. While the operation is pending, type in another control or switch channels to check responsiveness. Unlock the keyring using its own UI; log out, wait for the deletion result, inspect only this disposable user's Hamlet entry in the credential manager (do not reveal its value), then restart to check no restoration. If deletion fails, use **Retry saved-login deletion** after unlocking and check again. If your provider has no lock UI or handles lock by prompting immediately, record that limitation instead of claiming a slow native-store test; the deterministic worker-blocking tests (`save_completing_after_invalidation_is_cleaned_before_deletion_reply`, `delayed_failed_deletions_can_retry_without_touching_new_user`) cover the controlled-delay behavior only. Record provider name/version, desktop/session type, start/end timestamps, exact visible statuses, whether the UI accepted interaction while pending, presence/absence of the selected entry, and restart/revocation outcome. Never paste tokens or passwords into notes.

**Unmet/manual acceptance:** The two-client real-server smoke, native selection/copy, IME-safe Enter, variable-height physical scrolling/reader position, keyboard/accessibility inspection, slow-operation responsiveness, and real Linux Secret Service save/restart/delete (including fallback/revocation uncertainty) remain **unverified here** because there is no graphical session or physical desktop interaction. A maintainer must record actual results before marking those criteria verified. No packaging or non-Linux verification was performed.

## Presentation and future seams

Edit pane layout and semantic control wiring in `src/main.rs` (`Hamlet::render`, `conversation_panes`); replace color constants and bundled channel icon in its `theme` module (the send icon mapping is currently inline in `conversation_panes`). See `PRESENTATION.md` for control IDs, selectable history and focus notes. Behavioral tests use `AppSession`, `Conversation`, `Polling`, `AuthApi` and persistence outcomes. Headless tests address controls by semantic IDs and discover current bounds for scroll/selection; they do **not** assert fixed pixels or a presentation-specific tree path. Preserve meaningful action names/labels/tooltips and semantic IDs when restyling.

`src/polling.rs` owns the focus/interval/backoff schedule; `Conversation::refresh_history` and `complete_history` own gap-free selected-channel reconciliation and identity merging, while `src/http.rs` owns transport and `src/main.rs` dispatches async effects and preserves the GPUI list anchor. A future push adapter could trigger the existing selected-channel reconciliation without rewriting message rows, **but** the view currently directly owns poll timers/focus and read task cancellation: push subscriptions and reconnect lifecycle need explicit integration there. Unread indicators would need per-channel unread state and read-position policy (currently only selected history is fetched); typing would need ephemeral per-channel presence, transport and view state not represented by `Conversation` or server routes. Reconnect currently means retrying backed-off HTTP **reads**, not socket continuity or queued writes. No speculative push/unread/typing framework was added.

Prototype limitations: unchanged rewrite server; uncertain sends cannot guarantee exactly-once delivery without server idempotency, so there is no automatic write replay. No packaging, cross-platform verification, persistent history/drafts, queued/offline writes, rich message formatting or finished visual identity. Open signup and broad server access are explicitly **not** production readiness.
