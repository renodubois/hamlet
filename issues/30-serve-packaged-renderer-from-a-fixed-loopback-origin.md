---
id: 30
name: Serve packaged renderer from a fixed loopback origin
status: in-progress
tags: [project-electron-client-port, client-electron, afk, static-server, security]
blocked_by: [28]
---

# Serve packaged renderer from a fixed loopback origin

**Type**: AFK

**User stories covered**: 9, 12, 40-42, 60, 62-65, 87.

## Parent

Derived from `docs/electron-client-port-prd.md`.

## What to build

Add the packaged-mode rendering path that serves built renderer assets from a deterministic loopback HTTP origin instead of `file://`, custom protocols, or random ports. The static server should be a small, testable module that preserves cookie/localStorage behavior, supports deep routes, and provides a documented browser security posture for the packaged renderer.

## Acceptance criteria

- [ ] Packaged/production mode serves the built renderer from a fixed loopback HTTP origin and the Electron window loads that origin.
- [ ] LocalStorage and cookie-backed sessions remain stable across app relaunches when the server session is still valid.
- [ ] Deep channel routes refresh or reopen through SPA fallback without returning a missing-file response.
- [ ] The static server rejects path traversal, sends correct content types, and includes security headers including an initial CSP compatible with Hamlet HTTP/SSE/WebSocket/LiveKit and embed needs.
- [ ] Static-server tests cover successful asset serving, fallback behavior, security headers, path traversal rejection, MIME handling, and startup error surfaces.

## Blocked by

- Issue 28
