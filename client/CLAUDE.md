# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Web dev server (port 1420)
npm run dev

# Full Tauri app (spawns the dev server automatically)
npm run tauri dev

# Type check
npm run typecheck

# Lint
npm run lint

# Format
npm run fmt
npm run fmt:check

# Build web assets
npm run build

# Build Tauri app
npm run tauri build
```

## Architecture

This is a **Tauri v2 desktop app** wrapping a SolidJS frontend. The frontend talks to a backend HTTP server at `http://localhost:3030` — it does not use Tauri IPC for any application logic (the Rust side is currently only the Tauri scaffolding).

### Frontend (`src/`)

SolidJS with `@solidjs/router`. Entry point is `src/index.tsx`, which mounts a `<Router>` with `App` as the root layout component.

**Data flow:** `App.tsx` fetches the channel list on mount and auto-navigates to the first channel. Each route renders inside `App`'s `<Suspense>` slot. Pages fetch their own data via `createResource`.

**Routing** (`src/routes.ts`): routes are lazy-loaded. Pages live in `src/pages/`, error pages in `src/errors/`.

**Styling:** Tailwind CSS v4 via PostCSS (`postcss.config.js`). CSS entry is `src/index.css`.

**Real-time updates:** `src/pages/channel.tsx` opens an SSE connection to `http://localhost:3030/messages/subscribe` on mount and closes it on cleanup.

### Rust (`src-tauri/`)

Minimal Tauri shell — `src-tauri/src/lib.rs` registers a single placeholder `greet` command. Clippy is configured to warn on `unwrap_used` and `expect_used`.
