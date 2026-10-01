# Client verification

## Automated checks

Run from `client/`:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

The suite exercises production session/conversation coordinators with controlled execution/time, real GPUI/Kit controls, loopback HTTP, isolated files/fake providers and actual server routes on temporary SQLite databases and ephemeral ports. It covers authentication, verified restoration, save/delete ordering, stale outcomes, history continuity and timestamp ties, viewport anchoring, selection/copy, focus polling, read recovery and uncertain sends without write replay. These checks do not launch a desktop client or access a real wallet.

See [ARCHITECTURE.md](ARCHITECTURE.md) for ownership/test rules and [PRESENTATION.md](PRESENTATION.md) for semantic controls and editing locations.

## Native verification status

The latest recorded desktop attempt (2026-09-30) rendered an isolated login form but stopped because the desktop was securely locked. No interactive two-client workflow or credential save/read/delete was exercised. Private Secret Service registration alone does not establish usable storage.

Native acceptance is incomplete. In particular, IME candidate/Enter behavior, assistive technology and physical keyboard traversal, precise delayed older-page pixel anchoring, and a locked/slow real wallet's timeout/deletion retry remain unverified. Automated checks, headless rendering and provider registration are not substitutes for these cases. Packaging and non-Linux behavior are also unverified.

## Native safety gate

1. Obtain explicit consent and an **already unlocked**, dedicated test workspace/session before launching automation or injecting input. Check lock state first; stop if locked or unknown. Do not unlock, reset or bypass the desktop lock.
2. Use a disposable OS user with its own provider, or a private provider D-Bus with no host activation directories and isolated provider HOME/config/data/cache/runtime. Give clients A/B separate profiles. Changing only `XDG_CONFIG_HOME` does **not** isolate a wallet. Never use the ordinary wallet for these drills; stop if isolation cannot be demonstrated.
3. Start the server on a free loopback port with a fresh temporary database. Explicitly point both clients at the same URL before signup. Use disposable identities/passwords/messages only; do not collect credentials, authorization headers or payloads in logs/screenshots.
4. Record interactive and device-specific outcomes separately. Stop only owned processes and remove their private resources afterward. Do not count a launch or automated test as a completed native scenario.

On DMS/niri, read-only preflight checks can include:

```sh
dms ipc call lock isLocked
dms ipc call lock status
niri msg -j focused-window
```

A compositor socket or rendered X11 window alone does not prove an unlocked interactive session.

## Two-client smoke checklist

Install the [Linux prerequisites](OVERVIEW.md#linux-prerequisites), provide an isolated unlocked Secret Service, clipboard and configured IME, and complete the safety gate above. From `server/`, start a disposable database with an explicit bind:

```sh
HAMLET_DATABASE_URL='sqlite:///absolute/disposable/test.db?mode=rwc' \
HAMLET_BIND=127.0.0.1:8081 RUST_LOG=off cargo run --locked --bin hamlet
```

From `client/`, run `cargo run --locked` in each isolated graphical profile. Use `http://127.0.0.1:8081` in both clients. One selected login is saved per profile; separate config directories alone do not provide separate credential stores.

1. **Authentication and discovery:** sign up Alice in A and Bob in B; both enter a session without a second login. Check logout/login and editable invalid-credential feedback. Create a channel in B; discover it in A through refresh/focused polling without losing A's valid selection. Check empty/loading states, author and timestamps.
2. **History and drafts:** send multiline text using Shift+Enter then Enter or the send button. Observe one message in A. Publish over 50 messages while A is unfocused, refocus and check multi-page catch-up/deduplication. Scroll for older pages, retry after failure and jump to latest. Keep different drafts in two channels and verify switches preserve them. Compare the same visible message's viewport Y before/after delayed variable-height prepends.
3. **Outage and uncertainty:** with a draft visible, stop the server and use both refresh controls. Loaded history/draft remain while read errors are shown. Restart with the same database and check recovery without write replay. Delay/drop a POST response after acceptance; confirm uncertainty retains the originating draft/warning across navigation and only the originating pending composer locks. Check history before any manual resend. Revoke/expire a session and verify synchronous local cleanup and rejection of late outcomes.
4. **Storage:** save after signup and after login, wait for explicit success, exit/restart and verify `/me` restoration to the same user. Log out and check server revocation plus removal of the disposable provider entry without reading its value. Restart and verify no restoration. Repeat with a locked/disconnected provider to check memory-only failures, unconfirmed timeouts/deletion and retry. A timeout cannot establish whether queued storage or revocation eventually succeeded.
5. **Native input/accessibility:** select multiline message text and copy into another application; preserve exact line breaks. With an uncommitted IME candidate, Enter must commit without sending; a subsequent plain Enter sends once. Check Shift+Enter, trailing newline, Tab/button focus, accessible names/tooltips and interaction responsiveness during slow HTTP/storage.

## Slow HTTP and uncertain-send drill

Use only the disposable environment above. Keep the server on `127.0.0.1:8081`; the forwarding proxy below listens on `127.0.0.1:8082` without traffic logging. `slow` delays responses four seconds; `drop_send` forwards a message POST and consumes its completed response before closing the client connection. Stop with Ctrl+C and switch the environment variable between runs.

```sh
HAMLET_PROXY_MODE=slow python3 - <<'PY'
import http.client, os, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

class Relay(BaseHTTPRequestHandler):
    def log_message(self, *args): pass
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
            payload = upstream.read()
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

Set both clients to `http://127.0.0.1:8082` before signup. In `slow` mode, refresh and type/switch channels while reads are pending. In `drop_send` mode, send a unique message once, wait for the warning, navigate away/back and refresh: it should appear once without another Send. Record visible warning and server message identity, never traffic dumps. Avoid host-loopback traffic shaping, which would affect unrelated processes.

## Locked/slow secure-store drill

In the disposable OS user/provider, save a fake user's login and verify restoration after restart. Use the provider's own UI to lock its test wallet; perform another login/signup and record immediate failure, prompt or approximately ten-second unconfirmed timeout. Check responsiveness while pending. Unlock through the provider UI, log out, inspect only presence/absence of the disposable Hamlet entry, and use deletion retry if needed. Restart to confirm no restoration.

If the provider lacks a lock UI or immediately prompts instead of remaining slow, record that limitation rather than claiming the case passed. Capture provider/version, desktop/session type, timings, visible statuses, responsiveness, entry presence and restart/revocation outcome, without credential values.
