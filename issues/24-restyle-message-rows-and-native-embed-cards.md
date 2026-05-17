---
id: 24
name: Restyle message rows and native embed cards
status: completed
tags: [project-iced-visual-refresh, client-iced, afk, messages, embeds]
blocked_by: [20]
---


# Restyle message rows and native embed cards

**Type**: AFK

**User stories covered**: 29, 30, 31, 32, 33, 34, 35, 36, 43

## Parent

`docs/iced-client-visual-refresh-prd.md`

## What to build

Restyle message presentation so each row separates avatar, author, and body, wraps long content cleanly, keeps links visibly clickable, preserves edit/delete access with reduced visual weight, and renders embeds as constrained native cards that keep safe external-open behavior.

## Acceptance criteria

- [ ] Message rows show avatar, bold author, and body in separate visual areas with clean wrapping for long messages.
- [ ] Links remain visibly actionable and existing safe external-open behavior is preserved.
- [ ] Edit/delete actions for owned messages remain available without appearing as large permanent controls on every row.
- [ ] Embed previews render as contained cards and preserve existing native-client security behavior.

## Blocked by

- #20
