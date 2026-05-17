# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

This is the Rust/Actix-web backend for **Hamlet**, a Discord-like chat app. It is a standalone git repo; the Electron/Solid.js frontend lives in a sibling `client-electron/` directory (see `../CLAUDE.md` for the combined workspace view). The server binds `127.0.0.1:3030` and the frontend expects it there.

## Commands

```bash
cargo build                                       # compile
cargo run                                         # run server on 127.0.0.1:3030
cargo test                                        # run all tests
cargo test test_message_create                    # run a single test by name
cargo test -- --nocapture                         # show println!/dbg! output during tests
RUST_LOG=debug cargo run                          # raise log verbosity (default is `info`)
HAMLET_BIND_ADDR=0.0.0.0:3030 cargo run           # override bind address (used by docker-compose)
```

### Docker

The server also runs under Docker Compose alongside a self-hosted LiveKit container. `docker-compose.yml`, `docker-compose.override.yml`, and `livekit.yaml` live in this directory, so run `docker compose up` from `server/`. See `../CLAUDE.md` for the full workflow. There are two Dockerfiles:

- `Dockerfile` — multi-stage production build using `cargo-chef` for dependency caching.
- `Dockerfile.dev` — dev image with `cargo-watch`; the override bind-mounts `server/` into the container so code changes trigger incremental rebuilds.

The bind address is controlled by `HAMLET_BIND_ADDR` (defaults to `127.0.0.1:3030` to preserve the local `cargo run` behavior; containers set it to `0.0.0.0:3030`).

### Code quality (run before committing)

```bash
cargo fmt                                         # format all code
cargo clippy -- -D warnings                       # lint; all warnings must be fixed
cargo audit                                       # check deps against RustSec advisory DB (requires: cargo install cargo-audit)
```

`cargo fmt` and `cargo clippy` use defaults (no `rustfmt.toml` yet). All clippy warnings must be resolved — use `-- -D warnings` to enforce this. There is no CI yet.

`Cargo.toml` enforces `unsafe_code = "forbid"`, `unwrap_used = "warn"`, and `expect_used = "warn"`. Test modules may add `#[allow(clippy::unwrap_used, clippy::expect_used)]` since `.unwrap()` is conventional in test assertions.

Rust edition 2024.

## Architecture

### Entry points
- `src/main.rs` — thin binary: loads `Config`, initializes `tracing`, connects to SQLite, creates the `Broadcaster`, seeds dev data, calls `start_server`.
- `src/lib.rs` — module declarations + a small public re-export surface. Each concern lives in its own module.
- `src/startup.rs` — `AppDeps` (the bag of `web::Data` every handler needs), `configure_app`, `start_server`. Tests build an `AppDeps` and call `configure_app` against `actix_web::test::init_service` without going through a real socket.
- `src/config.rs` — `Config::from_env`. The single source of truth for env-driven configuration; no other module reads `std::env`.
- `src/error.rs` — `AppError` (thiserror-based) with `#[from]` for `sea_orm::DbErr`, `serde_json::Error`, and `std::io::Error`. Its `ResponseError` impl returns a JSON body shaped `{ "error": { "kind": "...", "message": "..." } }`. Internal-error variants log the cause through `tracing::error!` before sanitizing the response.
- `src/telemetry.rs` — installs the `tracing` subscriber. `tracing_actix_web::TracingLogger` is wired in `start_server` so each request gets a span with a request id.
- `src/util.rs` — `generate_id`, `now_unix_secs`.
- `src/seed.rs` — `seed_development_data`, plus the placeholder avatar bytes for the dev user.
- `src/auth.rs` — session/credential primitives: `register_user`, `authenticate_password`, `create_session`, `validate_session`, `AuthUser` extractor, cookie helpers.
- `src/middleware.rs` — `require_auth`: actix-web `from_fn` middleware that validates the session cookie and injects `AuthUser` into request extensions.
- `src/api/` — HTTP handlers, one module per resource. Each owns its DTOs and exposes a `pub fn configure(cfg)` that registers its routes.
  - `api/auth.rs` — register/login/logout, `/me`, `/me` PUT.
  - `api/avatars.rs` — `/me/avatar` upload/delete + `AvatarStorage`.
  - `api/channels.rs` — list/create/reorder + `ChannelResponse`.
  - `api/messages.rs` — message CRUD, suppress-embeds, typing, SSE subscribe + `MessageResponse`/`EmbedResponse` + the `EmbedFetcher` switch + the embed-refresh background task.
  - `api/voice.rs` — token minting, participant listing, speaking relay, LiveKit webhook receiver. Uses primitives from `src/voice.rs`.

### Request path
Routes are registered by `startup::configure_app`. **Auth is required by default** — only register/login/logout (and the LiveKit webhook, which authenticates via signed JWT in the body) are public. Everything else is wrapped in a scope with `require_auth` middleware.

