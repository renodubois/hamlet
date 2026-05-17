---
id: 20
name: Replace signed-in layout with two-pane chat shell
status: completed
tags: [project-iced-visual-refresh, client-iced, afk, layout]
blocked_by: [19]
---


# Replace signed-in layout with two-pane chat shell

**Type**: AFK

**User stories covered**: 1, 2, 3, 4, 20, 21, 22, 23, 43

## Parent

`docs/iced-client-visual-refresh-prd.md`

## What to build

Replace the signed-in debug-style vertical layout with a full-height chat shell: a fixed dark sidebar and a fill-width light main content area. Text channels should expose a persistent channel header, independently scrollable message body, and composer region while keeping current state, reducer, realtime, and API behavior intact.

## Acceptance criteria

- [ ] The signed-in app renders as a stable two-pane shell with dark sidebar and light main channel area.
- [ ] The Hamlet workspace title appears in the sidebar header and global debug/status text no longer competes at the top of the signed-in layout.
- [ ] Text-channel content has a fixed channel header, independent message scrolling, and a bottom composer region.
- [ ] Existing login, landing-in-general, realtime, and message-send smoke paths still pass or are updated for intentional label changes.

## Blocked by

- #19
