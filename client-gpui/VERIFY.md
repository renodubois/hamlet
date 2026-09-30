# Prototype-exit verification (#46)

## Results in this environment

Linux x86_64, kernel 7.2.7-arch1-1, rustc/Cargo 1.95.0. In the automated-check harness, `DISPLAY` and `WAYLAND_DISPLAY` were unset and `tty` returned `not a tty`. A separate, consented desktop smoke below explicitly connected to an existing niri/Xwayland session; do not confuse it with these automated results. Native development libraries available via pkg-config: dbus-1 1.16.2, fontconfig 2.18.3, freetype2 26.6.20, xkbcommon 1.13.2, wayland-client 1.26.0, X11 1.8.13, Vulkan 1.4.357. From `client-gpui/`, these commands **passed**:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked                  # 97 passed, 0 failed, 0 ignored
cargo build --locked
```

Before the full suite, `cargo fmt` and four new individually filtered `cargo test --locked <test-name>` runs passed, as did `cargo test --locked polling_and_send_confirmation_share_one_headless_history_without_duplicate`. The complete suite includes application-behavior, real Kit/GPUI control and scrolling tests, loopback transport fixtures, and HTTP tests starting the **unchanged** `hamlet::routes` on temporary SQLite databases and ephemeral ports. No externally running server, desktop, real Secret Service, or two physical clients were involved. `git diff --name-only -- server-rewrite` returned nothing; no rewrite-server source or contract was changed. Inspection of `client-gpui/src` found no `println!`, `eprintln!`, `dbg!`, `tracing::` or `log::` calls. The HTTP adapter does not enable request/body logging; config serialization includes server, public user identity, expiry and deletion intents but not password, token, message text or drafts. Tests check that signup's password/token are absent from metadata, but source inspection and tests are **not** an audit of every dependency, server/proxy log or native provider.

Controlled cross-feature coverage: poll-before-confirmation and confirmation-before-poll deduplicate by server message ID and require a safe post-send read; uncertain publication during a channel switch leaves the originating draft/warning intact, reconciles only on selection and never replays the POST; logout/expiry with pending polls and sends discard late outcomes; old-server 401s cannot invalidate a new session. The headless test drives actual login/composer/send controls and `poll_at` across the confirmation race. A headless integration test clicks the Kit signup controls, follows `Hamlet::submit` → `enter_authenticated` → `save_login` through a real persistence worker backed by a **controlled Store**, checks metadata and token separation, constructs a fresh view from the loaded selection, invokes `start_restore` and verifies the returned identity via its `AuthApi::current_user` (`/me`) fixture, then logs out and checks deletion/revocation. Separately, a real-route integration test uses `AppSession` (not the Kit controls), saves through a controlled worker, verifies the token via HTTP `/me`, restores through `finish_restore`, then logs out, deletes and revokes. No single test crosses both the GPUI controls and the actual rewrite-server `/me` route. Other tests exercise storage failures, deletion ordering, and stale restoration. These outcomes do not prove native Secret Service or IME behavior.

## Native desktop results (2026-09-28, isolated disposable environment)

With the user's consent, two compiled GPUI clients ran on the existing **niri Wayland / Xwayland `DISPLAY=:0`** desktop against the unchanged rewrite server bound to `127.0.0.1:8081`, a temporary SQLite database, and separate temporary `XDG_CONFIG_HOME` directories. `LIBGL_ALWAYS_SOFTWARE=1` was used. A disposable `/dev/uinput` virtual mouse/keyboard supplied actual compositor/Xwayland input; screenshots used `grim` on the application workspace. These are native GUI observations, **not physical human keyboard/IME or accessibility-device verification**. No production users, database, keyring, or traffic logs were used; the input scripts, screenshots, database and logs were kept outside the repository under `/tmp`. Screenshots are ephemeral, not part of the committed evidence.

- Alice and Bob each signed up from the native UI and immediately entered a session. Alice created `roomalpha`; Bob saw it on login. Bob created `roombravo` while Alice was unfocused; Alice's focus-return refresh discovered it without changing her selected channel. An incorrect password on Alice's subsequent login left the fields editable with an explicit error, and a correct login succeeded.
- Bob typed `hello from bob`, pressed **Shift+Enter**, typed `second line`, and saw both lines remain in the composer without publication; plain Enter published once. Alice saw Bob's author, timestamp, preserved line break and one message on focus return. Bob's different unsent `roomalpha`/`roombravo` drafts survived switches. A mouse drag selected both lines of the native rendered message; after selection settled, Ctrl+C yielded **exactly** `hello from bob\nsecond line` from `wl-paste -n --type UTF8_STRING`. An immediate Ctrl+C in the same input-event batch as mouse-up did not copy, so this result includes a short frame/selection-settle interval. Only the expected disposable text was compared; unrelated clipboard contents were not recorded.
- While Alice was unfocused, a separate disposable HTTP login as **Bob** seeded **65** uniquely identified variable-height messages into `roomalpha` via the unchanged server (not 65 GUI sends); that extra login was then revoked. Focus return reconciled the burst. Scrolling through variable-height rows exposed both endpoints (`hello from bob`, `catch up 000` through `catch up 064`), with **Jump to latest** enabled off-bottom. While Alice remained at the oldest visible rows, Bob published one more message; Alice's focus-return refresh retained the same oldest rows rather than jumping to the new post. After Alice logged out and logged back in, selecting the room first showed the newest page, then native upward scrolling fetched an older page and eventually showed the original first message and **Start of conversation**. The controlled tests, rather than screenshots alone, assert identity deduplication and precise anchor positions.
- With Alice's draft visible, stopping the isolated server caused both refresh paths to show connection/catch-up errors while previously loaded history and the draft remained. Restarting against the **same database** and refreshing restored a connected state with the draft intact. Alice's native logout cleared the conversation/draft immediately; the disposable server's session-row count fell from one to zero. Directly revoking only Bob's disposable server session and refreshing returned Bob to the login screen with an explicit rejected-session message; Alice's separate session remained active.
- An ephemeral **loopback-only, body/header-silent** proxy kept the clients' URL at `:8081` while forwarding to the same isolated server temporarily rebound to `:8083`. With a forwarded message POST's completed response deliberately dropped, Bob saw `Could not confirm publication`, retained his draft, and the safely fetched history displayed that post once. The server assigned message ID `502256666194354`; querying the disposable database by the fake text showed **one** row immediately and after four seconds without any resend. With a four-second delayed response on a *different* send, the originating channel displayed disabled **Sending…**, navigation to the other channel worked before completion, its existing draft remained usable, and returning to the origin after confirmation showed the published message once and cleared only that origin draft. A separate slow-read interaction accepted composer typing, but its screenshot did not capture the GET still in flight; do not cite that frame alone as proof of blocked-GET responsiveness. All proxy modes were reset to normal after these checks.
- The normal desktop bus had **no activatable `org.freedesktop.secrets` provider**. Both clients visibly reported **memory-only** after signup/login; Alice's logout showed an honest *unconfirmed deletion* warning, not a false persistence claim. For real provider verification, `kwallet` **6.30.0**'s `ksecretd` ran on a **separate private D-Bus session**, with an encrypted, password-protected disposable wallet and isolated `XDG_DATA_HOME`/`XDG_CONFIG_HOME` (not the user's existing wallet). A third disposable native client, Carol, used only that bus/profile: signup displayed **Login saved in Secret Service for this server and user**; restarting the same client/profile restored Carol and displayed **Login restored from Secret Service**; logout displayed **Saved login removed from Secret Service**. The provider's Hamlet item count went from one to zero, the disposable server's Carol session count from one to zero, and a second restart showed the login form, not a restored session. Its plaintext config contained public metadata but neither the fake password nor an `access_token` key. A separate disposable Secret Service probe was saved, read and cleared before the client test; no secret value or token was written to logs or this document.

**Still unverified natively:** An actual IME candidate composition/Enter check (IBus is installed but no candidate engine/session was configured), assistive-technology inspection of accessible names/tooltips and physical keyboard traversal, precise pixel anchoring while an older-page response is delayed, and a locked/slow *real* wallet's prompt/timeout and deletion-retry behavior. The controlled tests cover corresponding state transitions but do not establish those native cases. There was no packaging or non-Linux check. #43's native IME criterion and #46's remaining native criteria must stay open.

## Reproduce on a graphical Linux desktop (checklist; remaining cases above still need a human)

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
4. With an unlocked real Secret Service, save after login **and after signup**; wait for the explicit success status, exit and restart, and check `/me` restoration into the same user. Log out: local state clears immediately; confirm both server revocation and removal of the selected item using the provider's credential manager; restart and confirm no restoration. Lock/disconnect the store and repeat to check **memory-only** save failure, unconfirmed timeout/deletion warning and retry behavior. A store timeout cannot guarantee whether a queued save/delete later succeeded; a revocation timeout cannot guarantee token invalidation. Never treat a timeout as authoritative session invalidity.
5. On the physical desktop, select multiline message text with mouse and Ctrl+C into another application; check line breaks. With an IME candidate *not yet committed*, press Enter: it must commit without POST; a subsequent plain Enter must send once. Check Shift+Enter, trailing newline, focus via keyboard/Tab, accessible names and tooltips on icon-first actions, and that slow HTTP/store operations do not freeze interaction. Headless key/clipboard events and controlled blocked-store tests **do not prove** native IME, selection/copy, accessibility, responsiveness or real store integration.

### Reproducible slow HTTP / uncertain-send drill (remaining checks or repeat run)

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

### Slow/locked secure-store drill (locked-store variant not run)

In that **disposable desktop OS user** (not the normal keyring), open Seahorse (Passwords and Keys) and create/unlock a test Login keyring before launching the client. Sign up with a fake password, wait for the explicit saved-login status, exit/restart and verify the same identity. In Seahorse lock the Login keyring (right-click **Login → Lock**, if provided by the installed provider); with the store locked, perform another disposable login/signup and record the UI's save status and elapsed time, including whether a prompt, immediate failure or ~10-second unconfirmed timeout occurred. While the operation is pending, type in another control or switch channels to check responsiveness. Unlock the keyring using its own UI; log out, wait for the deletion result, inspect only this disposable user's Hamlet entry in the credential manager (do not reveal its value), then restart to check no restoration. If deletion fails, use **Retry saved-login deletion** after unlocking and check again. If your provider has no lock UI or handles lock by prompting immediately, record that limitation instead of claiming a slow native-store test; the deterministic worker-blocking tests (`save_completing_after_invalidation_is_cleaned_before_deletion_reply`, `delayed_failed_deletions_can_retry_without_touching_new_user`) cover the controlled-delay behavior only. Record provider name/version, desktop/session type, start/end timestamps, exact visible statuses, whether the UI accepted interaction while pending, presence/absence of the selected entry, and restart/revocation outcome. Never paste tokens or passwords into notes.

**Remaining manual acceptance:** The native IME-candidate check, assistive-technology/physical keyboard inspection, precise native older-page anchor comparison with a delayed response, and real locked/slow wallet behavior remain open as described above. Native uinput-driven Xwayland checks are not a substitute for those human/device-specific checks. No packaging or non-Linux verification was performed.

## Presentation and future seams

Edit pane layout and semantic control wiring in `src/main.rs` (`Hamlet::render`, `conversation_panes`); replace color constants and bundled channel icon in its `theme` module (the send icon mapping is currently inline in `conversation_panes`). See `PRESENTATION.md` for control IDs, selectable history and focus notes. Behavioral tests use `AppSession`, `Conversation`, `Polling`, `AuthApi` and persistence outcomes. Headless tests address controls by semantic IDs and discover current bounds for scroll/selection; they do **not** assert fixed pixels or a presentation-specific tree path. Preserve meaningful action names/labels/tooltips and semantic IDs when restyling.

`src/polling.rs` owns the focus/interval/backoff schedule; `Conversation::refresh_history` and `complete_history` own gap-free selected-channel reconciliation and identity merging, while `src/http.rs` owns transport and `src/main.rs` dispatches async effects and preserves the GPUI list anchor. A future push adapter could trigger the existing selected-channel reconciliation without rewriting message rows, **but** the view currently directly owns poll timers/focus and read task cancellation: push subscriptions and reconnect lifecycle need explicit integration there. Unread indicators would need per-channel unread state and read-position policy (currently only selected history is fetched); typing would need ephemeral per-channel presence, transport and view state not represented by `Conversation` or server routes. Reconnect currently means retrying backed-off HTTP **reads**, not socket continuity or queued writes. No speculative push/unread/typing framework was added.

Prototype limitations: unchanged rewrite server; uncertain sends cannot guarantee exactly-once delivery without server idempotency, so there is no automatic write replay. No packaging, cross-platform verification, persistent history/drafts, queued/offline writes, rich message formatting or finished visual identity. Open signup and broad server access are explicitly **not** production readiness.

---

## Post-migration automated verification — #61 (2026-09-30)

**This appended record verifies the migrated client automatically; everything above
is preserved historical #46 evidence and reproduction guidance, not a fresh native
run.** Pinned #61 start: `093c9ac76e3889d0c87fc5476854d8227741d303`.
Ownership/contraction and scenario equivalents: [MIGRATION-61.md](MIGRATION-61.md).
Current editing locations/commands: [ARCHITECTURE.md](ARCHITECTURE.md),
[PRESENTATION.md](PRESENTATION.md), [README.md](README.md).

Fresh checks from `client-gpui/`, Linux x86_64, rustc/Cargo 1.95.0, existing
lockfile/cache (not a cold rebuild):

| Command | UTC start → end | Result |
| --- | --- | --- |
| `cargo fmt --check` | 18:24:30 → 18:24:30 | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | 18:24:30 → 18:24:30 | passed |
| `cargo test --locked` | 18:24:30 → 18:24:39 | **158 passed, 0 failed, 0 ignored** |
| `cargo build --locked` | 18:24:39 → 18:24:42 | passed; compiled, not launched |

Targeted view, session, conversation, state, authentication and real-route checks
also passed; production history, polling views and feature real-route suites each
passed at `SEED=1,2,3`. All 160 starting scenarios are accounted for: only the two
redundant #36 `HistoryProbe` cases were retired in favor of their stronger production
wheel/anchor and exact multiline selection/copy scenarios; the Bob timer journey
was renamed without losing coverage. The immutable #47 baseline and all prior
migration records remain unchanged. Complete test names, logs, preservation hashes
and ownership audit are in `/tmp/hamlet-orchestrator/review-61/`.

Tests exercise production coordinator execution paths with controlled execution/time,
real Kit controls/stable IDs, bound controlled/loopback HTTP and isolated files/fake
providers. The saved-login real-route scenario now drives session startup/submission,
automatic save/verification, logout/deletion/revocation rather than manually dispatching
those workflows. API tests have no feature dependencies; startup/shell contain no
feature request workflows or child-private assertions. No server/dependency change,
transport/deadline relaxation, persisted-format migration or diagnostic logging.
Metadata remains token/password-free; write cancellation/timeouts remain uncertain,
not evidence of non-delivery or a reason for automatic replay.

**Native handoff — pending #62, not passed here:** rerun the isolated two-client
smoke above against the unchanged server with disposable credentials/profiles and
explicit consent. Historical uinput/Xwayland/private-wallet results do not prove
the migrated entity/subscription wiring. IME candidate Enter, assistive technology
and physical keyboard, precise pixel anchoring with delayed older pages, and a
locked/slow real wallet remain outstanding. No desktop automation, real keyring
access, native launch, packaging or non-Linux verification occurred in #61.

### #61 Standards P2 correction verification — 2026-09-30

The two conversation real-route integrations now exercise `ConversationHandle`
with injected execution/time and opaque delivery, replacing independent
conversation/polling state and manually dispatched catch-up. Production timer
polling and continuation mutations made the corrected tests fail; the old tests
had passed with those paths disabled. Production code was restored byte-identically.
Timestamp ties, identity/order, multi-page continuity, older-page exhaustion and
two-user message/channel discovery remain covered against actual rewrite routes.

Fresh checks from `client-gpui/`, same toolchain and existing lockfile/cache:

| Command | UTC start → end | Result |
| --- | --- | --- |
| `cargo fmt --check` | 18:45:10 → 18:45:10 | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | 18:45:10 → 18:45:11 | passed |
| `cargo test --locked` | 18:45:11 → 18:45:22 | **158 passed, 0 failed, 0 ignored** |
| `cargo build --locked` | 18:45:22 → 18:45:24 | passed; compiled, not launched |

Three all-target typechecks, conversation tests (40), feature route tests (3),
and both corrected route tests at `SEED=1,2,3` also passed. Count remains 158;
no scenario was deleted or renamed by this correction. Fresh logs and sensitivity
evidence are in `/tmp/hamlet-orchestrator/review-61/correction/`. All earlier records
above are unchanged. Parent re-review remains pending; native #62 and all its
previously recorded limitations remain unperformed, not passed.

---

## Post-migration native attempt — #62 (2026-09-30, incomplete)

**The two-client smoke is blocked, not passed.** This is fresh post-migration
preflight/launch evidence, not a substitute for interactive native verification.
All preceding records are preserved historical evidence. Start pinned to
`f0aa63e608f76e7d11664d962fa455228be7e38f` on `rewrite`. Live
[issue #62](https://github.com/renodubois/hamlet/issues/62) was open,
labelled `ready-for-agent`, with no comments. The issue was not mutated; disposition
and fresh independent Standards/Spec reviews remain with the parent.

### Consent, isolation and stopping condition

The user explicitly authorized desktop automation and a real provider **only for
disposable credentials/data, separate client profiles and isolated provider
resources**, not existing credentials or the user's wallet. No dependency install,
upgrade, host configuration change or desktop unlock was authorized/performed.

A mode-0700 `mkdtemp` tree under `/tmp/hamlet62-*` contained the disposable database
and separate client/provider HOME, XDG_CONFIG_HOME, XDG_DATA_HOME, XDG_CACHE_HOME
and XDG_RUNTIME_DIR directories. A new `dbus-daemon` used a custom configuration
with a private Unix listener and **no service activation directories**. Both the
client and manually launched `ksecretd` received only that private bus address.
The ordinary desktop bus/store was not queried for credentials. Merely changing
XDG_CONFIG_HOME would **not** have isolated the provider.

The unchanged rewrite server was rebuilt with `cargo build --locked --bin hamlet`
from `server-rewrite/`, then launched with `RUST_LOG=off`,
`HAMLET_BIND=127.0.0.1:18081` and
`HAMLET_DATABASE_URL=sqlite://$RUN/test.db?mode=rwc`. No request/header/body logs
were collected. One freshly built client used `DISPLAY=:0`,
`LIBGL_ALWAYS_SOFTWARE=1`, the private bus/profile and no WAYLAND_DISPLAY.
A disposable uinput device was created, but **no key, click, wheel or clipboard
input was emitted**. Only the client's PID-verified X11 window was captured using
`import -window <owned-XID> 01-login.png`; no whole-desktop screenshot was taken.

