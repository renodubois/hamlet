# Screen Sharing Manual QA and Release Notes

This document is the human validation checklist for the Hamlet screen-sharing MVP. It is intentionally not a completion record: issue #43 should stay open until the macOS, Windows, and Linux rows below have actually been executed and signed off on the target machines.

## Scope

Validate that screen sharing remains tied to an active voice channel, uses explicit user consent, preserves camera denial, supports opt-in watching, and behaves correctly across browser renderer development, Electron development, and unpacked packaged Electron where the platform permits display capture.

Out of scope for this MVP: camera chat, screen-share audio, recording, persistent stream history, watch-only participation, multi-stream grid viewing, moderation controls, and production packaging/signing.

## Prerequisites

- Run the server with LiveKit available:

  ```bash
  cd server
  docker compose up
  ```

  In an isolated worktree, source `.hamlet-worktree.env` before starting the stack so the server, renderer, LiveKit, and UDP port ranges line up.
- Use two authenticated users, for example seeded `baipas` / `password` and `teo` / `password`.
- Use two clients for live-update checks. Preferred combinations:
  - browser renderer + browser renderer
  - Electron dev + browser renderer
  - unpacked packaged Electron + browser renderer
- Keep the Hamlet server URL spelling stable for a run, preferably `http://127.0.0.1:<server-port>`.
- Run the automated slice before manual QA when practical:

  ```bash
  scripts/check.sh server
  scripts/check.sh client
  cd client && npm run test:e2e:voice:browser
  cd client && xvfb-run -a npm run test:e2e:electron  # Linux/headless only
  ```

## Environment matrix

Record each target before release. Do not mark the tracker issue complete until every required platform/environment has been evaluated or explicitly waived.

| Platform | Browser renderer dev (`npm run dev`) | Electron dev (`npm run electron:dev`) | Unpacked packaged Electron (`npm run package:launch`) | Notes / owner / date |
| --- | --- | --- | --- | --- |
| macOS | [ ] | [ ] | [ ] | |
| Windows | [ ] | [ ] | [ ] | |
| Linux | [ ] | [ ] | [ ] | |

## Common manual scenarios

Run these against each checked environment above.

### Starting and stopping a local share

- [ ] Join the `voice` voice channel before sharing.
- [ ] Confirm the Share screen control is unavailable or absent before joining voice.
- [ ] Start sharing a full display.
- [ ] Start sharing a single window or application.
- [ ] Cancel the browser/OS/Electron picker and confirm no stream is published, voice stays connected, and microphone mute/deafen state is unchanged.
- [ ] Stop sharing from Hamlet and confirm the local sharing indicator clears immediately.
- [ ] Start again, then stop from the browser/OS capture control and confirm Hamlet clears the local sharing indicator.
- [ ] While already sharing, try to start another share and confirm Hamlet rejects or replaces it clearly without publishing two local screen tracks.
- [ ] Leave voice while sharing and confirm the share stops.
- [ ] Switch to another voice channel while sharing and confirm the old-room share stops and no stale stream remains.

### Watching and switching streams

- [ ] In a second client joined to the same voice channel, confirm active shares appear near voice presence without auto-opening video.
- [ ] Confirm a user not joined to that voice channel sees Join to watch, not an automatic join.
- [ ] Click Watch for one stream and confirm the viewer panel opens in the main content area with the sharer name.
- [ ] Confirm remote microphone audio still works while watching.
- [ ] Stop watching and confirm the viewer detaches/closes while voice remains connected.
- [ ] With two presenters sharing, switch from stream A to stream B and confirm stream A is detached/unsubscribed before stream B opens.
- [ ] Stop the watched stream from the presenter and confirm the viewer closes cleanly or shows a short ended state.
- [ ] Stop a non-watched stream and confirm the watched stream remains stable.

### Live updates and cleanup

