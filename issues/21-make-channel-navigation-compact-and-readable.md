---
id: 21
name: Make channel navigation compact and readable
status: completed
tags: [project-iced-visual-refresh, client-iced, afk, sidebar]
blocked_by: [20]
---


# Make channel navigation compact and readable

**Type**: AFK

**User stories covered**: 5, 6, 7, 8, 9, 10, 11, 12, 44

## Parent

`docs/iced-client-visual-refresh-prd.md`

## What to build

Refresh sidebar channel navigation so text and voice channels render as compact, readable rows with clear selected/inactive styling, graceful long-name behavior, nested voice participants, and compact deterministic reorder controls instead of large full-width move buttons.

## Acceptance criteria

- [ ] Channel rows are compact, selected state is visually obvious, inactive channels are muted, and long names do not collide with controls.
- [ ] Voice channels are visually distinct from text channels and show participants nested under the relevant voice channel.
- [ ] Reorder remains deterministic and tested, but controls no longer dominate every row as full-width text buttons.
- [ ] Headless smoke tests are updated if reorder labels/selectors change.

## Blocked by

- #20
