## Parent

Part of renodubois/hamlet#77. Respect ADR-0001’s deliberate retention of deleted channels and conversations. This is the complete local deletion flow; live deletion propagation follows separately.

## What to build

An authenticated user can remove a channel from normal use through the sidebar while its stored conversation remains retained. The app protects the last active channel, handles failures clearly, and lets users reuse deleted names without inheriting the old conversation.

## Acceptance criteria

- [ ] Add a versioned migration with nullable channel deleted_at; null means active. Preserve existing identities, channel values, message rows, and relationships; existing channels start active.
- [ ] Replace all-row case-insensitive name uniqueness with uniqueness among active channels only. Both creation and the existing rename flow may reuse a deleted name; a replacement has a distinct identity and no inherited messages.
- [ ] Provide authenticated DELETE /api/v1/channels/{id} returning 204 on success. Any authenticated user can delete any active channel, including the bootstrap channel when another active channel exists. Reject absent/invalid authentication; add API decoding, shared contract changes as needed, OpenAPI, and route inventory coverage.
- [ ] Successful deletion sets a timestamp and retains the channel and all its messages unchanged. Do not introduce restore endpoints or individual message deletion markers.
- [ ] Exclude deleted channels from normal lists; return 404 for history, sends, renames, and repeat deletes against deleted channels. Missing channels also return 404.
- [ ] Reject deletion of the last active channel with 409 and distinguishable actionable client feedback. Enforce this atomically under simultaneous deletion requests; deleted rows do not count and failed deletions leave timestamps unchanged.
- [ ] Guard message insertion and history access against deletion races; a separate stale existence check must not permit a message insertion after deletion has won. Test outcomes with controlled ordering, not sleep-based assumptions.
- [ ] Restart preserves the active-channel invariant and does not expose deleted rows or recreate a conflicting bootstrap channel. Verify upgrade and retained data with an isolated previous-schema fixture.
- [ ] Wire the existing deletion dialog to the right-clicked channel ID, independently of selection, and show “You cannot restore this channel in the app.” Pre-submission cancel/escape sends no request.
- [ ] Keep the dialog open while pending, prevent duplicate submissions, and close only on its own confirmed success. Show missing-channel, last-channel, and request failures inline; uncertain outcomes trigger no automatic retry, rollback, or reconciliation read.
- [ ] On confirmed local deletion remove the channel’s local history, draft, and channel-specific operation state and reject late completions/deliveries that would resurrect it. Preserve session/request isolation.
- [ ] If the deleted channel was selected, select the first remaining locally known channel in sidebar order using ordinary history behavior; clear selection if none are locally known. Deleting an unselected channel preserves selection. No reconciliation reads are introduced.
- [ ] Extend the existing workspace-to-real-server harness and headless sidebar controls for deletion, targeting, cleanup, fallback, pending/error feedback, and request counts. Use existing server route/database fixtures to prove migration retention, all deleted-access restrictions, case-insensitive name reuse through create/rename, simultaneous last-channel protection, and send/history races.
- [ ] Keep native desktop/keyring access out of scope and run applicable documented automated checks. Live deletion notifications are intentionally deferred to the following slice.

## Blocked by

- renodubois/hamlet#78 — Rename channels through the sidebar. This slice extends the established action flow and proves that rename respects deletion and active-name reuse.
