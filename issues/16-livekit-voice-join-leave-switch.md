---
id: 16
name: LiveKit voice join/leave/switch with native audio worker
status: completed
tags: [client-iced]
blocked_by: [15]
---

# Issue 16: LiveKit voice join/leave/switch with native audio worker

**Type**: AFK

**User stories covered**: 60, 61, 62, 66, 67

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Implement the native voice connection path directly: use the server's voice token flow to connect to LiveKit through a long-lived worker/subscription, let users join, leave, and switch voice channels, and keep voice connection state visible and recoverable. The worker should own room and audio lifecycle details while the app exchanges typed commands and events with it.

## Acceptance criteria

- [ ] Joining a voice channel requests a server voice token and connects to the returned LiveKit room.
- [ ] Leaving a voice channel disconnects cleanly and removes local connected state.
- [ ] Switching voice channels disconnects from the previous room and joins the requested room without stale UI state.
- [ ] LiveKit, network, and audio initialization errors are visible and allow retry.
- [ ] Logout, session expiration, and app shutdown clean up any active voice connection.
- [ ] Worker contract/reducer tests cover join command, connected event, leave command, switch flow, error event, and cleanup.
- [ ] A manual or integration validation path exists for real LiveKit connection behavior with the development server stack.

## Blocked by

- Issue 15
