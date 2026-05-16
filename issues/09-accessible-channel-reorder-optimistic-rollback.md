---
id: 9
name: Accessible channel reorder with optimistic rollback and live reorder events
status: completed
tags: [client-iced]
blocked_by: [8]
---

# Issue 9: Accessible channel reorder with optimistic rollback and live reorder events

**Type**: AFK

**User stories covered**: 19, 20, 22, browser drag/drop compromise from 82

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Add native channel reordering with an accessible interaction model, such as move up/down controls or a proven pointer-driven reorder path. Reordering should update the local list optimistically, commit on server success, recover on failure, and apply live reorder events from other clients.

## Acceptance criteria

- [ ] Users can reorder channels using a keyboard-accessible native interaction.
- [ ] Local channel order updates immediately when a reorder is submitted.
- [ ] Successful server responses commit the optimistic order.
- [ ] Failed server responses roll back to the previous order or refetch authoritative server order with a visible error.
- [ ] Channel-reorder SSE events from other clients update the visible order.
- [ ] Tests cover optimistic update, commit, rollback/refetch, live reorder events, and boundary positions.

## Blocked by

- Issue 8
