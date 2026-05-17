---
id: 31
name: Make packaged lifecycle deterministic
status: in-progress
tags: [project-electron-client-port, client-electron, afk, lifecycle]
blocked_by: [30]
---

# Make packaged lifecycle deterministic

**Type**: AFK

**User stories covered**: 42-45, 62, 86.

## Parent

Derived from `docs/electron-client-port-prd.md`.

## What to build

Make packaged startup, shutdown, single-instance behavior, fixed-port handling, and development profile isolation predictable. A normal second packaged launch should focus the existing window instead of racing the fixed renderer port, while development and tests should still support isolated user data for multi-profile Hamlet testing.

## Acceptance criteria

- [ ] Packaged production acquires a single-instance lock, and a second normal launch focuses the existing window rather than starting a conflicting renderer server.
- [ ] Fixed renderer port reservation failures produce a clear user-facing or logged error and exit predictably.
- [ ] Development/test profile isolation supports the existing data-directory override convention without changing the packaged default behavior.
- [ ] App shutdown closes windows and loopback server resources cleanly.
- [ ] Focused tests or a small harness cover the lifecycle decisions that can be verified without full manual QA.

## Blocked by

- Issue 30