- [ ] With two clients connected, start a share in one client and confirm the other updates without reload.
- [ ] Stop the share and confirm the other client removes the stream without reload.
- [ ] Disconnect or close the presenter client and confirm participant cleanup removes active streams.
- [ ] Reload a viewer during an active share and confirm current stream state bootstraps correctly.
- [ ] Send text messages while watching and confirm chat remains usable.

### Privacy, camera denial, and accessibility

- [ ] Confirm no camera permission prompt appears during screen-share flows.
- [ ] Confirm screen-share audio is not offered or published in the MVP.
- [ ] Confirm microphone permission prompts and mute/deafen controls continue to behave as they did before screen sharing.
- [ ] Check keyboard access for Share screen, Stop sharing screen, Watch, Switch/Watch another stream, and Stop watching.
- [ ] Confirm accessible names include the relevant action and sharer where applicable.
- [ ] Confirm visible indicators do not rely on text alone and focus rings remain visible.

## Platform-specific notes

### macOS

- Screen sharing may require System Settings → Privacy & Security → Screen & System Audio Recording (or Screen Recording on older macOS versions). Verify first-run denial and recovery after granting permission.
- If permission is changed while Hamlet is running, relaunch the browser/Electron app before retesting capture.
- Verify full-display and window/application choices through the native picker where available.
- For unpacked Electron, unsigned alpha builds may show Gatekeeper prompts; this is expected for internal QA and is separate from screen-capture behavior.
- Confirm no Camera permission entry is requested or added for Hamlet during these flows.

### Windows

- Verify monitor and window sharing from the Chromium/Electron picker.
- Confirm canceling the picker publishes nothing and leaves the voice connection intact.
- Check Windows Privacy & security → Camera and Microphone after the run: microphone may be used for voice, but camera must not be requested by screen sharing.
- On multi-monitor systems, verify the expected monitor can be selected and stopped.
- Record GPU/driver, Windows version, and whether the app was Electron dev or unpacked packaged Electron if capture fails.

### Linux

- Test at least one Wayland/portal environment where practical and record whether X11 or Wayland was used.
- Screen/window capture commonly depends on PipeWire plus `xdg-desktop-portal` and a desktop-specific portal backend. Missing or broken portals may prevent picker display.
- Verify full-display and window/application capture where the active compositor exposes both.
- Confirm portal cancel publishes no stream.
- If capture works in browser but not Electron, record desktop environment, compositor, portal backend, and Electron stderr.
- Confirm no camera prompt appears from the portal or browser permission UI.

## Release/operator notes

- Screen sharing uses the same LiveKit room and WebRTC hardening requirements as voice. It does not introduce a second media service.
- Production deployments need secure origins: HTTPS for the Hamlet app/API and WSS for LiveKit WebSocket access. Localhost development is the browser exception for display capture.
- ICE candidates must advertise addresses reachable by clients. Local Compose uses host networking for LiveKit to avoid loopback-only ICE on browsers such as Firefox.
- TURN is expected for clients behind restrictive NATs/firewalls; screen sharing is more sensitive to bad relay/ICE configuration than voice because video bitrate is higher.
- Open and monitor the configured LiveKit UDP media range. The local Compose defaults are controlled by `server/livekit.yaml`; isolated worktrees generate `server/livekit.local.yaml` with non-overlapping TCP/UDP ports.
- Screen-share video materially increases CPU, GPU, battery, and bandwidth usage. The MVP intentionally keeps watching opt-in, subscribes to one watched stream at a time, and detaches previous/hidden videos.
- Screen-share audio, camera chat, recording, multi-stream grid viewing, watch-only mode, moderation stop controls, and signed/notarized public packaging are follow-up work, not part of this release.
- The server keeps screen-share stream state ephemeral. No screen-share history should be persisted when Hamlet moves to persistent SQLite.

## Completion record

When manual QA is actually performed, add a dated note in the release/QA tracking system or follow-up PR with:

- platform and version
- environment tested (browser renderer, Electron dev, unpacked package)
- capture backend/picker observed
- pass/fail summary for the common scenarios
- known caveats or waivers
- tester name/initials
