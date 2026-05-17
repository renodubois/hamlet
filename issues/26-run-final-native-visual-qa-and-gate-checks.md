---
id: 26
name: Run final native visual QA and gate checks
status: completed
tags: [project-iced-visual-refresh, client-iced, hitl, qa]
blocked_by: [21, 22, 23, 24, 25]
---


# Run final native visual QA and gate checks

**Type**: HITL

**User stories covered**: 44, 45

## Parent

`docs/iced-client-visual-refresh-prd.md`

## What to build

Perform the human validation pass for the completed visual refresh and update any final smoke selectors or documentation needed for the native client gate. Manual QA should focus on minimum-window overlap, reachability, and the core Hamlet chat affordances that are difficult to assert headlessly.

## Acceptance criteria

- [ ] Headless Iced smoke tests verify the refreshed shell exposes Hamlet title, add-channel action, selected general channel, composer input, emoji control, and send control.
- [ ] Manual QA covers 1200x800 and the configured 900x600 minimum window size.
- [ ] Manual QA verifies sidebar, create-channel controls, settings/profile controls, text composer, emoji picker, and voice controls remain reachable and non-overlapping.
- [ ] The native client gate passes: formatting, check, clippy with warnings denied, and tests.

## Blocked by

- #21
- #22
- #23
- #24
- #25
