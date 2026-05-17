---
id: 25
name: Bring voice channels into the refreshed main shell
status: completed
tags: [project-iced-visual-refresh, client-iced, afk, voice]
blocked_by: [20]
---


# Bring voice channels into the refreshed main shell

**Type**: AFK

**User stories covered**: 37, 38, 39, 43

## Parent

`docs/iced-client-visual-refresh-prd.md`

## What to build

Adapt voice channel screens to the same refreshed main shell used by text channels. Join, leave, switch, mute, and deafen controls should be clearly styled, and connection status/errors should appear in the voice channel body near the action context while preserving LiveKit worker contracts.

## Acceptance criteria

- [ ] Voice channel views use the refreshed main shell so switching between text and voice channels feels coherent.
- [ ] Join, leave, switch, mute, and deafen controls remain reachable and clearly styled.
- [ ] Voice connection errors and status messages appear in the voice channel body, not as unrelated global clutter.
- [ ] Existing voice reducer, API, token, and worker contracts remain unchanged and covered by tests.

## Blocked by

- #20
