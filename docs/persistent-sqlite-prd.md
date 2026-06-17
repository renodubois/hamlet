# Persistent SQLite PRD

## Problem Statement

Hamlet currently behaves like a persistent chat app but stores server data in an in-memory SQLite database during normal local startup. Every restart wipes users, sessions, channels, messages, attachments metadata, reactions, embeds, and emoji records. Schema creation is coupled to development seeding, tests bypass the real startup path by syncing entities directly, and Docker persists uploads but not the database. This makes local development convenient but prevents realistic restart behavior, hides migration risk, and can leave stored files out of sync with database metadata once persistence is added.

## Solution

Make persistent SQLite the server default while preserving fast local development. Hamlet should connect to a file-backed SQLite database by default, create its parent directory automatically, run idempotent schema migrations on every startup, and then run only the configured bootstrap and development seed steps. Development seed data should be safe to run repeatedly against an existing database, and production/release defaults should avoid creating development users unless explicitly enabled. Docker should persist the database in a named volume alongside upload volumes. Tests should exercise the same database initialization path used at runtime, including in-memory and file-backed SQLite cases.

## User Stories

1. As a Hamlet user, I want my account to remain after the server restarts, so that I do not need to register again.
2. As a Hamlet user, I want my login session to remain valid after a restart, so that normal restarts do not sign me out unexpectedly.
3. As a Hamlet user, I want channels to remain after a restart, so that the workspace structure is stable.
4. As a Hamlet user, I want messages to remain after a restart, so that conversation history is durable.
5. As a Hamlet user, I want message replies, threads, embeds, attachments, reactions, and emoji metadata to remain after a restart, so that all chat features behave consistently.
6. As a Hamlet user, I want avatar and attachment database references to survive restarts, so that uploaded files still appear when the server comes back.
7. As a local developer, I want `cargo run` to use a persistent SQLite file by default, so that manual testing matches real app behavior.
8. As a local developer, I want an easy way to reset local state, so that I can still return to a clean dev environment.
9. As a local developer, I want default channels to exist in a fresh database, so that the app is usable immediately.
10. As a local developer, I want development users to be optional and clearly gated, so that convenience fixtures do not leak into production-like runs.
11. As a local developer, I want development seeding to be idempotent, so that repeated restarts do not fail or create duplicates.
12. As a local developer, I want the fixed development session token to keep pointing at the existing dev user, so that quick-login workflows remain convenient.
13. As a local developer, I want an existing custom dev avatar to be preserved, so that seed code does not overwrite manual changes unnecessarily.
14. As a server operator, I want schema migrations to run automatically on startup, so that empty database files are initialized safely.
15. As a server operator, I want startup against an existing database to be safe, so that restarting the server does not recreate or corrupt schema.
16. As a server operator, I want clear errors when existing data violates new uniqueness constraints, so that I can remediate instead of losing data silently.
17. As a server operator, I want file-backed SQLite parent directories to be created automatically, so that configuration with a new path works without manual setup.
18. As a server operator, I want SQLite busy handling configured, so that normal concurrent requests are less likely to fail with transient lock errors.
19. As a server operator, I want SQLite foreign keys enabled, so that persisted relational data remains consistent.
20. As a server operator, I want WAL mode used where practical, so that SQLite behaves better for the server workload.
21. As a Docker user, I want the database stored in a Docker volume, so that container replacement does not erase chat data.
22. As a Docker user, I want `docker compose down -v` to reset database and uploads together, so that full environment cleanup remains simple.
23. As a Docker user, I want development Compose to keep seeding dev data, so that containerized development remains convenient.
24. As a release builder, I want release defaults to avoid development users, so that production-shaped images do not create test credentials by default.
25. As a release builder, I want explicit environment variables for database and seed behavior, so that deployment behavior is auditable.
26. As an API client, I want HTTP contracts to remain unchanged, so that persistence is transparent to the Electron client.
27. As an API client, I want existing auth cookies and session validation behavior to continue working, so that persistence does not change login flows.
28. As a backend developer, I want database schema initialization separated from seed data, so that migrations and fixtures can evolve independently.
29. As a backend developer, I want a single initialization function for runtime and tests, so that tests cover the real schema path.
30. As a backend developer, I want test fixtures to use named in-memory SQLite databases, so that tests remain isolated and fast.
31. As a backend developer, I want in-memory SQLite sentinel-pool behavior preserved, so that tests do not see disappearing tables.
32. As a backend developer, I want file-backed SQLite to use normal pool behavior, so that production-like databases are not forced into test-only settings.
33. As a backend developer, I want migrations to create all current Hamlet tables, so that no entity depends on legacy schema sync.
34. As a backend developer, I want migration-managed unique constraints for credentials, reactions, and active emoji names, so that duplicate-sensitive domain rules are enforced by the database.
35. As a backend developer, I want read-path indexes for messages, threads, sessions, attachments, embeds, and reactions, so that persistence does not make common queries unnecessarily slow.
36. As a backend developer, I want duplicate reaction rows deduplicated deterministically before adding the unique reaction index, so that benign old duplication can be repaired safely.
37. As a backend developer, I want duplicate credentials and active emoji names to fail migration clearly, so that user identity and emoji ownership are not guessed incorrectly.
38. As a backend developer, I want startup errors to propagate through normal error handling, so that database failures are visible and do not panic obscurely.
39. As a backend developer, I want seed functions to return typed errors, so that callers and tests can assert failure paths.
40. As a backend developer, I want local database URL parsing tested, so that SQLite URL forms keep working as configuration evolves.
41. As a backend developer, I want data persistence across reconnects tested, so that the default file-backed behavior is protected from regression.
42. As a backend developer, I want migrations tested for idempotency, so that repeated startup remains safe.
43. As a backend developer, I want development seeding tested for idempotency, so that duplicate channels, users, and sessions do not return.
44. As a backend developer, I want seed flag parsing tested, so that debug, release, and explicit environment behavior are predictable.
45. As a tester, I want tests to stop using direct schema sync, so that migration drift is caught by the test suite.
46. As a tester, I want existing auth, channel, message, voice, emoji, reaction, avatar, and attachment tests to keep passing, so that persistence does not regress features.
47. As a tester, I want a manual restart smoke test, so that users, channels, messages, sessions, avatars, emoji, and attachments can be verified after restart.
48. As a tester, I want a Docker restart smoke test, so that volume-backed persistence works outside `cargo run`.
49. As a maintainer, I want documentation to state that the database is persistent by default, so that future contributors do not assume restart resets state.
50. As a maintainer, I want reset instructions documented, so that local cleanup remains discoverable.
51. As a maintainer, I want migration behavior documented, so that schema changes go through migrations rather than ad hoc sync.
52. As a maintainer, I want seed behavior documented, so that default channels and development users are not confused.
53. As a maintainer, I want environment examples to include the database URL and seed flag, so that new setups start with correct configuration.
54. As a maintainer, I want local SQLite files ignored by git and Docker build context, so that persistent state is not committed or copied into images.
55. As a future feature developer, I want persistence to be established before more schema-heavy features land, so that later work can rely on migrations.
56. As a future feature developer, I want upload metadata and files considered together, so that later cleanup work can reconcile missing files safely.
57. As a future operator, I want this change to be server-only, so that the desktop client can keep using normal HTTP and SSE APIs.
58. As a future operator, I want any unsupported existing database state to fail loudly, so that migration does not silently choose destructive repairs.

