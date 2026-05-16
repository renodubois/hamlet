# Issue 6: Edit/delete own messages and reconcile message SSE events

**Type**: AFK

**User stories covered**: 30, 31, 33, 34, 35, 36

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Add message author actions for editing and deleting messages, plus realtime reconciliation for server message update/delete events. The UI should only expose actions the current user can take, keep failed actions visible and recoverable, and update message lists when edits or deletes arrive over SSE.

## Acceptance criteria

- [ ] Own messages expose edit/delete actions, while messages from other users do not expose unavailable controls.
- [ ] Editing a message updates the message through the server and exits edit mode on success.
- [ ] Deleting a message removes it through the server and updates the visible list on success.
- [ ] Edit/delete API failures are shown without losing the user's context.
- [ ] SSE edit events replace the correct message, and delete events remove the correct message.
- [ ] Events for inactive channels do not corrupt the active channel view.
- [ ] API/reducer tests cover action visibility, edit success/failure, delete success/failure, and SSE reconciliation.

## Blocked by

- Issue 5
