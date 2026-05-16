# Issue 10: Settings shell and display-name profile updates

**Type**: AFK

**User stories covered**: 50, 51, 52, 53, 56

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Add a native settings surface focused on profile identity and display-name management. Signed-in users should be able to open settings, view their current profile, update or clear their display name, understand validation failures, and see profile changes reflected in already-visible UI such as the sidebar and message author labels.

## Acceptance criteria

- [ ] Signed-in users can open and close a settings/profile view from the app shell.
- [ ] The settings view shows the current username and display name state.
- [ ] Users can update their display name and see the new name reflected in visible profile and message UI.
- [ ] Users can clear their display name and fall back to username display.
- [ ] Validation and transport errors are shown clearly without discarding the user's input.
- [ ] API/reducer tests cover settings open/close, display-name update, clear display name, validation failure, and visible profile refresh.

## Blocked by

- Issue 5
