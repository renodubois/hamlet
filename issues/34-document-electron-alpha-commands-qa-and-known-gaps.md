---
id: 34
name: Document Electron alpha commands, QA, and known gaps
status: in-progress
tags: [project-electron-client-port, client-electron, afk, docs, qa]
blocked_by: [33]
---

# Document Electron alpha commands, QA, and known gaps

**Type**: AFK

**User stories covered**: 74, 80, 84-85, 90.

## Parent

Derived from `docs/electron-client-port-prd.md`.

## What to build

Document how to run, test, package, and manually evaluate the Electron alpha. The docs should make the server lifecycle explicit, describe fixed-origin and profile-isolation gotchas, record known alpha gaps, and give testers enough platform-specific guidance to dogfood the app safely.

## Acceptance criteria

- [ ] Electron documentation covers development, renderer-only browser mode, Electron dev launch, build, package, E2E, size checks, data-directory/profile overrides, fixed renderer origin, and fixed-port failure behavior.
- [ ] Documentation states that Electron does not start or manage the Rust server in the first version.
- [ ] Known gaps and alpha expectations are recorded for users, support, and product decision-makers.
- [ ] Manual QA guidance covers macOS, Windows, and Linux smoke areas including login/logout/session persistence, server URL persistence, credentialed API calls, SSE delivery, channel/message/avatar/embed flows, external links, voice device enumeration, LiveKit join/leave/switch, two-client local development, OS media prompts, and unpacked package launch.
- [ ] Boundary notes explain renderer duplication, security/static-server module responsibilities, and future shared-package/product-decision follow-ups.

## Blocked by

- Issue 33
