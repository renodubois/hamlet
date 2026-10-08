# Channel rename and deletion design

Status: design decisions confirmed; implementation has not started.

## Confirmed decisions

- Keep action dialogs open while submitting, prevent duplicate submissions, and close only on confirmed success. Show errors inline and preserve rename input. Timeouts are uncertain outcomes, not proof of failure; do not automatically retry or roll back. Deletion warning: “You cannot restore this channel in the app.”
- Renaming reuses creation’s trimming, 1–64 ASCII-byte validation, and case-insensitive active-name uniqueness. Last successful rename wins; no stale-edit check. Preserve channel identity, messages, and selection; allow case-only changes and successful unchanged-name no-ops.
- Any authenticated user may rename or delete any active channel, matching channel creation permissions. No ownership or administrator model is introduced.
- Deletion sets a nullable deletion timestamp on the channel; retain its messages unchanged. Deleted channels are excluded from lists and return 404 for history, sends, renames, and repeat deletes. No restore API in this change.
- On successful local or live deletion, discard the deleted channel’s local history/draft and ignore late request completions for it. If selected, choose the first remaining locally known channel in sidebar order; clear selection if none are known, without reconciliation reads. An ordinary history read for the replacement selection is allowed.
- Reject deletion of the last active channel on the server, including concurrent delete attempts. Deleted channels do not count toward this invariant.
- Publish rename/delete events through the existing best-effort live stream. Do not add reconnect reconciliation or replay. A client that misses deletion may retain cached messages until reopening, but the server blocks normal access.
- Case-insensitive name uniqueness applies only to active channels. Reusing a deleted name creates a distinct channel and never inherits messages. See `llm-docs/adr/0001-channel-soft-deletion.md`.

## Existing constraints

- Creation trims names and accepts 1–64 ASCII bytes: letters, digits, spaces, hyphens, and underscores.
- The row context menu currently opens rename/delete dialogs, but confirmation only closes them. Actions must capture the clicked channel ID, not infer their target from selection.
- Workspace behavior owns channel state, selection, pending operations, and request identities; API requests belong in `client/src/api/`, not views.
- Live updates are currently best-effort creation notifications with no reconnect catch-up, replay, polling, or manual refresh.

## Implementation plan

1. Add a versioned migration with nullable `channels.deleted_at` and replace all-row name uniqueness with a partial unique index on active `name_key` values. Preserve existing channel/message data.
2. Add authenticated `PATCH /api/v1/channels/{id}` with a name-only rename request and a channel response, and `DELETE /api/v1/channels/{id}` with a 204 success response. Map missing/deleted channels to 404 and duplicate-name/last-active-channel conflicts to 409 with distinguishable feedback. Update shared protocol types, OpenAPI assembly, and route inventory.
3. Keep the last-active-channel invariant atomic under concurrent deletes. Prevent message insertion racing deletion from bypassing the active-channel check; guard history reads as well. Successful mutations alone publish rename/delete events.
4. Extend the API client and session-scoped workspace transitions/coordinator for pending actions, feedback, completion identities, rename reordering, and deletion cleanup/fallback. Handle HTTP/event arrival in either order without duplicates or resurrecting deleted channels; do not introduce general synchronization infrastructure.
5. Bind dialogs in `client/src/views/sidebar/channel_row.rs` to the clicked channel ID and workspace operations, not the selected channel. Render pending/error state and the revised warning.
6. Test migrations/data retention, active-name reuse, auth, validation, missing/deleted access, case-only/no-op rename, last-channel protection including concurrent attempts, mutation/message races, live publication, request/event ordering, late completions, selection/draft cleanup, and dialog request/feedback behavior. Run the documented server/client automated checks without native desktop or keyring access.
