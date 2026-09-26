# Hamlet server rewrite: architecture prototype

Date: 2026-09-23

> Historical prototype plan. For the current implemented file layout and contributor guidance, see [`server-rewrite/README.md`](../../server-rewrite/README.md) and [`server-rewrite/AGENTS.md`](../../server-rewrite/AGENTS.md). The layout below was illustrative, not a description of the present tree.

## Goal and boundaries

Build a **new**, HTTP-only Actix API under `server-rewrite/`. It does not need API, schema, or data compatibility with `server/`. This is a small, working prototype to validate module boundaries, testability, human readability, extension costs, and adequate performance for a **small self-hosted community**. Do not move existing clients to it as part of this phase. Treat this as a replacement candidate, not a committed migration: preserve awareness of domain differences from the current server without requiring compatibility in the prototype.

Implement account signup/login/logout, current-user lookup, bearer authentication, channel creation/listing, and text-message creation/history. Use SeaORM with SQLite. Defer SSE, roles/memberships, user roster, editing/deletion, attachments, cookies/CSRF, CORS, rate limiting, refresh tokens, and production monitoring. The future SSE delivery/replay guarantee is **unresolved**; do not build an event table, publisher interface, or subscription endpoint yet.

**Exit criterion:** HTTP integration tests cover signup → authenticated channel operations → message creation and pagination, plus failure paths. Review concretely how a new channel type, a new auth rule, and eventual SSE publication would be introduced, without implementing these extensions now.

## Architecture

- Organize by feature: `auth/`, `channels/`, `messages/` each own their HTTP handlers, request/response DTOs, and application operations. Shared `http/` owns routing/auth middleware/error mapping/OpenAPI assembly; `db/` owns connection setup and handwritten SeaORM entities; `config.rs` owns environment configuration; `bootstrap.rs` creates the initial channel. Split a feature into more files only where it improves navigation.
- Keep handlers thin: Actix extraction and response construction belong there. Feature services implement use cases, accept explicit inputs and database dependencies, and **do not depend on Actix types**. Use small, typed feature errors mapped in one HTTP-boundary location to status codes and a common error body. Do not introduce repository traits just to forward SeaORM calls; services can use SeaORM directly. Add persistence abstractions when a real query or testing need warrants them.
- Use explicit API DTOs, not serialized SeaORM entities. Route registration and OpenAPI metadata should be defined together as closely as tooling permits; avoid maintaining unrelated, duplicate route lists.
- Register signup/login as public routes and the rest in an Actix **protected scope**. Its bearer middleware resolves the session once and makes an authenticated identity available to handlers. This prevents newly added protected endpoints from accidentally omitting an auth extractor. `POST /auth/logout` belongs in the protected scope.
- A single `AppState` can hold the SeaORM connection pool and relevant configuration. Build the app from injected state so Actix test services use the same route registration as production. Pure normalization/validation logic is unit-testable; the main proof comes from real HTTP tests with isolated SQLite databases.
- Keep DB writes and returned results inside message services. Once SSE semantics are decided, add post-commit publishing at an explicit orchestration boundary, or a transactional outbox if replay/reliability requires it. Do not imply that a best-effort in-process event is durably delivered.

Illustrative layout, not a mandate for one-file-per-concept:

```text
server-rewrite/
  Cargo.toml                 # app crate + workspace including migration
  .gitignore                 # data/ and build output
  openapi.json               # checked-in, generated contract
  src/
    main.rs                  # config, tracing, DB connection, migrate, bootstrap, serve
    lib.rs
    app.rs                   # shared state and route configuration
    config.rs
    bootstrap.rs
    http/                    # bearer middleware, error mapping, OpenAPI assembly
    auth/                    # handlers, DTOs, session/account operations
    channels/                # handlers, DTOs, operations
    messages/                # handlers, DTOs, operations
    db/entities/             # user, session, channel, message
  migration/                 # separate SeaORM migration crate
  tests/                     # HTTP tests and an isolated SQLite test harness
  data/                      # ignored local database, created on first run
```

Use compatible **stable SeaORM 2.x** and `sea-orm-migration` releases. The current server's RC was needed for 2.x features before stable 2.0; it is not a constraint on this independent rewrite. Use only the SQLite/runtime/entity features required. Because we chose versioned migrations, do not enable or run entity-first `schema-sync`/`entity-registry` unless a later decision changes the schema strategy. Reference: [SeaORM 2 stable release](https://github.com/SeaQL/sea-orm/releases/tag/2.0.0) and [migration setup](https://www.sea-ql.org/SeaORM/docs/migration/setting-up-migration/).

## Persistence and startup

