# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

**Hamlet** is a Discord-like chat app. It has two independent parts:

- `server/` — Rust/Actix-web HTTP API on `127.0.0.1:3030` with SQLite
- `client/` — Tauri v2 desktop app wrapping a SolidJS frontend

Each subdirectory has its own `CLAUDE.md` with commands, architecture details, and gotchas. Start there for work scoped to one side.

## Testing expectations

New functionality should come with tests. The client side has a full test stack (Vitest unit/component/integration, MSW for HTTP, axe for accessibility, Playwright for E2E) — see `client/CLAUDE.md` for how to pick the right layer and which helpers to use.

Before marking any change as done, run the relevant checks for the side you touched:

- **`client/`** — `npm run fmt`, `npm run lint`, `npm run typecheck`, `npm run test`. Run `npm run test:e2e` if the change could affect a smoke-tested flow (login, sending messages). Run `npm run size` if the change might affect bundle size.
- **`server/`** — follow `server/CLAUDE.md` for its test/format/lint commands.

## Running the full app

Start both in separate terminals:

```bash
# Terminal 1 — backend
cd server && cargo run

# Terminal 2 — full Tauri app
cd client && npm run tauri dev
```

The frontend defaults to `http://localhost:3030`; this is configurable at login and stored in localStorage.

## How the two parts connect

All application logic goes over HTTP — the Tauri Rust shell is scaffolding only. The frontend calls the server via `src/api.ts`; there is no Tauri IPC involved.

Real-time messaging uses SSE: the server exposes `GET /messages/subscribe`, and each channel view opens an `EventSource` on mount that closes on cleanup. New messages are POSTed to `POST /message/{channel_id}`, which triggers a broadcast to all subscribers.

## Data model

SeaORM entities in `server/src/entity/`: `user`, `channel`, `message`, `credential`, `session`. IDs are random 16-digit integers (not autoincrement). The database is in-memory and resets on every server restart; dev data (a `general` channel and a `baipas`/`password` dev user) is seeded on startup along with a fixed session token printed to stdout.

The in-memory database is a temporary choice for ease of local development and testing. The plan is to migrate to a persistent SQLite file, but that work hasn't happened yet. When designing new features, assume data will eventually need to survive server restarts — avoid patterns that rely on the reset-on-restart behavior (e.g. hardcoded session tokens, seed-only data, schema changes that would be painful to migrate).

## Auth

The server has register/login/logout/me endpoints using Argon2 password hashing and session cookies. All routes except register/login/logout require authentication via the `require_auth` middleware (`server/src/middleware.rs`). The `AuthUser` extractor is used by handlers that need the caller's identity (e.g. message creation). The frontend has a login page and auth context provider.
