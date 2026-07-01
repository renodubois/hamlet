## Problem Statement

Hamlet server operators need a safe way to create accounts for people they invite to a self-hosted server. Today, account creation is exposed as part of the public application flow, but an operator has no dedicated command-line workflow for provisioning a username with a temporary password against the server's persistent SQLite database. Operators need a tool that works with the same database configuration as the server, reuses the existing password hashing and user creation path, and clearly reports duplicate users, invalid input, and success or failure.

## Solution

Add a server-side admin CLI command that creates a password-backed Hamlet user directly in the configured SQLite database. The CLI will be run by someone hosting the server from the command line, for example through Cargo during local hosting or as a built binary in a deployment. The admin provides a username and temporary password. The command loads the same database configuration as the server, initializes the schema/migrations if needed, calls the existing user registration primitive, and prints a clear success message with the created user id. Duplicate usernames and invalid arguments produce non-zero exits with actionable messages.

This approach avoids introducing an unauthenticated HTTP admin endpoint and avoids duplicating password hashing or credential insertion logic. It keeps the security-sensitive behavior centralized in the existing auth module while giving operators a deterministic provisioning path.

## User Stories

1. As a Hamlet server admin, I want to create a user from my shell, so that I can invite someone without asking them to self-register first.
2. As a Hamlet server admin, I want to specify a username, so that the created account has the identity I intend to share with the invitee.
3. As a Hamlet server admin, I want to specify a temporary password, so that the invitee can log in and change their profile later.
4. As a Hamlet server admin, I want the CLI to use the same database configuration as the server, so that I do not accidentally create accounts in the wrong database.
5. As a Hamlet server admin, I want the CLI to work when the server process is not accepting HTTP requests, so that I can provision users during maintenance or initial setup.
6. As a Hamlet server admin, I want the CLI to reuse the server's password hashing implementation, so that credentials created by the CLI are as secure as credentials created through normal registration.
7. As a Hamlet server admin, I want the CLI to reuse the server's user creation path, so that default per-user bookkeeping remains consistent.
8. As a Hamlet server admin, I want duplicate usernames to be rejected clearly, so that I know whether I mistyped or the user already exists.
9. As a Hamlet server admin, I want missing usernames to fail before touching the database, so that malformed commands do not create unusable accounts.
10. As a Hamlet server admin, I want missing passwords to fail before touching the database, so that accounts are not created with unusable credentials.
11. As a Hamlet server admin, I want whitespace-only usernames to be rejected, so that account names remain visible and usable in the client.
12. As a Hamlet server admin, I want whitespace-only passwords to be rejected, so that accidentally blank temporary passwords are not accepted.
13. As a Hamlet server admin, I want help text that shows the expected syntax, so that I can recover from a failed command without reading source code.
14. As a Hamlet server admin, I want the CLI to exit with a non-zero code on errors, so that scripts and deployment runbooks can detect failed provisioning.
15. As a Hamlet server admin, I want the CLI to print a concise success message, so that I can confirm which account was created.
16. As a Hamlet server admin, I want the success message to avoid printing the password, so that terminal logs do not leak temporary credentials unnecessarily.
17. As a Hamlet server admin, I want the created account to be able to log in through the normal client, so that no special admin-created-account path exists.
18. As a Hamlet server admin, I want the CLI to operate against persistent SQLite state, so that accounts survive server restarts.
19. As a Hamlet server admin, I want the CLI to initialize a new database consistently with server startup, so that first-run provisioning is possible.
20. As a Hamlet maintainer, I want automated tests for the CLI argument handling, so that future changes do not silently break operator workflows.
21. As a Hamlet maintainer, I want automated tests proving the CLI-created credential is hashed, so that password security does not regress.
22. As a Hamlet maintainer, I want automated tests proving duplicate-user handling is clear, so that operator error handling remains predictable.
23. As a Hamlet maintainer, I want the CLI implementation to stay small and server-side, so that it does not affect the Electron client or normal HTTP APIs.
24. As a Hamlet maintainer, I want account creation behavior centralized in a deep module, so that HTTP registration and admin provisioning share the same invariants.
25. As a Hamlet maintainer, I want operator-facing documentation for the command, so that hosting notes and support answers can point to one workflow.

## Implementation Decisions

- Build a server-side admin CLI binary rather than an unauthenticated HTTP endpoint. This keeps account creation available to operators with shell/database access and avoids exposing a new remote administrative attack surface.
- Keep the binary thin. It should parse command-line arguments, load server configuration, initialize database connectivity, call the existing auth/user creation primitive, and translate outcomes into operator-facing messages and exit codes.
- Reuse the existing password credential provider, Argon2 hashing, credential insertion, duplicate credential check, generated user ids, and per-user read-state baseline setup through the current registration primitive.
- Do not create a login session as part of admin provisioning. The invitee should authenticate normally with the temporary password.
- Do not seed development data, start the HTTP server, or modify channel bootstrap behavior from the admin command. The command's responsibility is account creation only.
- Use the same environment-driven database selection as the server: explicit database URL overrides win, otherwise the default local app data directory is used.
- Validate CLI arguments before database mutation. Username and password must both be present and not blank after trimming whitespace. The username passed to account creation should be the trimmed username; the password should be used exactly as provided after rejecting all-whitespace input.
- Print success without echoing the password.
- Map duplicate usernames to a clear conflict-style command failure instead of a generic internal error.
- Keep all behavior server-side; the Electron client does not require UI or API changes for this feature.

## Testing Decisions

- Good tests should exercise externally visible behavior: accepted syntax, rejected syntax, database effects, duplicate handling, and credential usability. They should avoid asserting internal parser implementation details beyond the public parse contract.
- Add server tests for CLI argument parsing: accepted `create-user --username <name> --password <password>` input, help requests, missing command, unknown flags, missing values, and blank values.
- Add server integration-style tests for account creation against an initialized SQLite database: successful account creation inserts a user/credential, stores a hashed password that authenticates through the existing login primitive, and creates read-state baselines through the shared registration path.
- Add a server test for duplicate usernames returning a clear duplicate-user error.
- Existing auth tests provide prior art for checking Argon2 hashes, duplicate username rejection, and login behavior.
- Existing database/test context helpers provide prior art for using initialized in-memory SQLite databases.
- Run Rust formatter, linter, and tests for the server side.

## Out of Scope

- A remote HTTP admin API.
- Role-based admin permissions or an in-app admin console.
- Password reset, forced password change, password expiry, or invite-link flows.
- Client UI changes for admin-created accounts.
- Email verification or email invite delivery.
- Managing existing users, deleting accounts, changing roles, or rotating passwords from the CLI.
- Storing or displaying temporary passwords after creation.
- Supporting databases other than SQLite.

## Further Notes

- Operators should be reminded to run the command with the same `DATABASE_URL` or `HAMLET_DATA_DIR` environment as the server.
- Docker/Compose deployments may need to run the command in an environment that can access the same persistent volume or database URL as the server container.
- Because SQLite is persistent by default, this workflow should be safe across server restarts and should not rely on development seed users.