- Independent SQLite database: default to `server-rewrite/data/hamlet.db` for development (directory gitignored), with a database URL override. Resolve the local default predictably and document the working-directory expectation; never fall back to the legacy server's database. Bind to `127.0.0.1` by default with an address override for deployments.
- Define versioned schema migrations in `migration/`. Apply them at startup and in test setup; fail startup clearly on failure. Handwrite the small set of app SeaORM entities, keeping migrations as the schema source of truth and exercising the entity mappings against migrated databases in tests.
- Tables: `users` (15-digit integer ID, preserved username, lowercase unique username key, password hash); `sessions` (token digest unique, user ID, expiry); `channels` (ID, trimmed name, lowercase unique name key, type); `messages` (ID, channel ID, author ID, text, creation timestamp). Use foreign keys and indexes, especially `(channel_id, created_at DESC, id DESC)` for history. No user memberships or per-channel permissions: all authenticated users can list/read/post in all channels.
- Store creation timestamps at sufficient precision to order concurrent messages; present times in the API as UTC RFC 3339 strings. Order message history deterministically by `(created_at DESC, id DESC)`, **not by ID alone**.
- IDs are random **15-digit positive SQLite integers**, returned as **decimal strings** in JSON and path parameters, without leading zeroes. Centralize generation; on a unique-key collision retry the insert a bounded number of times, distinguishing that from other DB errors. Random IDs are not secrets or time-ordered. Keep user/channel/message IDs consistent across all DTOs and OpenAPI schemas.
- After migration, bootstrap one `general` text channel **only if the channels table is empty**. It is an ordinary starter channel, not a permanent or privileged channel. Do not seed demo accounts. Keep bootstrap separate from schema migrations and make it safe to run repeatedly. If channel deletion is added later, revisit bootstrap so an intentionally empty community does not unexpectedly regain `general` on restart.
- For a small single-process deployment, configure a modest SQLite pool, busy timeout, foreign keys, and WAL mode for file-backed DBs; measure before adding caching or storage layers. Password hashing is CPU-intensive: perform Argon2id hashing/verification off Actix's async worker threads.

## HTTP contract

All paths below have `/api/v1` as prefix. Signup and login are public; every other route requires `Authorization: Bearer <access_token>`. The checked-in generated OpenAPI document is the language-agnostic contract; do **not** add a runtime documentation endpoint for now.

| Method and path | Request | Success |
| --- | --- | --- |
| `POST /auth/signup` | `{ "username": string, "password": string }` | `201` auth response |
| `POST /auth/login` | same | `200` auth response |
| `POST /auth/logout` | bearer token; no body | `204` empty; revokes only this session |
| `GET /me` | bearer token | `200` user |
| `POST /channels` | `{ "name": string, "type": "text" }`; type required | `201` channel |
| `GET /channels` | none | `200` `{ "items": [channel, ...] }`, sorted by case-insensitive name, then ID |
| `GET /channels/{channel_id}/messages` | optional `limit` (default 50, 1–100) and opaque `before` cursor | `200` `{ "items": [message, ...], "next_cursor": string \| null }` |
| `POST /channels/{channel_id}/messages` | `{ "text": string }` | `201` message, same shape as a history item |

- `user`: `{ "id": "<decimal>", "username": "..." }`. Both signup and login return `{ "user": user, "access_token": "...", "expires_at": "<RFC3339 UTC>" }`.
- `channel`: `{ "id": "<decimal>", "name": "...", "type": "text" }`. No channel creation timestamp in the API DTO. Only `text` is accepted for now; represent it as an enum in the contract so adding types later is deliberate.
- `message`: `{ "id": "<decimal>", "channel_id": "<decimal>", "author": { "id": "<decimal>", "display_name": "..." }, "text": "...", "created_at": "<RFC3339 UTC>" }`. Store **author ID only** on the message; obtain the current display name at read time via a join/batched lookup, never one user query per message. Initially the display name is the account username; this leaves room for channel-specific nicknames later. `/users` is not needed yet.
- History returns the latest page **newest-first**. `before` is an opaque cursor representing a `(created_at, id)` ordering position, not a message reference: it need not identify a message in the requested channel, and a cursor from another channel may be used as a position. Clients are promised only that they can pass back a returned `next_cursor`; they must not rely on its encoding or construct one. Cursors are not signed or proof of server issuance; existing signed cursors need not remain valid after this change. Query one extra row to determine whether to emit `next_cursor`; null means there are no older messages. A missing channel returns 404, not an empty page. Reject malformed cursors as invalid input. Authorization must be checked on every history request independently of the cursor.
- Username: 3–32 ASCII letters/digits/underscores/periods; unique case-insensitively, while preserving entered case for display. Channel name: trim surrounding whitespace; 1–64 ASCII letters/digits/spaces/hyphens/underscores after trimming; unique case-insensitively. Password: 8–256 **bytes**; hash with Argon2id, no composition policy. Message: non-empty after trimming, at most 4,000 Unicode characters; otherwise preserve entered text. Reject unknown request JSON fields to catch typos.
- Return one error envelope: `{ "error": { "code": "machine_readable_code", "message": "human-readable text" } }`. Use 400 for malformed/invalid requests and unsupported type/cursor, 401 for missing/invalid/expired bearer token or invalid login credentials, 404 for missing channels, 409 for duplicate usernames or channel names, and sanitized 500 for internal failures. Document codes/statuses in OpenAPI. Configure Actix JSON extraction and API fallbacks so framework-level errors do not silently become a different response shape. Do not reveal whether a username exists during login.

