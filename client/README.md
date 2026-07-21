# Hamlet Electron Alpha

Electron/React desktop client for Hamlet. By default it keeps the renderer on
`http://127.0.0.1:1422`; worktrees can override the loopback renderer port with
`HAMLET_RENDERER_PORT` so multiple app instances can run side by side.

The desktop client is intentionally explicit: **Electron does not bundle, start,
stop, or supervise the Rust Hamlet server**. Start the server yourself before
development, manual QA, or most E2E runs.

## Prerequisites and server lifecycle

Use Node.js 24.13.0 or newer and pnpm 11.11.0 or newer. With Corepack:

```bash
corepack enable pnpm
cd client
pnpm install
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

Run commands from `client/` unless noted.

| Goal                             | Command                                     | Notes                                                                                                                                                                                                                      |
| -------------------------------- | ------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Renderer-only browser dev        | `pnpm run dev`                              | Starts Vite on `http://127.0.0.1:${HAMLET_RENDERER_PORT:-1422}` with `strictPort`. Open that URL in a normal browser.                                                                                                      |
| Renderer-only built preview      | `pnpm run build:renderer && pnpm run serve` | Serves built renderer assets on the same configured loopback origin for browser-only checks.                                                                                                                               |
| Storybook component workshop     | `pnpm run storybook`                        | Starts Storybook on port 6006 with React/Vite, Hamlet CSS, router decorator, and browser MSW support.                                                                                                                      |
| Storybook static build           | `pnpm run storybook:build`                  | Builds Storybook into `storybook-static/` for smoke/CI validation.                                                                                                                                                         |
| Electron dev launch              | `pnpm run electron:dev`                     | Builds main/preload, starts Vite, waits for the configured renderer URL, then launches Electron with `HAMLET_RENDERER_URL` pointing at Vite. Server must already be running.                                               |
| Build everything                 | `pnpm run build`                            | Builds renderer output in `dist/` and Electron main/preload output in `dist-electron/`.                                                                                                                                    |
| Static web build                 | `pnpm run build:web`                        | Runs a normal Vite renderer build, then writes GitHub Pages-friendly SPA fallback files (`dist/404.html` and `dist/.nojekyll`). Set `HAMLET_BUILD_SOURCE_MAPS=true` to emit deployable `.map` files.                       |
| Electron-only build              | `pnpm run electron:build`                   | Builds only main/preload. Useful before launching multiple dev profiles against an already-running Vite server.                                                                                                            |
| Local unpacked package           | `pnpm run package:unpacked`                 | Runs `pnpm run build`, then writes `release/Hamlet Electron Alpha-<platform>-<arch>/`.                                                                                                                                     |
| Launch unpacked package          | `pnpm run package:launch`                   | Rebuilds/repackages, clears `HAMLET_RENDERER_URL`, and launches the unpacked app against the packaged loopback static renderer.                                                                                            |
| Package smoke                    | `pnpm run package:smoke`                    | Rebuilds/repackages, then Playwright launches the unpacked package and verifies the configured renderer origin.                                                                                                            |
| Full public package              | `pnpm run package:full`                     | Intentionally fails with a deferral message. Signing, notarization, installers, auto-update, and public distribution are not configured for the alpha.                                                                     |
| Format                           | `pnpm run fmt` / `pnpm run fmt:check`       | Formatter check/fix for the Electron client tree.                                                                                                                                                                          |
| Lint                             | `pnpm run lint`                             | Oxlint.                                                                                                                                                                                                                    |
| Native React audit               | `pnpm run check:native-react`               | Rejects the deleted compatibility layer and identifiers across renderer, Electron, E2E, scripts, tests, and config; Solid dependencies/tooling/JSX configuration; and active docs that misidentify the renderer framework. |
| Typecheck                        | `pnpm run typecheck`                        | Renderer TypeScript plus `tsconfig.electron.json` for main/preload.                                                                                                                                                        |
| Unit/component/integration tests | `pnpm run test`                             | Vitest, MSW, fake SSE, axe/component coverage.                                                                                                                                                                             |
| Browser E2E                      | `pnpm run test:e2e:renderer`                | Playwright Chromium against renderer-only Vite. The config starts `server` with `cargo run`.                                                                                                                               |
| Browser voice E2E                | `pnpm run test:e2e:voice:browser`           | Playwright Chromium + Firefox against renderer-only Vite and the server-side Docker Compose LiveKit stack; includes fake-media camera start/stop and Chromium screen-share smoke paths with prerequisite skips.            |
| Electron E2E                     | `pnpm run test:e2e:electron`                | Builds, starts `server` with `cargo run`, then launches Electron through Playwright.                                                                                                                                       |
| All E2E                          | `pnpm run test:e2e`                         | Runs browser renderer E2E, then Electron shell E2E.                                                                                                                                                                        |
| Size budget                      | `pnpm run size`                             | Builds, then checks gzip JS/CSS renderer budgets in `.size-limit.json`. Electron package/artifact size is not budgeted yet.                                                                                                |

