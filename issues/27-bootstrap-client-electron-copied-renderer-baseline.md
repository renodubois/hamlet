---
id: 27
name: Bootstrap client-electron copied renderer baseline
status: in-progress
tags: [project-electron-client-port, client-electron, afk, renderer]
blocked_by: []
---

# Bootstrap `client-electron` copied renderer baseline

**Type**: AFK

**User stories covered**: Renderer baseline for 2-39, 58-61, 66-69, 71, 79, 83, 86-89.

## Parent

Derived from `docs/electron-client-port-prd.md`.

## What to build

Create a side-by-side Electron Solid renderer by copying and adapting the existing browser-based client into an Electron-dedicated workspace while leaving the Tauri/Solid and native Iced clients available. The renderer should run as a standalone Vite app on a distinct development port, keep calling the Hamlet server directly for HTTP, SSE, WebRTC, and LiveKit, preserve existing localStorage/server URL behavior, and expose the standard package scripts expected by repository checks.

## Acceptance criteria

- [ ] The Electron renderer can run as a standalone browser app on a distinct development port without disturbing the existing Tauri/Solid or native Iced clients.
- [ ] Login, registration, session restore/logout, channel navigation, message history, message sending, profile/avatar surfaces, voice settings, local preferences, and accessibility-covered renderer behavior match the copied client baseline.
- [ ] Normal application API calls remain in the renderer using credentialed HTTP, SSE, WebRTC, and LiveKit directly; no Electron IPC proxy is introduced.
- [ ] Formatting, linting, typechecking, unit/component/integration tests, and renderer build scripts exist and pass for the Electron renderer.
- [ ] The repository-level Electron client selector succeeds for the non-E2E check contract once the Electron client directory exists.

## Blocked by

None - can start immediately
