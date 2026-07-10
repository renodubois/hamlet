# Webcam Video Calls Manual QA and Release Notes

This checklist validates Hamlet webcam video calls after the automated fake-media smoke tests pass. It is a runbook, not a completion record; record actual platform results in the release or tracker notes for the validation run.

## Scope

Validate explicit camera use in voice channels across browser renderer development, Electron development, and unpacked packaged Electron. Confirm camera permission denial/recovery, selected-camera behavior, leave/switch cleanup, cross-platform permission surfaces, and coexistence with screen sharing.

Out of scope: recording, virtual backgrounds, moderation camera controls, public signed installers, and production TURN/relay certification beyond the existing LiveKit voice requirements.

## Prerequisites

- Source the worktree environment before local runs when using an isolated worktree:

  ```bash
  source .hamlet-worktree.env
  ```

- Run the Hamlet server with LiveKit available:

  ```bash
  cd server
  docker compose up
  ```

- Use two authenticated users, for example seeded `baipas` / `password` and `teo` / `password`.
- Use at least one real camera when doing manual device-selection checks. A virtual camera is acceptable only when the run explicitly notes it.
- Keep the server URL spelling stable for a run, preferably `http://127.0.0.1:<server-port>`.
- Run practical automated checks first:

  ```bash
  scripts/check.sh client
  cd client && pnpm run test:e2e:voice:browser
  cd client && xvfb-run -a pnpm run test:e2e:electron  # Linux/headless only
  ```

## Environment matrix

Record each target before release. Mark unsupported/waived cells with the reason, owner, and date.

| Platform | Browser renderer dev (`pnpm run dev`) | Electron dev (`pnpm run electron:dev`) | Unpacked packaged Electron (`pnpm run package:launch`) | Notes / owner / date |
| --- | --- | --- | --- | --- |
| macOS | [ ] | [ ] | [ ] | |
| Windows | [ ] | [ ] | [ ] | |
| Linux | [ ] | [ ] | [ ] | |

## Common manual scenarios

Run these in each checked environment above unless the cell is explicitly waived.

### Camera start/stop in voice

- [ ] Join the seeded `voice` channel and confirm microphone join still works.
- [ ] Confirm the camera control is available only after joining voice.
- [ ] Click **Turn on camera** and grant camera permission when prompted.
- [ ] Confirm the local camera preview appears, the participant row shows a camera indicator, and the camera status reads `Camera on`.
- [ ] In a second client joined to the same voice channel, confirm the remote camera tile appears without a reload and has the sender's name.
- [ ] Click **Turn off camera** and confirm the local preview disappears, the remote tile disappears, and no stale camera indicator remains.
- [ ] Turn camera on/off twice in the same session to catch duplicate publication or stale-track bugs.

### Permission denial and recovery

- [ ] Deny camera permission from the browser/Electron/OS prompt and confirm Hamlet shows an accessible camera-denied error without disconnecting voice or changing mute/deafen state.
- [ ] While denied, retry **Turn on camera** and confirm no local preview or remote tile appears.
- [ ] Recover permission through the relevant browser site settings or OS privacy settings, relaunching the browser/Electron app if the platform requires it.
- [ ] Retry **Turn on camera** and confirm camera starts normally after recovery.
- [ ] Repeat the denial/recovery path in packaged Electron, because its app identity and OS privacy entry differ from Electron dev.

### Selected camera behavior

- [ ] Open Settings → Voice & Video and verify the Camera device list populates.
- [ ] Select a non-default camera or virtual camera, preview it, close Settings, then turn camera on in voice.
- [ ] Confirm the selected device is used for both preview and in-call camera.
- [ ] Relaunch and confirm the selected camera preference persists when the device is still present.
- [ ] Unplug or disable the selected camera and confirm Hamlet falls back or reports `No camera device was found` without publishing a broken track.

### Leave, switch, reload, and shutdown cleanup

- [ ] Leave voice while camera is on and confirm the local preview stops and the second client removes the remote tile.
- [ ] Switch from one voice channel to another while camera is on and confirm the old-room camera stops before joining the new room.
- [ ] Reload the renderer or close the Electron window while camera is on and confirm the second client receives cleanup without a stale camera tile.
- [ ] Stop/restart the server or LiveKit during an active camera session and confirm the UI clears connection/media state rather than showing a frozen live camera.

### Camera plus screen-share coexistence

- [ ] Start camera, then start screen share. Confirm both local indicators are visible and the second client can see the remote camera tile plus the active screen share entry.
- [ ] Stop only screen share and confirm camera stays on.
- [ ] Stop only camera and confirm screen sharing stays active.
- [ ] Start screen share first, then turn camera on, and repeat the one-at-a-time stop checks.
- [ ] Leave voice while both are active and confirm both camera and screen-share tracks stop and disappear remotely.
- [ ] Deny or cancel the screen-share picker while camera is on and confirm camera stays on.

### Accessibility and privacy

- [ ] Keyboard-focus and activate Turn on camera, Turn off camera, Preview camera, and device selects.
- [ ] Confirm camera errors are exposed as alerts/status text and focus remains usable after denial.
- [ ] Confirm browser/OS camera privacy indicators are active only while preview or in-call camera is running.
- [ ] Confirm screen-share flows do not request camera permission unless the tester separately starts camera.

## Platform-specific notes

### macOS

- Check System Settings → Privacy & Security → Camera and Microphone. Browser dev entries use the browser name; Electron dev/package should name `Electron` or `Hamlet Electron Alpha` depending on launch mode.
- After changing Camera permission, macOS may require relaunching the browser/Electron app before capture succeeds.
- In packaged mode, unsigned alpha Gatekeeper warnings are expected and separate from camera permission behavior.
- Verify camera and screen recording permissions independently when testing camera plus screen share.

### Windows

- Check Settings → Privacy & security → Camera and Microphone, including the desktop-app access toggles.
- SmartScreen/Defender warnings are expected for unsigned local packaged builds.
- Confirm denial/recovery works for the packaged executable identity under `%APPDATA%\Hamlet Electron Alpha`.
- On laptops with hardware privacy shutters or vendor camera toggles, record their state if capture fails.

### Linux

- Record distribution, desktop environment, X11/Wayland, PipeWire/PulseAudio, and portal backend when relevant.
- Camera capture usually comes from V4L2 devices; screen-share picker behavior may depend on `xdg-desktop-portal` and the compositor.
- Confirm browser dev and Electron package use the expected camera device and that denial/recovery behavior is documented for the active portal/browser.
- If camera works in browser but not Electron, record Electron stderr, device permissions, sandbox/container details, and portal versions.

## Completion record

When manual QA is performed, record:

- platform and version
- environment tested (browser renderer, Electron dev, unpacked package)
- camera device(s) used and selected-camera result
- permission denial/recovery result
- camera plus screen-share result
- pass/fail summary and caveats or waivers
- tester name/initials and date
