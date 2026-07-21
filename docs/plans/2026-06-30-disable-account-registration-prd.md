## Problem Statement

Hamlet server operators currently cannot turn off public account creation. Anyone who can reach a running server can use the registration endpoint and the Electron client exposes a "Create account" affordance on the sign-in screen. Operators need a disk-backed server configuration they can modify to close registration while preserving login for existing users.

## Solution

Add a server-owned configuration file with account registration enabled by default for backwards compatibility. The server reads the file at startup, exposes the public registration setting to clients, and rejects registration requests with a clear error when registration is disabled. The Electron sign-in screen reads the public setting from the configured server URL and hides registration affordances when the server reports registration is closed.

## User Stories

1. As a Hamlet server operator, I want a server config file on disk, so that I can change registration policy without editing source code.
2. As a Hamlet server operator, I want account registration to remain enabled by default, so that existing local and development workflows keep working until I opt out.
3. As a Hamlet server operator, I want to disable account registration with a single boolean setting, so that I can run an invite-only or closed server.
4. As a Hamlet server operator, I want the server to reject disabled registration attempts, so that clients or scripts cannot bypass the UI.
5. As a Hamlet server operator, I want registration-disabled errors to be explicit, so that failed signup attempts are understandable in logs and clients.
6. As an existing Hamlet user, I want login to continue working when registration is disabled, so that closing registration does not lock me out.
7. As a new visitor to a closed Hamlet server, I want not to see a create-account action, so that I do not try a flow the server will reject.
8. As a Hamlet client user, I want the sign-in screen to learn the registration policy from the currently configured server URL, so that the UI matches the server I am connecting to.
9. As a Hamlet developer, I want tests for the closed-registration path, so that future auth or login-screen changes do not accidentally reopen registration.
10. As a Hamlet developer, I want the public config contract to be narrow and stable, so that the client does not depend on private server settings.

## Implementation Decisions

- Extend the existing server configuration module instead of introducing ad hoc environment reads elsewhere.
- Add a disk-backed server settings object with an `account_registration_enabled` boolean that defaults to `true`.
- Resolve the server settings file from an explicit path override when provided, otherwise use the server application data directory so Docker and local persistent deployments keep config next to durable server state.
- Create or document a default settings file containing enabled registration so operators have a concrete file to edit.
- Parse the settings file at startup and fail clearly on invalid configuration instead of silently ignoring malformed policy.
- Keep database schema unchanged; registration policy is runtime server configuration, not persisted account data.
- Add a public, unauthenticated endpoint that returns only safe client-facing settings, starting with the account registration flag.
- Gate the existing register endpoint on the loaded setting before creating users or sessions.
- Use the existing JSON error envelope and add a distinct registration-disabled error kind.
- Update the Electron auth API and sign-in screen so the client fetches public server settings and hides the register-mode toggle when registration is disabled.
- Preserve existing login, logout, `/me`, seeded development users, and authenticated application behavior.

## Testing Decisions

- Tests should assert external behavior: config defaults and parsing, HTTP responses, user/session side effects, and visible UI affordances.
- Server configuration tests will cover default enabled behavior and disabled config parsing without depending on implementation internals beyond the config module API.
- Auth HTTP tests will cover successful registration by default, disabled registration returning the explicit error, no session cookie on disabled registration, and the public settings endpoint.
- Client API tests will cover fetching public server settings from the configured server URL.
- Login-screen tests will cover registration being available when enabled and hidden when disabled through the mocked server public settings endpoint.
- Existing auth and login tests are prior art for request/response assertions and React/MSW integration coverage.

## Out of Scope

- Admin UI for changing server settings at runtime.
- Hot-reloading registration policy without restarting the server.
- Invitation codes, allowlists, or per-user administrative roles.
- Email verification, password reset, or moderation workflows.
- Migrating existing users or sessions.

## Further Notes

The first implementation should keep the public config surface intentionally small. Future public settings can reuse the same endpoint and settings file structure if operators need more disk-backed server controls.
