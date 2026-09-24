# Hamlet HTTP server prototype

This is a **prototype**, not a migration or a production-ready replacement for `server/`. It has open signup and every authenticated user can access every channel. Do not expose it to the network without revisiting admission, TLS termination, and rate limiting.

Run from **`server-rewrite/`** (paths below are relative to this directory):

```sh
cargo run --bin hamlet
```

The default database is `data/hamlet.db` (created on startup, Git-ignored); the default bind is `127.0.0.1:8081`. Override with `HAMLET_DATABASE_URL=sqlite:///absolute/path?mode=rwc` and `HAMLET_BIND=127.0.0.1:9000`. Migrations and empty-channel bootstrap run at startup and fail startup on errors. A file database uses WAL, foreign keys, a five-connection pool, and a five-second busy timeout; HTTPS is expected at an external reverse proxy. No legacy storage or data is read.

Verification:

```sh
cargo fmt --check
cargo fmt --manifest-path migration/Cargo.toml --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run --quiet --bin generate-openapi > openapi.json  # regenerate after route/DTO changes
cargo test --test contract                         # checks artifact drift and inventory
```

The full `cargo test` suite also checks contract drift. CI can use the same commands. The generated `openapi.json` is a checked-in artifact, **not** a runtime endpoint. The `#[utoipa::path]` annotations beside handlers and the DTO `ToSchema` derives feed `contract::ApiDoc`; `tests/contract.rs` inventories Actix registrations against documented methods/paths, status/code pairs, bearer scopes, and representative HTTP behavior. A snapshot alone cannot prove annotations reflect actual responses: feature HTTP tests separately exercise success and error paths.

The `utoipa-actix-web` integration was evaluated: its automatic path collection currently supports annotated `.service(handler)` registration, not the explicit `web::resource(...).route(web::method().to(handler))` configuration used here for protected scope and uniform 405 handling. We use `utoipa` directly plus the explicit inventory test rather than silently omitting routes. If route registration changes, update both the OpenAPI path assembly and inventory test. Do not add a docs-serving route.

Cursor tokens are signed with a high-entropy key persisted in the database; keep that database/private key private. Tokens and passwords are never logged. Request tracing records request ID, method, path, and status, not Authorization or payloads. See [ARCHITECTURE.md](ARCHITECTURE.md) for extension-cost review and remaining seams.
