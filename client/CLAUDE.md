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

# Unit + component + integration tests (Vitest)
npm run test
npm run test:watch

# End-to-end tests (Playwright; requires browsers via `npx playwright install chromium`)
npm run test:e2e

# Bundle-size budget (builds, then checks against .size-limit.json)
npm run size

# Build web assets
npm run build

# Build Tauri app
npm run tauri build
```

## Working on this codebase

When you add or change client functionality, **write tests for it**. Pick the layer that fits:

- **Pure helpers / API wrappers** → co-located `*.test.ts` using Vitest with `fetch` stubbed (see `src/api.test.ts`).
- **Components** → `*.test.tsx` using `@solidjs/testing-library` (see `src/components/modal.test.tsx`, `src/components/channel_sidebar.test.tsx`).
- **Flows across providers/components** → `*.integration.test.tsx` with MSW serving fake HTTP and `FakeEventSource` for SSE (see `src/pages/login.integration.test.tsx`, `src/pages/channel.integration.test.tsx`).
- **Accessibility** → add an `expectNoA11yViolations(container)` assertion alongside component tests (helper in `src/test/a11y.ts`; examples in `src/a11y.test.tsx`).
- **Full stack smoke** → Playwright spec in `e2e/` — keep these to golden-path smoke tests only.

Before marking work as done, always run:

```bash
npm run fmt
npm run lint
npm run typecheck
npm run test
```

If you changed the built bundle meaningfully (added a dependency, new page, etc.), also run `npm run size` and update `.size-limit.json` if the new baseline is intentional.

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

### Tests (`src/**/*.test.{ts,tsx}`, `src/**/*.integration.test.tsx`, `e2e/`)

- **Vitest** + `@solidjs/testing-library` + `happy-dom`. Config: `vitest.config.ts`. Setup: `src/test/setup.ts` (registers jest-dom matchers, axe matchers, and an MSW server; clears `localStorage` between tests).
- **MSW** fake backend lives in `src/test/msw/`. `handlers.ts` holds REST handlers plus a mutable `HandlerState` (`state.me`, seeded channels/messages, `sentMessages`, etc). `server.ts` exposes `server`, `resetMswState()`, and `mswState()`. By default requests are 401/empty — individual tests call `resetMswState` and mutate the returned state for their scenario.
- **SSE** is not served by MSW. Instead, `src/test/msw/sse.ts` defines `FakeEventSource`, and `channel.integration.test.tsx` shows the `vi.mock("../api", ...)` pattern to swap `messagesEventSource()` for one. Use `latestFakeEventSource()` to get a handle and `push*()` helpers to simulate events.
- **Accessibility:** `expectNoA11yViolations(container, label)` in `src/test/a11y.ts`. Prefer calling this inside component tests over writing dedicated axe-only tests.
- **Playwright** E2E in `e2e/`. `playwright.config.ts` spawns both `cargo run` (server) and `npm run dev` (client) via `webServer`. Tests rely on the server's seeded dev user (`baipas` / `password`) and the auto-created `general` channel. The server uses in-memory SQLite, so each run starts fresh.
