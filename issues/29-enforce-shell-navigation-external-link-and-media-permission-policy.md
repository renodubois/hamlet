---
id: 29
name: Enforce shell navigation, external-link, and media-permission policy
status: in-progress
tags: [project-electron-client-port, client-electron, afk, security, voice]
blocked_by: [28]
---

# Enforce shell navigation, external-link, and media-permission policy

**Type**: AFK

**User stories covered**: 31, 34, 46-57, 72, 81-82, 87-88.

## Parent

Derived from `docs/electron-client-port-prd.md`.

## What to build

Implement a testable Electron shell security policy for navigation, popups, external links, and permissions. The app should stay on the trusted renderer origin, open validated HTTP/HTTPS links in the operating system browser, block unsafe schemes and unexpected navigation, and request only the media permissions needed for voice features from the trusted renderer.

## Acceptance criteria

- [ ] Top-level navigation and popup handling allow only the trusted app surface and block unexpected app-window navigation.
- [ ] Message and embed links with valid HTTP/HTTPS URLs open externally through the system browser; file, script, arbitrary custom, and unknown schemes are blocked.
- [ ] Permission decisions default-deny unrelated capabilities and allow only necessary microphone/media-device permissions for the trusted renderer origin.
- [ ] Voice settings and media-permission paths remain visible and recoverable when device or LiveKit access fails instead of crashing the app.
- [ ] Focused tests cover the policy decisions, and Electron smoke coverage exercises external-link and media-permission behavior.

## Blocked by

- Issue 28
