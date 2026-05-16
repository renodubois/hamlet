---
id: 13
name: Native embeds, image previews, embed events, and suppress action
status: completed
tags: [client-iced]
blocked_by: [6, 12]
---

# Issue 13: Native embeds, image previews, embed events, and suppress action

**Type**: AFK

**User stories covered**: 32, 39, 40, 41, 42, iframe/embed compromise from 82

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Render server-provided embeds as native preview cards where feasible, while degrading unsupported rich/video iframe embeds to an external-open action. Embed updates from the server should patch existing messages, and message authors should be able to suppress unwanted embeds on their own messages.

## Acceptance criteria

- [ ] Link embeds can show title, site, description, and available preview image data.
- [ ] Photo/image embeds render as native image previews when supported.
- [ ] Rich or video embeds that require iframes render as native cards with an external-open action instead of embedded web content.
- [ ] Embed-update SSE events patch the correct existing message without a manual refresh.
- [ ] Authors can suppress embeds on their own messages, and unavailable suppress actions are hidden for other users.
- [ ] API/protocol/reducer tests cover embed DTOs, image preview data, unsupported rich/video fallback, embed update events, suppress success, and suppress failure.

## Blocked by

- Issue 6
- Issue 12
