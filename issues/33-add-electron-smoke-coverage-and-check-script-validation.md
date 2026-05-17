---
id: 33
name: Add Electron smoke coverage and check-script validation
status: in-progress
tags: [project-electron-client-port, client-electron, afk, qa, e2e]
blocked_by: [29, 32]
---

# Add Electron smoke coverage and check-script validation

**Type**: AFK

**User stories covered**: 6, 13-14, 21, 23-25, 41, 52, 67-73, 89.

## Parent

Derived from `docs/electron-client-port-prd.md`.

## What to build

Add smoke coverage that proves the Electron shell, copied renderer, fixed-origin production path, and repository check contract work together. Keep browser-only renderer smoke tests available for isolating renderer failures, and add Electron E2E coverage for critical desktop-shell flows.

## Acceptance criteria

- [ ] Electron E2E launches the app, logs in with seeded development credentials, verifies channel list loading and auto-navigation, sends a message, verifies the message appears, and verifies the composer clears after success.
- [ ] Electron E2E reloads or reopens a deep channel route and verifies the app returns to a valid authenticated channel view.
- [ ] Electron E2E verifies external links open outside the app window and unsafe navigation is blocked.
- [ ] Electron E2E opens voice settings or an equivalent media-permission path without crashing and surfaces visible failures.
- [ ] Browser-only renderer smoke coverage remains available for the Electron renderer development port.
- [ ] The repository check script Electron selector passes for formatting, linting, typechecking, tests, and optional Electron E2E; package smoke coverage launches the unpacked app when practical.

## Blocked by

- Issue 29
- Issue 32