From the repository root, repo-level checks can target this client with:

```bash
scripts/check.sh client
scripts/check.sh client --e2e
```

## Voice media automated QA slice

Voice media regression coverage is split across fast fixtures and practical E2E smoke tests so CI does not depend on native OS picker automation:

- Vitest/MSW: `pnpm run test` covers active `/voice/screen-shares` and `/voice/cameras` fixture state, screen-share and camera SSE delivery, discovery, switching, and ended/unpublished cleanup.
- Browser voice E2E: `pnpm run test:e2e:voice:browser` starts the server-side Docker Compose LiveKit stack, joins voice, starts/stops camera with fake media in Chromium and Firefox, and in Chromium attempts screen-share start/stop plus a two-client discover/watch/stop-watching/live-stop smoke path. Camera and screen-share portions skip when LiveKit, fake media, or desktop-capture prerequisites are unavailable.
- Electron E2E: `pnpm run test:e2e:electron` launches Electron with `HAMLET_ELECTRON_UNDER_TEST=1` and `HAMLET_ELECTRON_TEST_DISPLAY_CAPTURE=hamlet-window`, exercising the trusted display-media path with a test-selected source instead of the native OS picker and verifying trusted fake-media camera capture stops tracks after use. Those environment variables are set only by the Playwright helper and are ignored outside the under-test flag.
- Server checks for this slice remain the normal Rust gates (`cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`) because screen-share/camera discovery and SSE state are driven by the LiveKit webhook handlers.

Manual platform QA and release/operator notes for screen sharing live in [`../docs/screen-sharing-support-manual-qa.md`](../docs/screen-sharing-support-manual-qa.md). Webcam video-call QA lives in [`../docs/webcam-video-calls-manual-qa.md`](../docs/webcam-video-calls-manual-qa.md).

Before running this slice from a worktree, source the worktree environment so the server, renderer, and LiveKit ports line up:

```bash
source ../.hamlet-worktree.env  # from client/
pnpm run fmt && pnpm run lint && pnpm run check:native-react && pnpm run typecheck && pnpm run test
pnpm run test:e2e:voice:browser
pnpm run test:e2e:electron
```

## Fixed origins, server URLs, and port behavior

There are two different URLs to keep straight:

- **Renderer origin:** `http://127.0.0.1:1422` by default in dev and packaged
  modes. Set `HAMLET_RENDERER_PORT` (and optionally `HAMLET_RENDERER_HOST`) for
  isolated worktrees. Chromium localStorage for the app is keyed to this origin.
- **Hamlet server URL:** entered on the login screen and stored as
  `hamlet.serverUrl` in renderer localStorage. The default is
  `http://127.0.0.1:3030`; set `VITE_HAMLET_DEFAULT_SERVER_URL` or
  `HAMLET_SERVER_URL` to change the default shown by a worktree. For static web
  builds, set `VITE_HAMLET_DEFAULT_SERVER_URL` before `pnpm run build:web` to
  bake in the hosted API URL shown on the login screen. Set
  `HAMLET_BUILD_SOURCE_MAPS=true` to publish Vite source maps alongside the
  static assets; the GitHub Pages deployment workflow enables this for deployed
  builds.

