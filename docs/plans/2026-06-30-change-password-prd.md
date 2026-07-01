## Problem Statement

Hamlet users can register, log in, update their display name, upload an avatar, and log out from Settings, but they cannot change the password for their account. If a user wants to rotate a compromised password or replace a temporary password, they currently have no self-service path inside the app.

## Solution

Add a self-service password change flow in the Settings menu. An authenticated user opens Settings, enters their current password, enters and confirms a new password, and submits the form. The server verifies the current password against the existing password credential, hashes the new password with the existing Argon2 password hashing path, stores the new hash, and returns a clear success or error result. The current session remains signed in after a successful change.

## User Stories

1. As an authenticated Hamlet user, I want to change my password from Settings, so that I can rotate credentials without creating a new account.
2. As an authenticated Hamlet user, I want the password change form to ask for my current password, so that someone with unattended access to my session cannot silently take over my account.
3. As an authenticated Hamlet user, I want to enter my new password twice, so that accidental typos are caught before my credential changes.
4. As an authenticated Hamlet user, I want a clear success message after changing my password, so that I know the new credential was saved.
5. As an authenticated Hamlet user, I want a clear error when my current password is wrong, so that I know why the change was rejected.
6. As an authenticated Hamlet user, I want a clear error when the new password fields do not match, so that I can correct the form before submitting.
7. As an authenticated Hamlet user, I want empty password fields rejected, so that I cannot accidentally set an unusable blank password.
8. As an authenticated Hamlet user, I want the password fields cleared after a successful change, so that the passwords are not left visible in the settings form state.
9. As an authenticated Hamlet user, I want the app to keep me signed in after a successful password change, so that password rotation does not interrupt my current work.
10. As an authenticated Hamlet user, I want my old password to stop working after the change, so that rotating the password meaningfully improves account security.
11. As an authenticated Hamlet user, I want my new password to work on the next login, so that I can verify the change after logging out.
12. As an authenticated Hamlet user using keyboard navigation, I want the password settings controls to be reachable and labeled, so that I can change my password without a mouse.
13. As a screen reader user, I want password change success and error messages announced, so that I receive feedback without relying on visual cues.
14. As a user on a slow connection, I want the form to show an in-progress state and avoid duplicate submissions, so that I do not submit the same password change multiple times.
15. As a Hamlet maintainer, I want the feature to reuse the existing password hashing and verification primitives, so that credential behavior stays consistent with register and login.
16. As a Hamlet maintainer, I want automated server tests proving the old password is replaced by the new password, so that the credential contract is protected.
17. As a Hamlet maintainer, I want automated client tests proving the settings form validates, submits, and reports errors accessibly, so that UI regressions are caught.
18. As a Hamlet maintainer, I want unauthenticated requests rejected by the existing auth middleware, so that password changes cannot be made without a valid session.

## Implementation Decisions

- Add an authenticated password change API under the current-user auth surface rather than creating an account-management subsystem.
- Require the current password and a non-empty new password in the request body. This follows the existing product convention that passwords must be present while avoiding a new password policy beyond what registration and login already enforce.
- Verify the current password using the existing password verification primitive and store the replacement using the existing Argon2 hashing primitive.
- Return authentication-style errors for an incorrect current password and bad-request errors for malformed or empty inputs. Do not reveal credential internals beyond the existing error envelope.
- Keep existing sessions valid after a successful password change. The immediate user need is self-service rotation, not global session revocation.
- Add a small client API helper for the password change endpoint and keep the form logic inside Settings, near the existing profile/account controls.
- Place the password change UI in the User Profile settings section because that is the existing account-related settings surface alongside display name, avatar, and logout.
- Client-side validation should check that all fields are filled and that the new password confirmation matches before making a network request. The server remains authoritative.
- On success, clear all password fields and show a status message. On failure, preserve field values except when a future server decision explicitly requires clearing them.

## Testing Decisions

- Good tests should verify externally observable behavior: HTTP status codes, credential replacement, client request shape, accessible form state, success messages, error messages, and disabled duplicate-submit behavior. Tests should not assert private implementation details except that stored password secrets remain hashed where existing auth tests already do so.
- Server integration tests should cover successful password change, old-password rejection after the change, new-password login after the change, wrong-current-password rejection, empty-password rejection, and auth gating.
- Auth primitive tests should remain focused on hashing and verification behavior; no new deep module is needed unless the credential update logic grows beyond the current auth surface.
- Client API tests should cover the request path and JSON body for changing passwords.
- Settings modal component tests should cover validation without a network request, successful submission, wrong-current-password error display, field clearing after success, and accessibility on the profile tab.
- MSW handlers should model password changes so component tests can verify behavior without a real server.

## Out of Scope

- Password reset by email or recovery codes.
- Password strength meters, password history, or breached-password checks.
- Forced logout of other active sessions after changing a password.
- Changing usernames or email addresses.
- Administrative password resets for other users.
- OAuth, passkeys, or non-password credential providers.

## Further Notes

The feature should not require a database schema change because password credentials already store an Argon2 hash in the credential secret field. The endpoint should be covered by the existing authenticated route scope, and the UI should follow the Settings modal's existing visual and accessibility patterns.
