# GPUI client — Linux feasibility gate (#36), login/logout (#37), reading (#38), signup (#39), channel creation (#40)

## #36 Linux feasibility gate (recorded before #37)

At the time of this gate, this was still the **starter**, not the authenticated rewrite-server client. Its input and Submit button were functional; the headless tests also exercised the proposed conversation-history primitives. The starter has since been replaced with the #37 login/logout slice below.

### Linux setup

Verified on Linux x86_64, Arch kernel 7.2.7, Rust/Cargo 1.95.0, locked `gpui-kit` 0.6.1 (`gpui-pre` 0.3.4). Native build dependencies here include a C compiler, pkg-config, fontconfig 2.18.3, freetype 26.6.20, xkbcommon 1.13.2, Wayland client 1.26.0, X11 libraries and Vulkan loader/driver. This machine has Xvfb, ImageMagick `import`, and a working Vulkan implementation. Install equivalent development headers/libraries for your distribution; on a desktop, provide a functioning X11 or Wayland session and Vulkan driver. Other platforms have **not** been verified.

From `client-gpui/`:

```sh
cargo build --locked
cargo run --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked varied_height_history_preserves_reader_on_prepend
cargo test --locked selectable_history_copies_line_breaks
```

The #36-only `cargo test --locked submitting_the_real_input_updates_the_greeting` command covered the starter greeting test, which was replaced in #37 and is no longer runnable as a named test. The #36 headless tests used `gpui-kit`'s `test-support` feature in dev dependencies, ran without DISPLAY/Xvfb, located **real** Kit Input and Button controls by stable IDs, typed, clicked, and asserted the changed greeting. A real GPUI `ListState` with variable-height selectable text was scrolled with native wheel events. Its top visible item's viewport position remained unchanged when two older items were prepended. A drag-selection followed by Ctrl+C copied a message with its newline to the simulated clipboard. The `HistoryProbe` lives in `src/main.rs`'s test module; it is **not** yet the production conversation view. Measuring every row in the small probe makes its scrollbar accurate; a production history will need bounded measurement/height estimates for longer conversations.

### Actual #36 gate results

On this machine `cargo build --locked`, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings` and `cargo test --locked` succeeded (3/3 tests). With no desktop DISPLAY, `xvfb-run -a -s '-screen 0 1280x800x24' env LIBGL_ALWAYS_SOFTWARE=1 timeout 12s target/debug/client-gpui` kept the app running until timeout (exit 124, expected). A root-window capture at 5 seconds showed the 500×500 starter window, greeting, styled input, and button; no runtime errors were logged. The capture is not an assertion in the test suite.

**Not yet manually verified on a physical desktop:** actual mouse selection/clipboard, IME behavior, screen-reader behavior, and native variable-height scrolling. Headless GPUI events and Xvfb rendering do not prove these integrations. This gate established feasibility of the framework and headless seams, not the complete client or manual smoke test required at prototype exit (#46).

## #38 Read newest conversations

After login, the client loads protected text channels in server order, selects the first, and displays that channel's newest history page. Select another channel in the left pane to read it. Empty lists, empty histories, loading, and errors are shown explicitly. Message text is plain and line breaks are retained; author and timestamp are shown. Already loaded pages are kept only in memory until logout, expiry, server change, or a protected 401. Sending, older-page traversal, and refresh are not part of this slice. See `PRESENTATION.md` for presentation-editing locations and manual selection/copy status.

For this slice use the same build/fmt/clippy/test commands below. The HTTP tests cover protected channels and newest history against unchanged rewrite routes. Real-control headless tests cover selection and text display. Manual native Linux text selection/copy has **not** been verified; headless clipboard testing in the #36 feasibility probe is not a native desktop confirmation.

## #40 Create a text channel

Start the unchanged `server-rewrite` (see its README), then run `cargo run --locked` from `client-gpui/` and log in or sign up. In the left pane, enter a name under **Channel name** and select **Create text channel**. The server trims surrounding whitespace; names after trimming must be 1–64 bytes of ASCII letters, digits, spaces, hyphens or underscores. Names are unique ignoring ASCII letter case. Duplicate/invalid names display feedback without clearing the field; a pending write disables another submission and does not navigate. On confirmed creation the returned channel is inserted in server list order, selected, and its newest conversation loaded. An uncertain failure keeps the name and warns that the write may have succeeded: check the channel list before deliberately trying again (no automatic replay). Logout, expiry or server change prevents late responses from navigating. Requests use the existing eight-second HTTP timeout, no redirects, and memory-only session.

Run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, and `cargo build --locked` from `client-gpui/`. Headless tests exercise actual Kit creation controls and selection; targeted HTTP tests exercise the public adapter against unchanged rewrite routes (normalized name, duplicate, invalid name, bearer rejection, decoded response). **#38 physical desktop native selection/copy verification remains open**; headless clipboard coverage does not resolve it.

## #39 Signup and immediate session

Start `server-rewrite` following its README, then from `client-gpui/` run `cargo run --locked`. With the default loopback server URL (`http://127.0.0.1:8081`), select **New user? Sign up**, enter a username (3–32 ASCII letters, digits, `_` or `.`) and password (8–256 bytes), then select **Create user**. The returned session immediately opens the channel/conversation view; no second login is needed. Select **Log out** to leave it. To demonstrate rejection, try the same username with different letter case, or an invalid username/password; inputs remain editable. A network failure does not automatically replay signup: it may have succeeded, so check before resubmitting. Remote servers require HTTPS; redirects are never followed and the eight-second transport timeout applies. Credentials and sessions remain memory-only.