Keep the server URL spelling stable while testing. The `localhost` and
`127.0.0.1` spellings are different cookie hosts, so switching between them can
look like a lost session even though the renderer origin did not change. When
the local renderer is on the default `127.0.0.1` host, the client automatically
rewrites saved `http://localhost:...` server URLs to `http://127.0.0.1:...` so
SameSite=Lax cookies still work after the CSRF cookie changes.

`HAMLET_RENDERER_URL` is a developer/test override for the renderer URL. The
security policy still requires the origin to match the configured loopback
renderer origin; `file://`, custom protocols, remote renderer origins, and
unexpected ports are rejected. When `HAMLET_RENDERER_URL` is unset or empty,
Electron starts the packaged static renderer server from `dist/` on the
configured host/port.

Renderer-side Sentry telemetry is opt-in via `VITE_HAMLET_SENTRY_DSN`; unset or
blank values disable Sentry initialization. This is separate from the server-side
`HAMLET_SENTRY_DSN`, and the renderer keeps user info and HTTP body collection
disabled.

The renderer port is strict:

- `pnpm run dev` and `pnpm run serve` fail fast if another process already owns the
  configured port.
- Packaged/static-renderer Electron also binds the configured renderer port. If
  the port is occupied, startup fails with a clear "already in use"
  message/dialog. Close the other Hamlet Electron instance or free the port, then
  relaunch.
- Normal packaged launches are single-instance. A second packaged launch should
  focus the existing window instead of racing for the configured port.

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
cd client && pnpm run dev

# Terminal 3: first Electron profile
cd client
pnpm run electron:build
HAMLET_DATA_DIR=/tmp/hamlet-electron-baipas \
HAMLET_RENDERER_URL=http://127.0.0.1:1422 \
pnpm exec electron .

# Terminal 4: second Electron profile
cd client
HAMLET_DATA_DIR=/tmp/hamlet-electron-teo \
HAMLET_RENDERER_URL=http://127.0.0.1:1422 \
pnpm exec electron .
```

On PowerShell, set environment variables before `pnpm exec electron .`:

```powershell
$env:HAMLET_DATA_DIR="$env:TEMP\hamlet-electron-baipas"
$env:HAMLET_RENDERER_URL="http://127.0.0.1:1422"
pnpm exec electron .
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
runtime permission policy allows microphone/audio, camera-on capture, and
explicit camera previews for trusted Hamlet voice/video paths.

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
- Packaged mode assumes one configured renderer port and one normal packaged instance.
  Multiple packaged instances with separate profiles are not supported.
- Localhost-style Hamlet servers are the supported alpha path. Arbitrary remote
  or insecure HTTP servers depend on the existing server CORS/cookie behavior
  and are not a new Electron guarantee.
- Voice and video require the server's LiveKit configuration to be available.
  Device and OS media-prompt behavior must still be checked manually on each
  platform.

Support/product notes:

- A failed app launch with `127.0.0.1:<renderer-port> is already in use` is usually
  a stale Vite server, a running packaged Electron instance, or another local
  process on the configured renderer port.
- A login that works in one run but not another is often server lifecycle or host
  spelling (`localhost` vs `127.0.0.1`) rather than Electron storage loss.
- Server data is persistent by default. If channels, messages, sessions, or
  seeded users are missing, check the server `HAMLET_DATABASE_URL`/`HAMLET_DATA_DIR`,
  reset history, and `HAMLET_SEED_DEV_DATA` setting before treating it as an
  Electron packaging bug.

## Manual QA runbook

Before dogfooding an alpha package, run the automated checks that are practical
for the target machine, then complete the platform smoke areas below. For voice
media changes, also complete the dedicated screen-sharing and webcam checklists:
[`../docs/screen-sharing-support-manual-qa.md`](../docs/screen-sharing-support-manual-qa.md)
and [`../docs/webcam-video-calls-manual-qa.md`](../docs/webcam-video-calls-manual-qa.md).

### Common setup

1. Start the Hamlet server (`cargo run`) or the server Compose stack when LiveKit
   voice is in scope.