The client rendered the empty login form (still showing default `:8081`; no
submission was made). Native focus/move requests had no effect and niri reported
no focused window. Read-only preflight then established the blocker:

```text
dms ipc call lock isLocked
true
dms ipc call lock status
sessionLockLocked=true, sessionLockSecure=true, loginctlLocked=true
NIRI_SOCKET=<existing compositor socket> niri msg -j focused-window
null
```

The desktop was **securely locked**. Operations stopped rather than unlocking,
resetting the locker, injecting credentials or bypassing the lock. The second
client was not launched. A read-only query on the **private** bus confirmed
`org.freedesktop.secrets` registration; that does not establish a usable/unlocked
wallet. No wallet was created/opened, and no provider save/read/delete occurred.
The disposable database had **0 users and 0 sessions**; the provider profile had
no files and the client had no `session.json`. This is not a client regression
reproducer; no interactive behavior could be evaluated.

### Fresh native scenario matrix

| Scenario | #62 outcome / affected prerequisite |
| --- | --- |
| Isolated binary launch and login-form rendering | Observed in the real Xwayland session, behind the locked desktop; not an interactive pass |
| Signup, login, invalid credentials/editable retry | **Blocked** by locked desktop; no credentials submitted |
| Two-client channel create/discovery, retained selection | **Blocked**; no authenticated clients or channel creation |
| Multiline send, Enter/Shift+Enter, per-channel drafts | **Blocked**; no input or publication |
| Focus polling and multi-page burst catch-up | **Blocked**; no focus acquisition, second client or burst seed |
| Older-page scrolling, reader retention, jump to latest | **Blocked**; no history or scroll input |
| Native selection/copy and exact multiline clipboard | **Blocked**; clipboard was neither read nor written |
| Outage/recovery with loaded history/draft | **Blocked**; server cleanup alone is not an outage/recovery test |
| Pending/uncertain send across navigation | **Blocked**; no POST, response-delay/drop proxy or navigation |
| Logout, revocation and rejected-session cleanup | **Blocked**; zero sessions, not a logout/rejection test |
| Secure save after signup/login, restart, verified restore, delete, no-restore restart | **Blocked**; private provider registered, but no wallet operations or authenticated client restart |
| Native IME candidate/Enter | **Unverified**; no IME composition exercised |
| Accessibility and physical keyboard inspection | **Unverified**; no assistive technology or human keyboard exercise |
| Precise delayed-prepend pixel anchoring | **Unverified**; no delayed older-page response or pixel comparison |
| Locked/slow real-wallet prompt, timeout, responsiveness, deletion retry | **Unverified**; a locked **desktop** is not a locked-wallet test |

