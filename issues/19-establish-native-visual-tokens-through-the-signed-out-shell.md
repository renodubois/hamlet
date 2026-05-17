---
id: 19
name: Establish native visual tokens through the signed-out shell
status: completed
tags: [project-iced-visual-refresh, client-iced, afk, design-system]
blocked_by: []
---


# Establish native visual tokens through the signed-out shell

**Type**: AFK

**User stories covered**: 40, 41, 42, 43

## Parent

`docs/iced-client-visual-refresh-prd.md`

## What to build

Introduce a lightweight native Iced design system for the visual refresh and apply it first to the signed-out login experience. The login screen should look polished and grouped while preserving current authentication, server URL persistence, reducer behavior, and API contracts.

## Acceptance criteria

- [ ] Shared native visual tokens/style helpers exist for the refreshed client surfaces without becoming a separate theming framework.
- [ ] The signed-out screen clearly groups server URL, username, and password inputs and presents a polished Hamlet-branded first-launch experience.
- [ ] Login/register/session behavior and storage behavior remain unchanged and covered by existing tests or updated smoke coverage.
- [ ] Native client formatting, check, clippy with warnings denied, and relevant tests pass.

## Blocked by

None - can start immediately