2. From `client/`, run either `pnpm run electron:dev` for shell dev QA or
   `pnpm run package:launch` for unpacked package QA.
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
  - Send text and photo messages from one client and confirm the other updates
    without reload, including the received photo thumbnail.
  - Check channel creation/reorder, typing indicators, message edits/deletes, and
    voice participant/speaking updates where the server supports them.
- **Channel, message, avatar, embed, and photo flows**
  - Open text and voice channels and verify the sidebar distinguishes them.
  - Send plain text, emoji, URLs, and messages that produce embeds/previews.
  - Upload JPEG, PNG, and static WebP photos in the channel composer; verify
    selected-photo previews clear after send and thumbnails render from the
    configured Hamlet server URL.
  - Upload a photo in a thread reply and verify the open thread panel renders the
    reply thumbnail. Renderer E2E covers this flow; rerun manually if thread UI
    changes or if packaged behavior is being certified.
  - Try a file larger than 10 MB and an unsupported type such as GIF/HEIC/TXT;
    verify the composer rejects it accessibly and no message is sent.
  - Delete a message or thread reply with photos and verify the thumbnail/full
    attachment disappear in the current client and in another SSE-connected
    client without reload.
  - Copy a thumbnail/full attachment URL: it should load only with an
    authenticated server session and return an auth error from a clean browser or
    after logout.
  - In both `pnpm run electron:dev` and `pnpm run package:launch`, repeat a photo
    send against the configured local server URL and confirm thumbnails/full
    links use that server origin, not the renderer origin.
  - Reload a deep `/channel/<id>` route and confirm SPA fallback restores the
    same channel in dev and packaged modes.
  - Verify deterministic fallback avatars and uploaded avatars render beside
    messages and in profile/settings surfaces.
- **External links and unsafe navigation**
  - Click `http://` and `https://` message/embed links and verify they open in
    the OS default browser, not a new Electron app window.
  - Try `file:`, `javascript:`, or custom-scheme links from a controlled test
    message or devtools snippet and verify the app blocks navigation.
- **Voice settings, camera, and LiveKit**
  - Open Settings → Voice & Video and verify input/output/camera device enumeration.
  - Change input/output/camera devices, noise suppression, input gain, and
    speaking indicator preferences; relaunch and confirm preferences persist.
  - Join a LiveKit voice channel, grant microphone permission when prompted,
    mute/unmute, deafen/undeafen, leave, and switch channels.
  - Turn camera on/off from the joined voice channel, grant camera permission
    when prompted, and confirm the local preview appears only while enabled.
  - With two clients, verify participant lists, join/leave, speaking indicators,
    and remote camera tiles update through LiveKit and SSE.
- **Unpacked package launch**
  - Run `pnpm run package:unpacked`, then launch the platform executable directly
    and with `pnpm run package:launch`.
  - Confirm the app URL uses the configured renderer origin, deep-route reload works, and
    a second packaged launch focuses the existing window.
  - Confirm `pnpm run package:full` fails with the expected deferral message.

### macOS-specific checks

- Launch `release/Hamlet Electron Alpha-darwin-<arch>/Hamlet Electron Alpha.app`.
  Because the alpha is unsigned, Finder/Gatekeeper warnings are expected; use the
  normal internal-testing override path only if you trust the local build.
- Verify the app name, Dock/menu behavior, app icon, and second-launch focus.
- Join voice and verify the macOS microphone prompt names the alpha app in
  packaged mode. If permission was denied, recover through System Settings →
  Privacy & Security → Microphone.
- Turn camera on from an active voice channel and verify the macOS camera prompt
  names the alpha app in packaged mode. If permission was denied, recover
  through System Settings → Privacy & Security → Camera, then relaunch if macOS
  requires it. In Voice & Video settings, verify Preview camera also uses the
  selected camera only after you request a preview.
- Verify external links open the configured default browser.

### Windows-specific checks

- Launch `release\Hamlet Electron Alpha-win32-<arch>\hamlet-electron-alpha.exe`.
  SmartScreen/Defender warnings are expected for unsigned local builds.
- Verify taskbar identity, app icon, window focus on second launch, and clean exit
  from the window close button.
- Check microphone and camera access under Windows Privacy & security →
  Microphone/Camera if device enumeration, voice join, or camera start fails.
