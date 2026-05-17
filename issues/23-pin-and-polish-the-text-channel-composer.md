---
id: 23
name: Pin and polish the text-channel composer
status: completed
tags: [project-iced-visual-refresh, client-iced, afk, composer]
blocked_by: [20]
---


# Pin and polish the text-channel composer

**Type**: AFK

**User stories covered**: 23, 24, 25, 26, 27, 28, 44

## Parent

`docs/iced-client-visual-refresh-prd.md`

## What to build

Polish the text composer as a pinned bottom bar with a comfortable width-filling input, compact distinct emoji control, and visually primary but not oversized send control. Emoji picker behavior should remain stable and should not cover or break the composer layout.

## Acceptance criteria

- [ ] The composer remains pinned at the bottom of text channels while messages scroll independently.
- [ ] The message input expands across available width and emoji/send controls sit beside it without overlap.
- [ ] Emoji insertion and send behavior remain covered by smoke tests and preserve current reducer/API behavior.
- [ ] The send control is visually primary and the emoji control is compact and distinct.

## Blocked by

- #20
