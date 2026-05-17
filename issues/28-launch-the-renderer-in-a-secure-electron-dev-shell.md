---
id: 28
name: Launch the renderer in a secure Electron dev shell
status: in-progress
tags: [project-electron-client-port, client-electron, afk, electron-shell, security]
blocked_by: [27]
---

# Launch the renderer in a secure Electron dev shell

**Type**: AFK

**User stories covered**: 1, 12, 40, 44-45, 54-56, 58, 60-62, 66, 79, 81-82, 87.

## Parent

Derived from `docs/electron-client-port-prd.md`.

## What to build

Add the first Electron desktop shell around the copied renderer for development. The shell should load the trusted localhost renderer origin, provide hot-reload-friendly development commands, centralize app lifecycle and window creation, build and typecheck main/preload code separately, and keep the renderer browser-only with no normal Hamlet APIs routed through IPC.

## Acceptance criteria

- [ ] An Electron development command launches the copied renderer from a distinct localhost origin and supports normal renderer iteration.
- [ ] BrowserWindow construction uses explicit secure defaults: no Node integration, context isolation enabled, sandbox enabled, web security enabled, and insecure content disabled.
- [ ] The preload boundary is empty or exposes only typed, read-only metadata; no raw IPC object or broad system API is available to renderer code.
- [ ] Main/preload build and typecheck steps are covered by package scripts, and focused tests or assertions cover trusted renderer loading and secure window defaults.
- [ ] Auth, channel, message, SSE, and LiveKit calls still go directly from the renderer to the Hamlet server.

## Blocked by

- Issue 27
