# Issue 14: Emoji picker and keyboard-friendly draft insertion

**Type**: AFK

**User stories covered**: 43, 44

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Add an emoji picker for message drafts that works naturally in the native Iced UI. Users should be able to browse or search emoji, insert them into the current draft, and continue typing without the picker disrupting draft state or keyboard flow.

## Acceptance criteria

- [ ] Users can open and close an emoji picker from the message composer.
- [ ] Users can search or navigate emoji choices with the keyboard.
- [ ] Selecting an emoji inserts it into the current draft without losing existing text.
- [ ] Closing the picker preserves the draft and returns focus to a sensible composer state.
- [ ] Tests cover emoji search/filter behavior, picker open/close state, insertion into drafts, and keyboard navigation state.

## Blocked by

- Issue 5