- Confirm external links open the default browser and no extra Electron windows
  remain after link clicks.
- Reopen the app and verify profile data under `%APPDATA%\Hamlet Electron Alpha`
  preserves server URL, session cookies, and voice preferences.

### Linux-specific checks

- Launch `./release/Hamlet\ Electron\ Alpha-linux-<arch>/hamlet-electron-alpha`.
- Verify app icon/window title under the active desktop environment and that a
  second launch focuses the existing window where the window manager allows it.
- Check microphone and camera enumeration through the local PulseAudio/PipeWire,
  V4L2, and any xdg-desktop-portal prompts used by the distribution.
- Confirm external links route through `xdg-open` to the default browser.
- Reopen the app and verify profile data under `~/.config/Hamlet Electron Alpha`
  preserves server URL, session cookies, and voice preferences.

## Architecture and boundary notes

- `src/` is the React renderer. It owns auth, channels, messages, avatars,
  embeds, typing, SSE, localStorage preferences, and LiveKit/WebRTC calls.
- Context hooks expose direct values (`auth.user`, `channels.channels`,
  `voice.isMuted`) rather than callable getter/accessor wrappers. Providers memoize
  context objects and stable commands where identity affects subscribers; consumers
  should read values during render and use ordinary effects for synchronization.
- Canonical reducers own event reconciliation for state with overlapping HTTP,
  optimistic, and real-time updates: `messages/channel-message-reducer.ts` owns the
  channel timeline, `messages/thread-reducer.ts` owns thread state, and
  `voice/channel-presence.ts` owns voice-channel presence. Components dispatch
  scoped actions instead of duplicating those transition rules in local setters.
- `voice/voice-session.ts` is the imperative `VoiceSession` boundary for LiveKit
  room/track lifecycle, serialized media commands, and external-store snapshots.
  `VoiceChatProvider` adapts that session to React with `useSyncExternalStore`.
  `VoicePreferencesProvider` separately owns persisted device, gain, suppression,
  and speaking-display choices, and passes an immutable preference snapshot into
  `VoiceSession`; UI components do not own LiveKit rooms or persistence directly.
- `electron/preload.ts` is intentionally empty. Do not expose raw IPC or broad
  system APIs to the renderer.
- `electron/security.ts` owns the trusted renderer origin, secure
  `BrowserWindow` options, top-level navigation policy, popup/external-link
  policy, and default-deny media/permission decisions.
- `electron/static-server.ts` owns packaged renderer serving: loopback-only bind,
  configured origin, SPA fallback, path-traversal rejection, MIME types, security
  headers/CSP, and clear startup errors such as `EADDRINUSE`.
- `electron/lifecycle.ts` owns development data-directory overrides, packaged
  single-instance behavior, focus-on-second-launch behavior, and clean shutdown
  of windows/static-server resources.
- `scripts/package-*.mjs` own local unpacked package metadata and launch helpers;
  they do not configure public distribution.

## Theming

The renderer is styled entirely through the semantic CSS variables defined in
`src/index.css` (shadcn theme tokens in oklch: `--background`, `--foreground`,
`--primary`, `--muted`, `--destructive`, etc., plus a `--sidebar-*` group that
keeps the channel rail dark against the light chat area). Components never use
raw Tailwind palette classes (`gray-*`, `blue-*`, ...) — they reference tokens
(`bg-background`, `text-muted-foreground`, `bg-sidebar-accent`), so re-theming
the app means editing the variable values in `src/index.css` only. A `.dark`
block already carries dark-mode values; wiring a toggle just means setting the
`dark` class on `<html>`. Reusable primitives (Button, Input, Textarea, Label,
Badge, Separator, Card) are generated shadcn components in `src/components/ui/`
(base-ui + cva); add more with `pnpm exec shadcn add <component>`.

Deliberate exceptions to the token rule: status greens (live/connected
indicators, mic level meter), purple thread/animated-emoji affordances, and
black video wells with white overlay text in media tiles.

Future follow-ups, outside this alpha slice:

- Define signing/notarization/installers/auto-update and package-size budgets
  when the project moves beyond local unpacked dogfooding.
