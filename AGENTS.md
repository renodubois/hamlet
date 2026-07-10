# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

**Hamlet** is a Discord-like chat app. It has two independent parts:

- `server/` — Rust/Actix-web HTTP API on `127.0.0.1:3030` with SQLite
- `client/` — Electron desktop app wrapping a SolidJS frontend

The server has its own `CLAUDE.md` with commands, architecture details, and gotchas. The Electron client keeps its usage and QA notes in `client/README.md`.

## Planning documents

Any sort of planning document should live in `docs/plans/`. Include the date and a short topic summary in the filename, for example `YYYY-MM-DD-short-topic-summary.md`.

## Testing expectations

New functionality should come with tests. The Electron client has a full test stack (Vitest unit/component/integration, MSW for HTTP, axe for accessibility, Playwright for renderer and Electron E2E) — see `client/README.md` for the available commands and QA notes.

Before marking any change as done, run the relevant checks for the side you touched:

- **`client/`** — `pnpm run fmt`, `pnpm run lint`, `pnpm run typecheck`, `pnpm run test`. Run `pnpm run test:e2e` if the change could affect a smoke-tested flow (login, sending messages, shell launch). Run `pnpm run size` if the change might affect bundle size.
- **`server/`** — follow `server/CLAUDE.md` for its test/format/lint commands.

A single `scripts/check.sh` runs the default checks both sides expose — fmt, lint, typecheck/clippy, tests, and (if installed) `cargo audit` — in the order CI would. `scripts/check.sh server` or `scripts/check.sh client` scopes to one side; `--fix` applies formatter fixes before running the rest; `--e2e` adds the Playwright E2E suite. Use E2E testing for features whenever it feels necessary for confidence; prefer more testing over less. Use it as a pre-push gate.

### Fix failing checks even if you didn't cause them

If any test, linter, type check, size budget, or other repo-level check fails while you're working — **fix it**, even if the failure predates your change and isn't in the files you were sent to edit. "Not my fault" is not a reason to leave the tree red. Track the fix alongside your main change (or as its own follow-up commit on the same branch), and explain in the commit message what the pre-existing breakage was. Flaky tests count: stabilize them, don't retry-until-green.

## Running the full app

Start both in separate terminals:

```bash
# Terminal 1 — backend
cd server && cargo run

# Terminal 2 — Electron app
cd client && pnpm run electron:dev
```

The frontend defaults to `http://127.0.0.1:3030`; this is configurable at login and stored in localStorage. The Electron renderer uses the fixed local origin `http://127.0.0.1:1422` in development and packaged modes.

### Docker Compose (server + LiveKit)

The server side also runs under Docker Compose, which brings up `server` and a self-hosted `livekit` container together. Compose is for the backend only — the Electron client still runs on the host with `pnpm run electron:dev`. Compose files live in `server/`, so run these from there:

```bash
cd server
docker compose up          # dev mode (default): cargo-watch, hot reload on src changes
docker compose up --build  # force rebuild after Cargo.toml / Dockerfile edits
docker compose down        # stop
docker compose down -v     # stop + wipe data (database + uploads) and cargo cache volumes
```

`docker-compose.override.yml` is picked up automatically and swaps the server image for `Dockerfile.dev`, bind-mounts `server/` into the container, explicitly enables development seed data, and runs `cargo watch -x run`. The server database, public uploads, and private uploads live together in the `hamlet_data` named volume at `/var/lib/hamlet`, so container replacement preserves them and `docker compose down -v` resets them together. Cargo's registry, git cache, and `target/` live in named volumes so rebuilds are incremental (first build is slow, subsequent edits recompile in seconds).

To run the production image instead (fresh multi-stage build, no hot reload):

```bash
cd server
docker compose -f docker-compose.yml up --build
```

LiveKit listens on `ws://127.0.0.1:7880` with dev credentials `devkey` / `devsecretdevsecretdevsecretdevsecret` (see `livekit.yaml`). In Compose, the LiveKit container uses host networking so it can auto-advertise a non-loopback ICE address that browsers such as Firefox can use for local WebRTC. UDP media ports `50000-50100` are controlled by `livekit.yaml`; widen that range if you hit port exhaustion.

The server binds to whatever `HAMLET_BIND_ADDR` is set to (default `127.0.0.1:3030` for `cargo run`; Compose sets it to `0.0.0.0:3030` so the port is reachable from the host). Persistent SQLite defaults, reset workflows, and migration expectations are documented in `docs/persistent-sqlite.md`.

## How the two parts connect

All application logic goes over HTTP. The frontend calls the server via the `client/src/api/` modules; normal Hamlet APIs do not go through Electron IPC.

Real-time messaging uses SSE: the server exposes `GET /messages/subscribe`, and each channel view opens an `EventSource` on mount that closes on cleanup. New messages are POSTed to `POST /message/{channel_id}`, which triggers a broadcast to all subscribers.

## Data model

SeaORM entities in `server/src/entity/`: `user`, `channel`, `message`, `credential`, `session`, attachments, embeds, custom emoji, and reactions. IDs are random 15-digit integers (not autoincrement; 15 digits stays inside JS `Number.MAX_SAFE_INTEGER` so the browser can round-trip them as JSON numbers). The default database is file-backed SQLite under the local application data directory (or `HAMLET_DATA_DIR`); explicit `DATABASE_URL` overrides can still use in-memory SQLite for tests or clean-room runs. Startup initializes schema, applies explicit migration/integrity steps, bootstraps the default `general` text channel and `voice` voice channel when the channel table is empty and `HAMLET_BOOTSTRAP_DEFAULT_CHANNELS` is enabled, and only seeds local development users (`baipas`/`password` and `teo`/`password`) when `HAMLET_SEED_DEV_DATA` is enabled. When dev seed data is enabled, a fixed `baipas` session token is printed to stdout.

When designing new features, assume data must survive server restarts — avoid patterns that rely on reset-on-restart behavior (e.g. hardcoded session tokens, seed-only data, schema changes that would be painful to migrate). Schema changes for persistent data should go through migration-shaped startup steps with tests against existing rows; do not rely on deleting local DBs or ad hoc schema sync alone.

## Auth

The server has register/login/logout/me endpoints using Argon2 password hashing and session cookies. All routes except register/login/logout require authentication via the `require_auth` middleware (`server/src/middleware.rs`). The `AuthUser` extractor is used by handlers that need the caller's identity (e.g. message creation). The frontend has a login page and auth context provider.