Public:
- `POST /register`               — create account, returns session cookie
- `POST /login`                  — authenticate, returns session cookie
- `POST /logout`                 — destroy session, clears cookie (public so clients can always recover from a stale cookie)

Require auth:
- `GET  /channels`                — list all channels
- `GET  /messages/{channel_id}`   — fetch history for a channel (400s if channel missing)
- `POST /message/{channel_id}`    — insert a message, broadcast via SSE; uses authenticated `user_id`
- `GET  /messages/subscribe`      — SSE stream; returns an `actix_sse::Sse` response backed by an mpsc receiver
- `GET  /me`                      — return the authenticated user's profile

`GET /messages/subscribe` must be registered before `GET /messages/{channel_id}` in `api::messages::configure` so actix-web's router doesn't match `/messages/subscribe` as a channel ID.

### Broadcasting (`src/broadcast.rs`)
`Broadcaster` holds `Mutex<Vec<mpsc::Sender<Event>>>` plus the `BroadcastEvent` taxonomy that consumers switch on. Three callable surfaces:
- `Broadcaster::new()` — bare instance, **no** ping loop. Tests use this so stale-client culling doesn't interfere.
- `Broadcaster::create()` — calls `new()` and spawns a 10s interval that sends a `ping` comment to every client; senders that fail to accept are dropped. This is what `main.rs` uses.
- `broadcaster.publish(&BroadcastEvent::X(...))` — handlers go through this. It serializes to JSON internally and surfaces any failure as `AppError::Json`. There's also a lower-level `subscribe()` returning the SSE responder.

Tests can attach a fake subscriber via `Broadcaster::test_client()` (gated on `#[cfg(test)]`) which returns the raw `mpsc::Receiver<Event>` rather than an HTTP response, so you can assert on broadcast payloads without an HTTP client.

### Persistence (`src/entity/`)
SeaORM 2.0-rc entities: `user`, `channel`, `message`. `message.channel_id` → `channel.id`, `message.user_id` → `user.id`. IDs are `i64` generated by `generate_id()` (random 15-digit int, not autoincrement or UUID; sized to fit JS `Number.MAX_SAFE_INTEGER` so the browser doesn't round-trip them as a different value).

The database is **`sqlite::memory:` in `main.rs`** — the `// TODO(reno): Using in-memory for now` comment reflects the current reality. Every restart wipes data, which is why `seed_development_data` runs unconditionally on startup: it calls `get_schema_registry(...).sync(db)` to create tables, inserts a `general` text channel and a `voice` voice channel, and registers two dev users (`baipas`/`password` with a fixed session token, and `teo`/`password` for multi-user testing). When swapping to a persistent DB, this seeding needs to be gated — the TODO in `main.rs` flags this.

The dev session token is printed to stdout on startup (`=== DEV: baipas session active — set cookie: session=devdev... ===`). Paste it into browser devtools to skip the login flow during development.

### Tests
Integration tests are split per resource:
- `tests/auth.rs` — register/login/logout, `/me`, `update_me`, expired session, auth-gating.
- `tests/channels.rs` — channel CRUD + reordering.
- `tests/messages.rs` — message CRUD, suppress_embeds, embed surfacing.
- `tests/avatar.rs` — multipart upload, delete, cross-resource avatar carry.
- `tests/common/mod.rs` — `TestCtx::new()` / `TestCtx::with_avatar_storage()` builder, `AuthSession` with `cookie_header()` helper, `setup_db`, `insert_message`. Each test is normally three lines: `let ctx = TestCtx::new().await; let alice = ctx.register("alice", "pw").await; let app = test::init_service(...).await;`.

A handful of unit tests stay in `src/api/messages.rs` and `src/api/voice.rs` because they need `Broadcaster::test_client()` — that's `#[cfg(test)]`-gated and therefore invisible from the separate integration test crate in `tests/`.

## Gotchas

- `Broadcaster::create` vs `new`: if a test uses `create`, the 10s ping task keeps running and can race with assertions — use `new` in tests.
- CORS in `start_server` allows **any** `http://localhost` / `http://127.0.0.1` origin with any method/header. Fine for dev, dangerous in prod (flagged with a `NOTE(reno)` in the code).
- Error responses are JSON-shaped (`AppError`) — extend the `AppError` enum rather than introducing a new ad-hoc error type. Internal variants (`Db`, `Json`, `Io`, `Internal`) are auto-derived via `#[from]` and log through `tracing::error!` before sanitizing the response.
- `Config::from_env` is the only place env vars are read. If you need a new tunable, add a field to `Config` rather than calling `std::env::var` from a handler.
- The `.env` file is gitignored and currently not auto-loaded.
