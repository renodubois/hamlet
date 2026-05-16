# Issue 2: Cookie-backed login/register with clear errors

**Type**: AFK

**User stories covered**: 4, 5, 6, 7, 8, 70, 71, 73, 79

## Parent

Derived from `docs/iced-native-client-conversion-prd.md`.

## What to build

Add native login and registration flows that use the configured Hamlet server URL through a typed, cookie-capable HTTP transport. Successful auth should establish an authenticated client session for later requests, while credential, validation, and connectivity failures should produce clear user-facing errors. Debug builds should preserve convenient local-development login shortcuts.

## Acceptance criteria

- [ ] Users can log in with username/password and reach the signed-in app state on success.
- [ ] Users can register a new account and reach the signed-in app state on success.
- [ ] Session cookies set by login/register are retained by the transport and reused for authenticated requests.
- [ ] Invalid credentials, duplicate usernames, invalid input, and unreachable servers surface distinct, understandable errors.
- [ ] Debug-only dev login shortcuts remain available without leaking into release behavior.
- [ ] Transport/protocol tests cover auth DTOs, cookie reuse, configured base URL handling, and auth error mapping.

## Blocked by

- Issue 1