## Implementation Decisions

- Treat persistent SQLite as a server-only change. The Electron client, HTTP routes, SSE event names, and request/response contracts should remain unchanged.
- Add a migration module as a deep module that owns schema creation and schema evolution behind a small initialization interface.
- The database initialization interface should expose a function that runs all migrations for an existing connection. A combined connect-and-initialize helper may be added if it keeps startup and tests clearer.
- Runtime startup should connect to the database, run database initialization, run default-channel bootstrap when enabled, run development seed data only when enabled, and only then start accepting HTTP traffic.
- Schema initialization must be removed from development seeding. Seed functions should assume the schema already exists.
- The initial migration should create the current Hamlet tables for users, credentials, sessions, channels, messages, embeds, message attachments, message reactions, and custom emoji.
- The initial migration should use idempotent table and index creation where possible so databases previously created by entity sync can be baselined.
- The migration should enforce unique credentials by provider and external identifier.
- The migration should enforce one reaction per message, user, and emoji key.
- The migration should enforce unique active custom emoji names with a partial index over non-deleted emoji.
- The migration should add read-path indexes for channel history, thread lookups, reply/message relationships, session validation, attachment loading, embed loading, and reaction aggregation.
- Before adding duplicate-sensitive unique indexes, migration logic should repair only cases with a deterministic safe policy. Duplicate reactions can be deduplicated by keeping the oldest row; duplicate credentials and active emoji names should produce a clear migration error.
- The configuration module should remain the only module that reads environment variables.
- The default database URL should become a file-backed SQLite database under a local data directory, with environment override preserved.
- Add a development-seed flag with debug builds defaulting to enabled and release builds defaulting to disabled unless explicitly configured.
- Add a default-channel bootstrap flag if needed to let fresh production-like databases get `general` and `voice` without also creating development users.
- Boolean environment parsing should be explicit and tested for common true/false values and fallback behavior.
- The database module should own SQLite URL handling, including in-memory detection, file-path extraction, parent-directory creation, and connection option tuning.
- In-memory SQLite URLs should keep the sentinel connection behavior that prevents named in-memory databases from disappearing during tests.
- File-backed SQLite URLs should create parent directories before connecting and use practical SQLite options such as busy timeout, foreign keys, and WAL where supported.
- SQLite URL helpers should handle the URL forms currently used by Hamlet tests and proposed runtime defaults.
- Development seeding should be split into default-channel bootstrap and development-user/session seeding.
- Default-channel bootstrap should create `general` and `voice` only when missing, or when the channel table is empty, to avoid duplicating customized channel lists.
- Development seeding should find existing password credentials before registering dev users, rather than blindly inserting users.
- Development seeding should upsert or replace the fixed development session token so it points at the existing `baipas` user and has a fresh expiration.
- Development seeding should write the placeholder avatar only when the dev user has no avatar metadata or when the referenced seeded file is missing.
- Seed functions should return typed application errors instead of panicking or unwrapping.
- Startup should convert database, migration, and seed failures into normal process startup errors with useful logging.
- Public library exports should include the new database initialization and seed entry points needed by integration tests.
- Test fixtures should use the same database initialization entry point as runtime instead of direct entity schema sync.
- Existing named in-memory test databases should remain isolated with unique names.
- Docker production Compose should set the file-backed database URL inside the application data directory and mount that directory as a named volume.
- Docker development Compose should explicitly enable development seed data.
- The runtime image should create and own data, upload, and private-upload directories before dropping privileges.
- Local SQLite database files, SQLite sidecar files, and the local data directory should be ignored by git and excluded from Docker build context.
- Documentation should describe persistent defaults, startup migrations, seed gating, local reset, and Docker volume reset.
- Because the user is AFK, the assumed deep modules to prioritize are the migration runner, SQLite URL/path preparation helper, and idempotent seed/bootstrap layer. These modules should receive focused tests.

