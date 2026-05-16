---
id: 15
name: Voice presence in channel list without joining audio
status: completed
tags: [client-iced]
blocked_by: [5]
---

# Issue 15: Voice presence in channel list without joining audio

**Type**: AFK

**User stories covered**: 57, 58, 59

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Show voice-channel participant presence in the native channel list before adding local audio connection controls. The app should fetch current participants from the server, display who is connected to each voice channel, and keep the list current through voice presence events.

## Acceptance criteria

- [ ] Voice channels show currently connected participants when the signed-in shell loads.
- [ ] Participant join events update the appropriate voice channel live.
- [ ] Participant leave events remove users from the appropriate voice channel live.
- [ ] Presence state is cleared on logout/session expiration and does not imply the local user has joined audio.
- [ ] Loading and error states for voice participant data are visible or gracefully degraded.
- [ ] API/protocol/reducer tests cover participant DTOs, initial participant fetch, join events, leave events, logout cleanup, and fetch failure.

## Blocked by

- Issue 5
