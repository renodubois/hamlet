# Hamlet HTTP server

The server has open signup and every authenticated user can access every channel. Do not expose it to the network without revisiting admission, TLS termination, and rate limiting.

Code layout: `src/main.rs` handles process startup; `src/lib.rs` owns reusable app state, database initialization, and top-level route wiring. Each feature (`auth/`, `channels/`, `messages/`) keeps HTTP handlers, request/response types, and database-backed operations in `handlers.rs`, `types.rs`, and `operations.rs`. Feature `mod.rs` files register routes where appropriate. Shared bearer middleware, request logging, and error responses live in `http/`; `bootstrap.rs` seeds the starter channel. `contract.rs` assembles OpenAPI. The feature operations still use raw SQL; entity mappings are a separate future change.

Run from **`server/`** (paths below are relative to this directory):

```sh
cargo run --locked --bin hamlet
```

The default database is `data/hamlet.db` (created on startup, Git-ignored); the default bind is `127.0.0.1:3001`. The desktop client defaults to port 8081; set `HAMLET_BIND=127.0.0.1:8081` or change its server URL to match. Override with `HAMLET_DATABASE_URL=sqlite:///absolute/path?mode=rwc` and `HAMLET_BIND=127.0.0.1:9000`. Migrations and empty-channel bootstrap run at startup and fail startup on errors. A file database uses WAL, foreign keys, a five-connection pool, and a five-second busy timeout; HTTPS is expected at an external reverse proxy.

Verification:

```sh
cargo fmt --check
cargo fmt --manifest-path migration/Cargo.toml --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked --quiet --bin generate-openapi > openapi.json  # regenerate after route/DTO changes
cargo test --locked --test contract                         # checks artifact drift and inventory
```

The full `cargo test --locked` suite also checks contract drift. The generated `openapi.json` is a checked-in artifact, **not** a runtime endpoint. The `#[utoipa::path]` annotations beside handlers and the `ToSchema` derives on request/response types feed `contract::ApiDoc`; `tests/contract.rs` inventories Actix registrations against documented methods/paths, status/code pairs, bearer scopes, and representative HTTP behavior. A snapshot alone cannot prove annotations reflect actual responses: feature HTTP tests separately exercise success and error paths.

The `utoipa-actix-web` integration was evaluated: its automatic path collection currently supports annotated `.service(handler)` registration, not the explicit `web::resource(...).route(web::method().to(handler))` configuration used here for protected scope and uniform 405 handling. We use `utoipa` directly plus the explicit inventory test rather than silently omitting routes. If route registration changes, update both the OpenAPI path assembly and inventory test. Do not add a docs-serving route.

Message-history cursors are unsigned, opaque ordering positions; clients should only pass a returned `next_cursor` back as `before`, not parse or construct one. A cursor is not authorization: every history request still checks access to the requested channel. Tokens and passwords are never logged. Request tracing records request ID, method, path, and status, not Authorization or payloads. See [ARCHITECTURE.md](ARCHITECTURE.md) for extension-cost review and remaining seams.
