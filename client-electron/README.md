# Hamlet Electron Alpha

Electron/Solid desktop client for Hamlet. It keeps the renderer on the fixed
loopback origin `http://127.0.0.1:1422` so auth, cookies, SSE, and LiveKit behave
like a normal browser-localhost app.

The desktop client is intentionally explicit: **Electron does not bundle, start,
stop, or supervise the Rust Hamlet server**. Start the server yourself before
development, manual QA, or most E2E runs.

## Prerequisites and server lifecycle

Install client dependencies once:

```bash
cd client-electron
npm install
```

Run the backend in a separate terminal before using the app:

```bash
cd server
cargo run
```

For voice QA that needs the self-hosted LiveKit dev service, use the server-side
Compose stack instead:

```bash
cd server
docker compose up
```

The Electron client talks to the configured Hamlet server URL directly from the
renderer over HTTP, credentialed fetch, SSE, WebSocket, and LiveKit/WebRTC. The
shell does not proxy application APIs through Electron IPC.

## Commands

Run commands from `client-electron/` unless noted.

| Goal                             | Command                                   | Notes                                                                                                                                                             |
| -------------------------------- | ----------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Renderer-only browser dev        | `npm run dev`                             | Starts Vite on `http://127.0.0.1:1422` with `--strictPort`. Open that URL in a normal browser.                                                                    |
| Renderer-only built preview      | `npm run build:renderer && npm run serve` | Serves built renderer assets on the same fixed origin for browser-only checks.                                                                                    |
| Electron dev launch              | `npm run electron:dev`                    | Builds main/preload, starts Vite, waits for `127.0.0.1:1422`, then launches Electron with `HAMLET_RENDERER_URL` pointing at Vite. Server must already be running. |
| Build everything                 | `npm run build`                           | Builds renderer output in `dist/` and Electron main/preload output in `dist-electron/`.                                                                           |
| Electron-only build              | `npm run electron:build`                  | Builds only main/preload. Useful before launching multiple dev profiles against an already-running Vite server.                                                   |
| Local unpacked package           | `npm run package:unpacked`                | Runs `npm run build`, then writes `release/Hamlet Electron Alpha-<platform>-<arch>/`.                                                                             |
| Launch unpacked package          | `npm run package:launch`                  | Rebuilds/repackages, clears `HAMLET_RENDERER_URL`, and launches the unpacked app against the packaged fixed-loopback static renderer.                             |
| Package smoke                    | `npm run package:smoke`                   | Rebuilds/repackages, then Playwright launches the unpacked package and verifies the fixed renderer origin.                                                        |
| Full public package              | `npm run package:full`                    | Intentionally fails with a deferral message. Signing, notarization, installers, auto-update, and public distribution are not configured for the alpha.            |
| Format                           | `npm run fmt` / `npm run fmt:check`       | Formatter check/fix for the Electron client tree.                                                                                                                 |
| Lint                             | `npm run lint`                            | Oxlint.                                                                                                                                                           |
| Typecheck                        | `npm run typecheck`                       | Renderer TypeScript plus `tsconfig.electron.json` for main/preload.                                                                                               |
| Unit/component/integration tests | `npm run test`                            | Vitest, MSW, fake SSE, axe/component coverage.                                                                                                                    |
| Browser E2E                      | `npm run test:e2e:renderer`               | Playwright Chromium against renderer-only Vite. The config starts `server` with `cargo run`.                                                                      |
| Electron E2E                     | `npm run test:e2e:electron`               | Builds, starts `server` with `cargo run`, then launches Electron through Playwright.                                                                              |
| All E2E                          | `npm run test:e2e`                        | Runs browser renderer E2E, then Electron shell E2E.                                                                                                               |
| Size budget                      | `npm run size`                            | Builds, then checks gzip JS/CSS renderer budgets in `.size-limit.json`. Electron package/artifact size is not budgeted yet.                                       |

From the repository root, repo-level checks can target this client with:

```bash
scripts/check.sh client
scripts/check.sh client --e2e
```

## Fixed origins, server URLs, and port behavior

There are two different URLs to keep straight:

- **Renderer origin:** always `http://127.0.0.1:1422` by default in dev and
  packaged modes. Chromium localStorage for the app is keyed to this origin.
- **Hamlet server URL:** entered on the login screen and stored as
  `hamlet.serverUrl` in renderer localStorage. The default is
  `http://127.0.0.1:3030`.

Keep the server URL spelling stable while testing. The `localhost` and
`127.0.0.1` spellings are different cookie hosts, so switching between them can
look like a lost session even though the renderer origin did not change.

`HAMLET_RENDERER_URL` is a developer/test override for the renderer URL. The
security policy still requires the origin to be `http://127.0.0.1:1422`; random
ports, `file://`, custom protocols, or remote renderer origins are rejected. When
`HAMLET_RENDERER_URL` is unset or empty, Electron starts the packaged static
renderer server from `dist/`.

Port `127.0.0.1:1422` is fixed on purpose:

- `npm run dev` and `npm run serve` use Vite `--strictPort`; they fail fast if
  another process already owns the port.
- Packaged/static-renderer Electron also binds `127.0.0.1:1422`. If the port is
  occupied, startup fails with a clear "already in use" message/dialog. Close the
  other Hamlet Electron instance or free the port, then relaunch.
- Normal packaged launches are single-instance. A second packaged launch should
  focus the existing window instead of racing for the fixed port.

## Data directories and profile isolation

Electron profile data contains Chromium cookies, localStorage, permissions, and
other browser state. Use profile isolation for multi-user local testing.

- Development and tests honor `HAMLET_DATA_DIR` when launching Electron directly.
  The directory is created if needed and becomes Electron's `userData` path.
- Packaged production ignores `HAMLET_DATA_DIR` so the alpha has one stable
  profile and one single-instance lock.
- Default packaged profile locations follow Electron's app-data conventions:
  - macOS: `~/Library/Application Support/Hamlet Electron Alpha`
  - Windows: `%APPDATA%\Hamlet Electron Alpha`
  - Linux: `~/.config/Hamlet Electron Alpha`

Two-client local development recipe:

```bash
# Terminal 1: backend
cd server && cargo run

# Terminal 2: one shared renderer dev server
cd client-electron && npm run dev

# Terminal 3: first Electron profile
cd client-electron
npm run electron:build
HAMLET_DATA_DIR=/tmp/hamlet-electron-baipas \
HAMLET_RENDERER_URL=http://127.0.0.1:1422 \
npx electron .

# Terminal 4: second Electron profile
cd client-electron
HAMLET_DATA_DIR=/tmp/hamlet-electron-teo \
HAMLET_RENDERER_URL=http://127.0.0.1:1422 \
npx electron .
```

On PowerShell, set environment variables before `npx electron .`:

```powershell
$env:HAMLET_DATA_DIR="$env:TEMP\hamlet-electron-baipas"
$env:HAMLET_RENDERER_URL="http://127.0.0.1:1422"
npx electron .
```

## Alpha package identity

The unpacked package deliberately uses a distinct side-by-side identity:

- Product name: `Hamlet Electron Alpha`
- Executable name: `hamlet-electron-alpha` on Windows/Linux; macOS uses
  `Hamlet Electron Alpha.app`
- macOS application identifier: `com.renodubois.hamlet.electron.alpha`
- Output directory: `release/Hamlet Electron Alpha-<platform>-<arch>/`

Icons live under `packaging/icons/`. The package metadata includes macOS
microphone and camera usage strings for Chromium media permission prompts; the
runtime permission policy allows microphone/audio for trusted Hamlet voice paths
and denies camera capture.

Optional cross-target package environment variables:

- `HAMLET_ELECTRON_PACKAGE_PLATFORM`: `darwin`, `linux`, or `win32`
- `HAMLET_ELECTRON_PACKAGE_ARCH`: `arm64`, `armv7l`, `ia32`, or `x64`

Only local unpacked dogfooding is in scope. Do not treat the output as a signed
or distributable release artifact.

## Known alpha gaps and expectations

Use this build for local dogfooding and parity evaluation, not public release.
Expected gaps:

- No bundled or supervised Rust server. Testers must start/stop the server and
  LiveKit stack themselves.
- No signing, macOS notarization, installers, auto-update, release channels, or
  public distribution. Unsigned-app warnings are expected.
- Normal app APIs stay in the browser renderer. The preload boundary is
  deliberately empty and raw `ipcRenderer` is not exposed.
- Packaged mode assumes one fixed renderer port and one normal packaged instance.
  Multiple packaged instances with separate profiles are not supported.
- Localhost-style Hamlet servers are the supported alpha path. Arbitrary remote
  or insecure HTTP servers depend on the existing server CORS/cookie behavior
  and are not a new Electron guarantee.
- Voice requires the server's LiveKit configuration to be available. Device and
  OS media-prompt behavior must still be checked manually on each platform.

Support/product notes:

- A failed app launch with `127.0.0.1:1422 is already in use` is usually a stale
  Vite server, a running packaged Electron instance, or another local process on
  the fixed renderer port.
- A login that works in one run but not another is often server lifecycle or host
  spelling (`localhost` vs `127.0.0.1`) rather than Electron storage loss.
- Server data may reset when the current development server restarts; do not
  interpret missing seeded messages/users as an Electron packaging bug without
  checking server state.

## Manual QA runbook

Before dogfooding an alpha package, run the automated checks that are practical
for the target machine, then complete the platform smoke areas below.

### Common setup

1. Start the Hamlet server (`cargo run`) or the server Compose stack when LiveKit
   voice is in scope.
2. From `client-electron/`, run either `npm run electron:dev` for shell dev QA or
   `npm run package:launch` for unpacked package QA.
3. Use seeded local users such as `baipas` / `password` and `teo` / `password`
   when the dev server seed data is available.
4. Pick one server URL spelling for the run, preferably `http://127.0.0.1:3030`
   when comparing two local clients, and keep it stable.

### Cross-platform smoke areas

- **Login, logout, and session persistence**
  - Log in, close/reopen Electron, and confirm the session remains signed in
    when the server session is still valid.
  - Log out from Settings and confirm reload/reopen returns to Sign in.
  - Stop the server and verify login shows a reachable error instead of a crash.
- **Server URL persistence**
  - Change the Server URL on the login screen, log in, relaunch, and verify the
    field is remembered.
  - Repeat with a clean `HAMLET_DATA_DIR` profile to confirm isolation.
- **Credentialed API calls**
  - Load channels after login, create a channel if available, update display
    name, upload/crop/delete an avatar, and verify all requests use the logged-in
    session.
  - After logout, confirm authenticated surfaces stop loading or return to login.
- **SSE delivery and two-client behavior**
  - Run two isolated Electron profiles, or one Electron profile plus a browser
    renderer session, against the same server.
  - Send messages from one client and confirm the other updates without reload.
  - Check channel creation/reorder, typing indicators, message edits/deletes, and
    voice participant/speaking updates where the server supports them.
- **Channel, message, avatar, and embed flows**
  - Open text and voice channels and verify the sidebar distinguishes them.
  - Send plain text, emoji, URLs, and messages that produce embeds/previews.
  - Reload a deep `/channel/<id>` route and confirm SPA fallback restores the
    same channel in dev and packaged modes.
  - Verify deterministic fallback avatars and uploaded avatars render beside
    messages and in profile/settings surfaces.
- **External links and unsafe navigation**
  - Click `http://` and `https://` message/embed links and verify they open in
    the OS default browser, not a new Electron app window.
  - Try `file:`, `javascript:`, or custom-scheme links from a controlled test
    message or devtools snippet and verify the app blocks navigation.