## Authentication and future browser concerns

- Public signup creates the account **and** a session; login creates a new session without invalidating other devices. Open signup plus access to every channel is a **prototype-only shortcut**: do not deploy it as the default for a network-exposed replacement without first deciding admission and channel-access policy. Sessions expire **30 days from login**, without refresh or sliding renewal. `/me` validates a stored token and returns the current user but does not extend its expiry. Clients re-login on expiry or 401.
- Generate high-entropy opaque bearer tokens, return the raw token only at creation, and store only a cryptographic digest in SQLite. Never log passwords, bearer headers, or raw tokens. Logout removes/revokes only the calling session. Keep token parsing, lookup, expiry, and user injection in shared auth code.
- Bearer tokens sent explicitly in `Authorization` are not ambient browser cookies; conventional cookie-based CSRF protection is not needed here. If cookie authentication is added later, introduce CSRF policy at the HTTP/auth boundary and update the API contract/tests rather than spreading it through feature services. Defer CORS until a browser-origin client needs it; a native HTTP client does not require CORS. Use HTTPS termination for network-exposed deployments; no TLS server setup in this prototype.
- Use `tracing` for structured request/error logs with request IDs; defer metrics, Sentry, and dashboards.

## OpenAPI approach and synchronization

OpenAPI is appropriate for a language-agnostic description of paths, parameters, bearer security, DTO schemas, error responses, and status codes. It supports documentation and potential client generation. Its limitations: it does not prove business semantics, database behavior, authorization placement, or that a handwritten handler really emits the annotated response. Generated clients also differ in quality across languages.

Use a Rust-first generator compatible with Actix (evaluate `utoipa` and its Actix integration when implementing). Derive DTO schemas and annotate/register routes near their handlers; generate and check in `server-rewrite/openapi.json`. Add a deterministic regeneration/check command in CI or `cargo test` that fails on a stale artifact, and HTTP integration tests for documented success/error cases and auth gating. A snapshot comparison alone **cannot** detect an undocumented route or a misleading annotation: keep route registration close to spec registration and add a route-coverage check or explicit API inventory test. Never treat generated documentation as conformance testing by itself. Tradeoff versus spec-first: less duplicated editing for this small Rust prototype, but the Rust implementation leads the contract; adopting a spec-first workflow later would require independent validation or generated server/client bindings.

## Tests and milestones

Use isolated temporary SQLite databases with the real migrator and Actix test server; avoid a shared development DB and be careful that an SQLite in-memory pool does not accidentally open distinct databases per connection. Focus on:

1. **Foundation:** independent workspace, config, tracing, connection/migration/bootstrap, common HTTP error mapping, runnable empty API; startup migration and idempotent `general` tests.
2. **Auth:** signup/login/logout/`me`, lowercase username uniqueness, Argon2id, token digest, fixed expiry, multiple sessions, missing/invalid/expired-token gating and sanitized errors.
3. **Channels:** required text type, validation/duplicates, shared protected listing, deterministic name order, response DTOs.
4. **Messages:** create/read, author display-name resolution, missing-channel errors, newest-first tuple ordering, limit bounds, older-page cursor, equal-timestamp tie handling, no N+1 author queries.
5. **Contract and review:** generated OpenAPI artifact + drift check, route/response inventory tests, `cargo fmt`, `cargo clippy`, `cargo test`; document commands in a local README. Walk through how channel variants, auth policy, cookie/CSRF, and SSE would enter the existing modules; record any boundary that proved awkward.

## Decisions deferred

- SSE event types, whether delivery is best-effort or replayable, resume cursors, and whether an outbox is required. Decide these **before** adding an event publisher or subscription route.
- Invitations, roles, permissions, multi-instance deployment, user roster, nicknames, message edits/deletion, browser CORS/cookie auth, rate limits, and operational hardening. The prototype should not claim production readiness.
