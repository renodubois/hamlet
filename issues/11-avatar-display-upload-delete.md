---
id: 11
name: Avatar display, fallback generation, upload/delete, and image URL handling
status: completed
tags: [client-iced]
blocked_by: [10]
---

# Issue 11: Avatar display, fallback generation, upload/delete, and image URL handling

**Type**: AFK

**User stories covered**: 48, 49, 54, 55, 56

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Add native avatar rendering and profile avatar management. The app should show user avatars next to messages and in the sidebar, generate deterministic fallbacks for users without avatars, resolve relative avatar/upload URLs against the configured server, and let users upload or delete their avatar through native file selection and existing server behavior.

## Acceptance criteria

- [ ] Messages and sidebar/profile surfaces show avatars when a user has one.
- [ ] Users without avatars receive deterministic fallback identity images.
- [ ] Relative avatar URLs resolve against the configured Hamlet server URL.
- [ ] Users can select a local image file and upload it as their avatar.
- [ ] Users can delete their avatar and return to the fallback image.
- [ ] Avatar upload/delete refreshes visible profile and existing message rows.
- [ ] Tests cover fallback determinism, URL resolution, image fetch/cache behavior, upload success/failure, delete success/failure, and profile refresh.

## Blocked by

- Issue 10
