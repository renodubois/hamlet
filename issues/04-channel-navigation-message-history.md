# Issue 4: Channel navigation with message-history read path

**Type**: AFK

**User stories covered**: 12, 13, 14, 15, 23, 24, 25, 26, 70, 71

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Build the signed-in shell's basic channel navigation and text-channel read experience. The native app should load server-ordered channels, visually distinguish text and voice channels, choose the first text channel when no channel is selected, fetch message history for selected text channels, and show channel headers, loading states, and errors.

## Acceptance criteria

- [ ] Signed-in users see text and voice channels in the order returned by the server.
- [ ] When no channel is selected, the app navigates to the first available text channel.
- [ ] Text and voice channels are visually distinct, and selecting a voice channel does not open a message history view.
- [ ] Selecting a text channel fetches and renders its message history with a header showing the current channel name.
- [ ] Message-history loading and failure states are visible and recoverable.
- [ ] API/protocol/reducer tests cover channel DTOs, channel selection, ordered rendering data, message-history success, and message-history failure.

## Blocked by

- Issue 3