- **Voice settings and LiveKit**
  - Open Settings → Voice & Video and verify input/output device enumeration.
  - Change input/output devices, noise suppression, input gain, and speaking
    indicator preferences; relaunch and confirm preferences persist.
  - Join a LiveKit voice channel, grant microphone permission when prompted,
    mute/unmute, deafen/undeafen, leave, and switch channels.
  - With two clients, verify participant lists, join/leave, and speaking
    indicators update through LiveKit and SSE.
- **Unpacked package launch**
  - Run `npm run package:unpacked`, then launch the platform executable directly
    and with `npm run package:launch`.
  - Confirm the app URL is `http://127.0.0.1:1422`, deep-route reload works, and
    a second packaged launch focuses the existing window.
  - Confirm `npm run package:full` fails with the expected deferral message.

### macOS-specific checks

- Launch `release/Hamlet Electron Alpha-darwin-<arch>/Hamlet Electron Alpha.app`.
  Because the alpha is unsigned, Finder/Gatekeeper warnings are expected; use the
  normal internal-testing override path only if you trust the local build.
- Verify the app name, Dock/menu behavior, app icon, and second-launch focus.
- Join voice and verify the macOS microphone prompt names the alpha app in
  packaged mode. If permission was denied, recover through System Settings →
  Privacy & Security → Microphone.
- Confirm camera capture is not requested for normal voice flows. Record any
  camera prompt as an alpha bug/regression.
- Verify external links open the configured default browser.

### Windows-specific checks

- Launch `release\Hamlet Electron Alpha-win32-<arch>\hamlet-electron-alpha.exe`.
  SmartScreen/Defender warnings are expected for unsigned local builds.
- Verify taskbar identity, app icon, window focus on second launch, and clean exit
  from the window close button.
- Check microphone access under Windows Privacy & security → Microphone if device
  enumeration or join fails.
- Confirm external links open the default browser and no extra Electron windows
  remain after link clicks.
- Reopen the app and verify profile data under `%APPDATA%\Hamlet Electron Alpha`
  preserves server URL, session cookies, and voice preferences.

### Linux-specific checks

- Launch `./release/Hamlet\ Electron\ Alpha-linux-<arch>/hamlet-electron-alpha`.
- Verify app icon/window title under the active desktop environment and that a
  second launch focuses the existing window where the window manager allows it.
- Check microphone enumeration through the local PulseAudio/PipeWire setup and
  any xdg-desktop-portal prompts used by the distribution.
- Confirm external links route through `xdg-open` to the default browser.
- Reopen the app and verify profile data under `~/.config/Hamlet Electron Alpha`
  preserves server URL, session cookies, and voice preferences.

## Architecture and boundary notes

- `src/` is the Solid renderer. It owns auth, channels, messages, avatars,
  embeds, typing, SSE, localStorage preferences, and LiveKit/WebRTC calls.
- `electron/preload.ts` is intentionally empty. Do not expose raw IPC or broad
  system APIs to the renderer.
- `electron/security.ts` owns the trusted renderer origin, secure
  `BrowserWindow` options, top-level navigation policy, popup/external-link
  policy, and default-deny media/permission decisions.
- `electron/static-server.ts` owns packaged renderer serving: loopback-only bind,
  fixed origin, SPA fallback, path-traversal rejection, MIME types, security
  headers/CSP, and clear startup errors such as `EADDRINUSE`.
- `electron/lifecycle.ts` owns development data-directory overrides, packaged
  single-instance behavior, focus-on-second-launch behavior, and clean shutdown
  of windows/static-server resources.
- `scripts/package-*.mjs` own local unpacked package metadata and launch helpers;
  they do not configure public distribution.

Future follow-ups, outside this alpha slice:

- Define signing/notarization/installers/auto-update and package-size budgets
  when the project moves beyond local unpacked dogfooding.
