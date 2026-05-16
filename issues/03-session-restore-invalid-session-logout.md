# Issue 3: Startup session restore, invalid-session recovery, and logout

**Type**: AFK

**User stories covered**: 9, 10, 11, 70, 73, 74

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Restore or validate the native client's authenticated session during startup and provide a reliable logout path. A valid stored session should resume into the signed-in app; an unauthorized, expired, or unusable session should return to login without trapping the user in a broken state. Logout should clear local signed-in state even when the server session is already stale.

## Acceptance criteria

- [ ] Startup validates an existing session against the server before showing signed-in content.
- [ ] Valid sessions resume into the signed-in shell with the current user's profile available.
- [ ] Invalid, expired, or unauthorized sessions return to login with a clear recoverable state.
- [ ] Logout calls the server when possible, clears local auth state, and returns to login.
- [ ] Reducer/transport tests cover boot success, boot failure, unauthorized responses, and logout recovery.

## Blocked by

- Issue 2
