---
id: 8
name: Create text/voice channels and apply live channel-create events
status: completed
tags: [client-iced]
blocked_by: [5]
---

# Issue 8: Create text/voice channels and apply live channel-create events

**Type**: AFK

**User stories covered**: 16, 17, 18, 21

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Let signed-in users create both text and voice channels from the native client and keep the channel list live when other clients create channels. Creation should use the existing server API, preserve server-defined ordering, and surface validation or connectivity failures clearly.

## Acceptance criteria

- [ ] Users can create a text channel and then navigate to it as a normal text channel.
- [ ] Users can create a voice channel and see it rendered as a voice channel without a message view.
- [ ] Channel creation errors are visible and leave the create form recoverable.
- [ ] Channel-created SSE events from other clients update the channel list without restart or refresh.
- [ ] Channel ordering remains consistent with server responses after local creation and live events.
- [ ] API/reducer tests cover text-channel creation, voice-channel creation, failures, and live channel-created events.

## Blocked by

- Issue 5
