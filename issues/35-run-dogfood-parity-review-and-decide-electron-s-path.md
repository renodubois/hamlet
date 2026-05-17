---
id: 35
name: Run dogfood parity review and decide Electron's path
status: in-progress
tags: [project-electron-client-port, client-electron, hitl, product, qa]
blocked_by: [34]
---

# Run dogfood parity review and decide Electron's path

**Type**: HITL

**User stories covered**: 84, 90.

## Parent

Derived from `docs/electron-client-port-prd.md`.

## What to build

Run the human dogfood and product review for the Electron alpha, then record whether Electron replaces Tauri, remains an alternate client, or is abandoned. This issue is intentionally HITL because the outcome depends on user testing, maintainer review, and product judgment rather than implementation alone.

## Acceptance criteria

- [ ] Stakeholders run the documented dogfood and manual QA checklist against the Electron alpha on the required platforms.
- [ ] The decision is recorded with rationale: replace Tauri, keep Electron as an alternate, or abandon the Electron port.
- [ ] Follow-up tracker issues are created for any chosen next path, such as app identity migration, storage/session migration, Tauri retirement, renderer duplication cleanup, signing/notarization, installers, auto-update, public distribution, or archival.
- [ ] The existing Tauri/Solid and native Iced clients are not removed or blocked unless the recorded decision explicitly authorizes a later migration plan.

## Blocked by

- Issue 34
