---
id: 12
name: Safe URL recognition and external link opening
status: completed
tags: [client-iced]
blocked_by: [5]
---

# Issue 12: Safe URL recognition and external link opening

**Type**: AFK

**User stories covered**: 37, 38

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Recognize URLs in native message text and provide safe external-open behavior. Links should be visually identifiable and open through an explicit platform capability rather than rendering or executing arbitrary content inside the app.

## Acceptance criteria

- [ ] URLs in message text are recognized and displayed with an openable affordance.
- [ ] Non-URL text remains unchanged and does not create false-positive link actions.
- [ ] Opening a link delegates to a platform external-open service that can be faked in tests.
- [ ] Unsupported or malformed link targets fail safely with a visible error or no-op, not an in-app execution path.
- [ ] Tests cover URL parsing, multiple links in one message, punctuation boundaries, platform open success, and platform open failure.

## Blocked by

- Issue 5
