# Issue 7: Typing notifications and expiring indicators

**Type**: AFK

**User stories covered**: 45, 46, 47

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Add native typing notifications for text channels. The app should send best-effort, throttled typing pings while the current user is composing a non-empty draft, display typing indicators for other users, and expire stale indicators locally so the UI cannot get stuck showing someone typing forever.

## Acceptance criteria

- [ ] Non-empty draft activity sends typing pings for the active text channel.
- [ ] Typing pings are throttled so rapid keystrokes do not spam the server.
- [ ] Empty drafts and inactive channels do not send typing pings.
- [ ] Typing events from other users display indicators in the appropriate channel.
- [ ] Typing indicators expire automatically after the configured timeout.
- [ ] Tests cover throttling, empty-draft suppression, per-channel indicator updates, self-event handling, and expiry timers.

## Blocked by

- Issue 5