No historical #46 observation or headless test is counted as native #62 coverage.
Packaging and non-Linux behavior also remain unverified.

### Environment, fresh gates and evidence

Linux x86_64 `7.2.7-arch1-1`; rustc `1.95.0 (59807616e 2026-04-14)`;
Cargo `1.95.0 (f2d3ce0bd 2026-03-21)`; niri `26.04 (8ed0da4)`;
Xwayland `24.1.13-1`; kwallet/ksecretd `6.30.0-1`. Libraries from pkg-config:
dbus-1 1.16.2, fontconfig 2.18.3, freetype2 26.6.20, xkbcommon 1.13.2,
wayland-client 1.26.0, X11 1.8.13, Vulkan 1.4.357. DISPLAY/WAYLAND_DISPLAY
were unset in the command harness; the isolated native process explicitly used
the existing Xwayland display. Existing toolchain, lockfile and cache were reused.

Fresh commands from `client-gpui/` on 2026-09-30 (UTC):

| Command | Start → end | Result |
| --- | --- | --- |
| `cargo fmt --check` | 19:11:48 → 19:11:48 | passed |
| `cargo clippy --locked --all-targets -- -D warnings` | 19:11:48 → 19:11:48 | passed |
| `cargo test --locked` | 19:11:48 → 19:11:57 | **158 passed, 0 failed, 0 ignored** |
| `cargo build --locked` | 19:11:57 → 19:11:57 | passed |

