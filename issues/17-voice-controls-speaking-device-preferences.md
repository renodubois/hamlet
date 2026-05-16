---
id: 17
name: Voice controls, speaking indicators, device preferences, and permission handling
status: completed
tags: [client-iced]
blocked_by: [16]
---

# Issue 17: Voice controls, speaking indicators, device preferences, and permission handling

**Type**: AFK

**User stories covered**: 63, 64, 65, 68, 69, 78

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Complete native voice usability after the join/leave path works. Users should be able to mute/unmute, deafen/undeafen, see speaking indicators, persist supported device preferences, and understand microphone permission failures. Platform metadata needed for microphone access should be included with the native app where applicable.

## Acceptance criteria

- [ ] Users can mute and unmute their microphone while connected to voice.
- [ ] Users can deafen and undeafen remote audio while connected to voice.
- [ ] Speaking indicators update from worker/server events and clear when speaking stops or voice disconnects.
- [ ] Supported microphone/output preferences persist and are reapplied on restart.
- [ ] Microphone permission failures are explained in user-facing language with a recoverable state.
- [ ] Required microphone permission metadata is included for platforms that need it.
- [ ] Worker/reducer tests cover mute, unmute, deafen, undeafen, speaking events, device preference persistence, permission failure, and disconnect cleanup.

## Blocked by

- Issue 16
