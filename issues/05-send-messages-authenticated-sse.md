---
id: 5
name: Send messages with authenticated SSE live updates
status: completed
tags: [client-iced]
blocked_by: [4]
---

# Issue 5: Send messages with authenticated SSE live updates

**Type**: AFK

**User stories covered**: 27, 28, 29, 70, 71, 72, 74, 79

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Complete the first live text-chat loop: users can send messages to the selected text channel, drafts clear after successful send, and authenticated SSE updates append incoming messages in real time. The SSE implementation should be isolated behind a typed interface that handles cookies, connected frames, ping comments, malformed input, reconnect/backoff, auth expiration, and shutdown.

## Acceptance criteria

- [ ] Users can send a non-empty message in the active text channel and see the draft clear after success.
- [ ] Send failures leave the draft recoverable and show a user-facing error.
- [ ] The app subscribes to the authenticated SSE stream only while signed in and stops it on logout/session expiration.
- [ ] Incoming message events append to the appropriate channel without requiring refresh.
- [ ] SSE connection state and reconnect/backoff behavior are represented in testable state, not hidden in widgets.
- [ ] Tests cover message send, draft clearing, send failure, SSE parsing, connected/ping frames, malformed events, reconnect decisions, auth expiration, and shutdown.

## Blocked by

- Issue 4