From `client-gpui/`, run `cargo build --locked`, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --locked`. Headless tests operate the real signup Kit controls; targeted HTTP tests call the unchanged rewrite-server routes and check the returned bearer token through `/api/v1/me`. Manual native desktop/assistive-technology behavior still requires verification.

## #37 Login/logout slice

This client starter now has real login/logout controls. Sessions are **memory-only**: closing the app requires logging in again. No password, token, or message data is persisted by the client. The authenticated shell shows the user and server; the #38 reading slice below adds channels and messages.

Start `server-rewrite` following its README, create a user through its `/api/v1/auth/signup` endpoint (or the #39 signup form above), then from `client-gpui/` run `cargo run --locked`. The login form defaults to `http://127.0.0.1:8081`; change this to the rewrite server's base URL if needed. Enter the existing user's username and password and select **Log in**. The button is disabled while the login request is pending. Select **Log out** to immediately leave the authenticated shell. A failed server revocation leaves you logged out locally and shows a warning; a 401 means the token was already invalid and does **not** confirm a fresh revocation. A known expiry also returns to login. Invalid credentials remain editable for retry.

Only HTTPS is permitted for remote servers; HTTP is limited to localhost or loopback IPs. Certificates are checked normally. Redirects are never followed, so passwords and tokens are not forwarded by a redirect. Login and logout have eight-second request timeouts and run off the UI thread. No automatic request retries occur. Do not use loopback HTTP over untrusted networks.

From `client-gpui/` run `cargo build --locked`, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --locked`. Tests cover controlled session outcomes/time, loopback TCP boundary fixtures (including redirects/timeouts), and a live HTTP contract test: `http::tests::rewrite_routes_signup_login_logout_and_errors` starts the **unchanged** `server-rewrite` `hamlet::routes` on a disposable port with a temporary SQLite database via `hamlet::connect_to_database`. It checks signup/bootstrap, decoded login and errors via public `AuthApi`, real malformed-JSON 400, bearer logout, protected `/me` 401 after logout, and distinct already-invalid 401. The AuthApi always emits valid JSON, so the malformed-JSON request uses raw HTTP separately. Headless GPUI tests operate real Kit inputs/buttons and verify validation feedback, the flag passed to Kit's disabled button, and inert repeated clicks (Kit's test snapshot does not expose the native disabled flag); the history-selection and variable-height scroll tests remain feasibility probes rather than a production conversation view. For this #37 slice on this machine, `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked` (14/14 tests, including the live route test), and `cargo build --locked` succeeded. Native screen-reader, IME, clipboard and desktop interaction need manual verification; headless tests do not establish these integrations.
