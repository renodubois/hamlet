# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

**Hamlet** is a Discord-like chat app. It has two independent parts:

- `server/` — Rust/Actix-web HTTP API on `127.0.0.1:3030` with SQLite
- `client/` — Tauri v2 desktop app wrapping a SolidJS frontend

Each subdirectory has its own `CLAUDE.md` with commands, architecture details, and gotchas. Start there for work scoped to one side.

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

SeaORM entities in `server/src/entity/`: `user`, `channel`, `message`, `credential`, `session`. IDs are random 16-digit integers (not autoincrement). The database is in-memory and resets on every server restart; dev data (a `general` channel and `anonymous` user id=1) is seeded on startup.

## Auth

The server has register/login/logout/me endpoints using Argon2 password hashing and session cookies. The frontend has a login page and auth context provider. However, `user_id` on new messages is currently hardcoded to `1` — auth is wired up but not yet threaded through message creation.