A final run after the guidance edits also passed all four commands: fmt
19:19:47 → 19:19:47, strict locked all-target Clippy 19:19:47 → 19:19:48,
locked tests 19:19:48 → 19:19:56 (**158 passed, 0 failed, 0 ignored**), and
locked build 19:19:56 → 19:19:57. Its timestamped logs are in `final/` below.

These are automated results only. Native attempt, blocker diagnosis and cleanup
occurred between the two gate runs (native command record 19:14:08–19:16:29 UTC).
Local ephemeral evidence is under
`/tmp/hamlet-orchestrator/review-62/`: `start.txt`, live `issue.json`,
`environment.txt`, timestamped gate logs, `server-build.log`, `native-driver.py`,
`native-command.py`, `native-commands.txt`, sanitized `native-outcomes.txt`,
`desktop-blocker.txt`, `01-login.png`, `cleanup.txt` and preservation hashes.
The helper scripts record the actual attempt, **not an unattended smoke runner**;
use the safety gate below before any repeat. Passwords were generated only in
process memory, never submitted or logged; no bearer tokens were created.
Unrelated window metadata was omitted from the retained outcome log. This
committed record is the durable summary; `/tmp` artifacts are not permanent.

Cleanup stopped/reaped only the owned client, provider, server and private bus,
destroyed/closed the virtual input device, and removed the entire generated
private tree (including database/profiles), driver socket and driver process.
A final process check found none of the four children. No real wallet, config or
existing process was modified. `.gitignore`, `CONTEXT.md` and both separately
owned plans remained byte-identical and uncommitted. Server source/protocol,
client source, dependencies and persisted formats were unchanged.

