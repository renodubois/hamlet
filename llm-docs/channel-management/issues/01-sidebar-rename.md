## Parent

Part of renodubois/hamlet#77. The parent spec is authoritative; this ticket implements the local rename slice, with live rename delivery following separately.

## What to build

An authenticated user can rename the channel they right-clicked through the existing sidebar dialog, receive useful feedback, and continue using the same conversation without losing their selection or draft.

## Acceptance criteria

- [ ] Any authenticated user can rename any existing channel, including the bootstrap channel; absent/invalid authentication is rejected. Introduce no ownership or role model.
- [ ] Provide authenticated PATCH /api/v1/channels/{id} with a name-only request and HTTP 200 channel response; reject unexpected fields and invalid input with 400, missing channels with 404, duplicate names with 409, and internal failures with 500 using existing response conventions. Update the shared protocol, API decoder, OpenAPI assembly, and route inventory.
- [ ] Reuse creation rules: trim names; accept 1–64 ASCII bytes using letters, digits, spaces, hyphens, and underscores; enforce case-insensitive uniqueness. Case-only changes and unchanged-name no-ops succeed. Last successful rename wins without version preconditions.
- [ ] Preserve channel identity, type, messages, selection, and draft; move the renamed channel into existing sidebar sort order without navigation or reconciliation reads.
- [ ] Capture the clicked channel ID independently of selection. Prefill the dialog with its displayed name; pre-submission cancel/escape sends no request and discards dialog input.
- [ ] Keep the dialog open while submitting, visibly represent pending state, and prevent duplicate requests. Close only on its own confirmed successful response; show validation, conflict, missing-channel, and request errors inline while preserving input.
- [ ] Explain timeout/unconfirmed outcomes as uncertain; do not automatically retry, roll back shared state, or perform reconciliation reads.
- [ ] Reject obsolete completions after logout/new authentication; use existing session/request identity rules and feature ownership boundaries rather than HTTP logic in views.
- [ ] Extend the existing workspace-to-real-server integration harness as the primary seam and headless real-sidebar-control tests for targeting, pending/error feedback, duplicate clicks, closure, and cancellation. Use focused server route/API decoder tests for contract boundaries; test external behavior, not private implementation structure.
- [ ] Run the applicable documented automated checks. Do not launch native desktop automation or access a real credential store.

## Blocked by

None (can start immediately).
