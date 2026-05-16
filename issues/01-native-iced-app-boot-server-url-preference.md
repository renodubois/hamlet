---
id: 1
name: Native Iced app boot, server URL preference, and fixture harness
status: completed
tags: [client-iced]
blocked_by: []
---

# Issue 1: Native Iced app boot, server URL preference, and fixture harness

**Type**: AFK

**User stories covered**: 1, 2, 3, 74, 75, 79

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Create the first runnable native Iced client alongside the existing Tauri/Solid client. It should boot into a signed-out route, display a basic login shell, let the user configure the Hamlet server URL, persist that preference in native storage, and include deterministic fake services/fixtures that later slices can use to test app state transitions without a real UI or server.

## Acceptance criteria

- [ ] Native app launches in development and shows a signed-out/login view rather than a blank window.
- [ ] A user can edit the server URL and the value survives app restart or equivalent storage reload in tests.
- [ ] Boot and signed-out state transitions are represented by typed app state/messages and covered by reducer-style tests.
- [ ] Fake API, fake storage, and fake realtime fixtures exist for deterministic native-client tests.
- [ ] Existing Tauri/Solid client remains available and is not removed or made dependent on the native client.

## Blocked by

None - can start immediately