## Testing Decisions

- Good tests should assert externally visible behavior and stable module contracts: data remains after reconnect, migrations can run repeatedly, seed runs do not duplicate user-visible rows, configuration flags parse correctly, and startup fails clearly for unsupported duplicate state. Tests should not depend on private SQL formatting, internal helper names, or exact query ordering.
- Migration tests should cover fresh database initialization, idempotent second initialization, and all current tables being usable through SeaORM entities.
- Migration tests should cover the duplicate-reaction cleanup policy and clear failure behavior for duplicate credentials or active emoji names where feasible.
- Database helper tests should cover default file-backed URL handling, parent-directory creation, in-memory URL detection, and sentinel connection options.
- Persistence tests should write data to a temporary file-backed SQLite database, reconnect, initialize again, and verify the data remains.
- Configuration tests should cover database URL override, development-seed defaults, explicit seed flag parsing, and default-channel bootstrap flag parsing if added.
- Seed tests should run default-channel bootstrap twice and assert no duplicate `general` or `voice` channels are created.
- Seed tests should run development seed twice and assert no duplicate dev users, password credentials, channels, or fixed sessions are created.
- Seed tests should assert the fixed development session validates as the existing `baipas` user.
- Seed tests should assert an existing custom avatar is not overwritten by the placeholder avatar path.
- Integration test fixtures should be updated to call the real initialization function for named in-memory SQLite databases.
- Message API unit fixtures that need a broadcaster test client should use the same initialized in-memory database pattern.
- Voice API unit fixtures that need a broadcaster test client should use the same initialized in-memory database pattern.
- Existing server integration suites for auth, channels, messages, avatars, attachments, emoji, reactions, and voice should continue to pass without client changes.
- Prior art for integration tests includes the existing server test context builder, resource-specific integration tests, and unit tests that use named in-memory SQLite with a quiet broadcaster.
- Relevant implementation checks are server formatting, clippy with warnings denied, the full server test suite, and the repository server check script.
- Manual smoke testing should verify `cargo run`, register/login, message creation, restart persistence, avatar/emoji/attachment access after restart, Docker Compose persistence across container restarts, and Docker volume reset.

## Out of Scope

- Replacing SQLite with Postgres or another database.
- Adding user-facing backup, restore, import, or export tooling.
- Building an admin UI for migrations or database health.
- Changing HTTP API contracts, SSE event contracts, auth cookie shape, or Electron IPC.
- Changing how message, reaction, emoji, avatar, attachment, embed, thread, or voice features behave beyond making their data persistent.
- Designing a complete upload garbage-collection or metadata reconciliation system.
- Recovering every possible manually edited or corrupted SQLite database automatically.
- Introducing production account bootstrap flows beyond optional default channels and explicit development seed data.
- Encrypting the SQLite database at rest.
- Committing generated database files, Docker volumes, or local test artifacts.
- Implementing code as part of this PRD.
- Publishing issues or labels from this PRD.

## Further Notes

- The user explicitly requested the full PRD workflow while AFK, so module and test confirmation is recorded here as assumptions rather than gathered interactively.
- SeaORM migration dependency versions should match the current SeaORM release candidate used by Hamlet.
- Existing database files created by older schema-sync behavior may need baselining and duplicate checks before unique indexes are added.
- Persistent database metadata can outlive missing upload files; this PRD preserves that risk for a later reconciliation/cleanup feature.
- SQLite PRAGMAs are connection-sensitive, so durable connection-option support is preferred over one-time setup that may not apply to future pooled connections.
