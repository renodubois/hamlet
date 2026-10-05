# Client verification

## Automated checks

Run from `client/`:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

The suite exercises production session/conversation coordinators with controlled execution/time, real GPUI/Kit controls, loopback HTTP, isolated files/fake providers and actual server routes on temporary SQLite databases and ephemeral ports. It covers authentication, verified restoration, save/delete ordering, stale outcomes, history continuity and timestamp ties, viewport anchoring, selection/copy, focus-independent live delivery, actual loopback server restart, bounded overflow, independent initial reads, local read failures and uncertain sends without write replay. Reconnect retries after a fixed three seconds without channel/history reads or resetting local conversation state; readiness restores future delivery, not missed creations. See [best-effort live-update verification](../BEST-EFFORT-LIVE-UPDATES-VERIFICATION.md) for current acceptance mapping and check outcomes. The unchanged [#70 verification report](../LIVE-UPDATES-VERIFICATION.md) records historical baseline recovery and fanout measurements, not current recovery guarantees or release prerequisites. These checks do not launch a desktop client or access a real wallet.

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

1. **Authentication and discovery:** sign up Alice in A and Bob in B; both enter a session without a second login. Check logout/login and editable invalid-credential feedback. Create a channel in B; observe it automatically in A without losing A's valid selection. Check empty/loading states, author and timestamps.
2. **History and drafts:** send multiline text using Shift+Enter then Enter or the send button. Observe one message in A. Publish over 50 messages while A is unfocused; verify live delivery continues without refocus, refresh or duplicate rows. Scroll for older pages, retry after failure and jump to latest. Keep different drafts in two channels and verify switches preserve them. Compare the same visible message's viewport Y before/after delayed variable-height prepends.
3. **Outage and uncertainty:** with a draft visible and older pages loaded, stop the server. Check **Live updates disconnected — reconnecting.** and preservation of loaded active/inactive history, pages/cursors, drafts, selected channel and reading position; no Refresh or connection-retry controls exist. Restart with the same database and check fixed three-second retry, no automatic channel/history reads or state resets, and delivery of future creations after readiness. A creation made while a client is disconnected may remain absent after reconnect; check that a later ordinary channel/history load can retrieve it, without a special catch-up operation. Request counts and retry timing require the controlled automated scenarios, not visual inference. Delay/drop a POST response after acceptance; confirm uncertainty retains the originating draft/warning across navigation and reconnect, matching text does not confirm it, and only the originating pending composer locks. No writes are automatically resent. Check the conversation before any deliberate resend. Revoke/expire a session and verify synchronous local cleanup and rejection of late outcomes.
4. **Storage:** save after signup and after login, wait for explicit success, exit/restart and verify `/me` restoration to the same user. Log out and check server revocation plus removal of the disposable provider entry without reading its value. Restart and verify no restoration. Repeat with a locked/disconnected provider to check memory-only failures, unconfirmed timeouts/deletion and retry. A timeout cannot establish whether queued storage or revocation eventually succeeded.
5. **Native input/accessibility:** select multiline message text and copy into another application; preserve exact line breaks. With an uncommitted IME candidate, Enter must commit without sending; a subsequent plain Enter sends once. Check Shift+Enter, trailing newline, Tab/button focus, accessible names/tooltips and interaction responsiveness during slow HTTP/storage.

## Slow HTTP and uncertain-send drill

The former whole-body forwarding proxy is unsuitable for indefinite SSE and has
been removed from this checklist. Do not collect an event stream to EOF or shape
host-loopback traffic affecting unrelated processes. Automated controlled-transport
and real-route barrier tests cover delayed reads, both HTTP/event orders and
uncertain sends; those are not a native fault-injection drill.

A future consented native drill needs an isolated SSE-aware relay which preserves
incremental event delivery and targets only the selected ordinary HTTP response.
While reads are delayed, type and switch channels. After an accepted POST response
is lost, verify that the originating draft/warning survives even if the message
arrives by event, with no automatic resend or catch-up read. Record visible status
and server message identity, never traffic dumps. This native relay/drill remains
unimplemented and unverified; server stop/restart can be exercised separately via
the checklist above.

## Locked/slow secure-store drill

In the disposable OS user/provider, save a fake user's login and verify restoration after restart. Use the provider's own UI to lock its test wallet; perform another login/signup and record immediate failure, prompt or approximately ten-second unconfirmed timeout. Check responsiveness while pending. Unlock through the provider UI, log out, inspect only presence/absence of the disposable Hamlet entry, and use deletion retry if needed. Restart to confirm no restoration.

If the provider lacks a lock UI or immediately prompts instead of remaining slow, record that limitation rather than claiming the case passed. Capture provider/version, desktop/session type, timings, visible statuses, responsiveness, entry presence and restart/revocation outcome, without credential values.