### Safety gate for the next attempt

1. Obtain/confirm explicit consent and an **already unlocked**, dedicated test
   workspace/session before launching or injecting input. On this DMS/niri setup,
   run the read-only lock queries above **first**. Stop on a locked/unknown state;
   a compositor socket and a rendered X11 window do not prove interactive access.
   Do not use the unlock/reset IPC, existing passwords or an Xvfb/headless result
   to bypass or satisfy this requirement.
2. Establish a new private provider bus with no host activation directories,
   isolated provider HOME/config/data/cache/runtime, and separate A/B client
   profiles; or use a disposable OS user with its own provider. Never aim these
   drills at the ordinary wallet. If isolation cannot be demonstrated, stop.
3. Start the unchanged server on a free loopback port with a fresh temporary
   database. Explicitly set **both** clients to that URL before signup (the
   attempted server used `:18081`, not the form's default `:8081`). Use disposable
   identities only. Retain no passwords/tokens in screenshots, scripts or logs.
4. Run every scenario in the historical two-client checklist and delay/drop
   proxy drill above, adapting its ports to the isolated server. Do not count
   this launch, automated tests or historical passes as completed scenarios.
   Exercise save after both signup and login, then restart/restore/delete and
   verify no restoration; record provider item presence/counts without values.
5. Record remaining native/device-specific cases separately, sanitize evidence,
   stop only owned processes and remove their private resources. Keep #62 and
   native completion **open/incomplete** until disposition by the parent; this
   blocker record does not satisfy the full-smoke criterion.
