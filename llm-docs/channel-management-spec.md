## Problem Statement

The sidebar already offers rename and delete dialogs, but confirming either action only dismisses the dialog: channels cannot actually be renamed or deleted. Users need these controls to manage their community’s text channels, with consistent server enforcement, useful failure feedback, and updates visible to other connected users.

Deletion should remove a channel from normal use without destroying its stored conversation, preserving the possibility of restoration in a future feature. The community must retain at least one active channel.

## Solution

Implement authenticated channel renaming and soft deletion on the backend and connect the existing sidebar dialogs to those operations. Any authenticated user can act on any active channel. Renaming preserves the channel’s identity and conversation; deletion hides it and blocks normal API access while retaining its messages.

Dialogs remain open while requests run, prevent duplicate submissions, and close only on confirmed success. Validation failures, conflicts, missing channels, and uncertain outcomes receive inline feedback. Other connected clients receive best-effort rename/delete events using the existing live-update stream.

## User Stories

1. As an authenticated user, I want to rename a channel from its sidebar context menu, so that its name reflects its purpose.
2. As an authenticated user, I want the dialog to start with the channel’s current displayed name, so that I can edit rather than retype it.
3. As an authenticated user, I want an action to target the channel I right-clicked even when another channel is selected, so that I do not modify the wrong channel.
4. As an authenticated user, I want surrounding whitespace trimmed from a submitted name, so that accidental padding does not become part of its identity.
5. As an authenticated user, I want rename validation to match channel creation, so that naming rules are consistent.
6. As an authenticated user, I want an invalid name explained without losing my edit, so that I can correct it.
7. As an authenticated user, I want duplicate active names rejected case-insensitively, so that active channels remain distinguishable.
8. As an authenticated user, I want to change only the capitalization of a channel name, so that I can correct its presentation.
9. As an authenticated user, I want submitting an unchanged valid name to succeed harmlessly, so that I am not blocked by a no-op edit.
10. As an authenticated user, I want renaming to preserve the channel’s messages and draft, so that changing its name does not disrupt its conversation.
11. As an authenticated user, I want renaming to preserve my selected channel, so that sidebar reordering does not navigate me elsewhere.
12. As an authenticated user, I want renamed channels to move into the existing sidebar sort order, so that navigation remains predictable.
13. As an authenticated user, I want the last successful rename to determine the name when users edit concurrently, so that the result follows a simple shared rule.
14. As an authenticated user, I want to delete a channel from its sidebar context menu, so that an unused channel no longer appears in normal navigation.
15. As an authenticated user, I want a confirmation explaining that I cannot restore the channel in the app, so that I understand the current consequence before deleting it.
16. As an authenticated user, I want to cancel an unsubmitted rename or deletion without making a request, so that opening a dialog is not itself a mutation.
17. As an authenticated user, I want deleted channels excluded from channel lists, so that they no longer appear as usable spaces.
18. As a community operator, I want deletion to retain the channel and its messages, so that future restoration remains possible.
19. As an authenticated user, I want deleted channels to reject history reads, sends, renames, and repeat deletes as not found, so that deletion consistently removes them from normal use.
20. As an authenticated user, I want to reuse a deleted channel’s name, so that removing an obsolete channel does not permanently reserve its name.
21. As an authenticated user, I want a replacement channel to have its own identity and conversation, so that reusing a name does not expose old messages.
22. As an authenticated user, I want deletion of the last active channel rejected with a clear explanation, so that the community always retains a conversation space.
23. As a community operator, I want concurrent deletions to preserve the last-active-channel rule, so that simultaneous requests cannot remove every channel.
24. As an authenticated user, I want deletion of my selected channel to select the first remaining locally known channel, so that I can continue participating.
25. As an authenticated user, I want deletion of an unselected channel to leave my selection unchanged, so that another channel’s removal does not interrupt me.
26. As an authenticated user, I want the deleted channel’s local history and draft discarded, so that deleted conversation state does not remain in normal workspace use.
27. As an authenticated user, I want an empty selection if no remaining channel is locally known, so that missed live creations do not cause an invented or invalid selection.
28. As an authenticated user, I want pending requests visibly represented and duplicate submissions prevented, so that repeated clicks do not issue repeated mutations.
29. As an authenticated user, I want dialogs to close only after their own request confirms success, so that a live event is not mistaken for confirmation of my action.
30. As an authenticated user, I want failures shown inline while preserving rename input, so that I can understand and address the problem.
31. As an authenticated user, I want timeouts explained as uncertain outcomes without automatic retries, so that the app does not repeat a mutation that may already have succeeded.
32. As an authenticated user, I want other connected users’ renames and deletions reflected live, so that shared navigation stays current while updates are delivered.
33. As an authenticated user, I want live events and HTTP responses handled in either arrival order without duplicate channels or resurrected deleted state, so that asynchronous delivery does not corrupt my workspace.
34. As an authenticated user, I want late requests for a deleted channel ignored, so that its messages or draft cannot reappear after removal.
35. As an authenticated user, I want late results from a previous session ignored, so that logout or a new login isolates my workspace.
36. As an unauthenticated visitor, I want protected channel operations unavailable without valid authentication, so that only community users can change channels.
37. As a community operator, I want existing channels and messages preserved across the schema upgrade, so that deploying channel management does not lose data.
38. As an authenticated user, I want reconnection to retain the existing best-effort behavior without replaying writes, so that connection recovery cannot repeat my actions.

## Implementation Decisions

- Extend the existing channel operations/handlers, shared protocol, API client, workspace behavior/coordinator, sidebar dialogs, schema migrations, and API contract assembly. Preserve current ownership boundaries: views issue intentions and render local interaction state; workspace behavior owns selection, pending actions, feedback, and completion rules; the API module owns HTTP construction and decoding.
- Authorization matches channel creation: any authenticated user may rename or delete any active channel. Introduce no creator ownership or administrator role. The bootstrap channel is not privileged; it may be renamed or deleted when another active channel exists.
- Use a versioned migration adding nullable `deleted_at` to channels. A null value means active; successful deletion sets the timestamp. Retain the channel’s identity, name, type, and message rows. Existing channels start active.
- Replace all-row case-insensitive name uniqueness with a partial unique index applying only to active channels. Preserve existing data and message relationships during migration. Creating or renaming an active channel may reuse a deleted channel’s name; identity, not name, determines its conversation.
- Respect ADR-0001: retain deleted channels and conversations intentionally, and defer restoration. Any future restore must resolve an active-name conflict rather than merge identities or transfer messages.
- Add authenticated `PATCH /api/v1/channels/{id}` accepting a name-only request and returning the updated channel with HTTP 200. Reject unexpected request fields consistently with existing creation requests. Renaming does not change channel type.
- Add authenticated `DELETE /api/v1/channels/{id}` returning HTTP 204 on success. Deletion is not an idempotent success for an already-deleted channel: repeat deletion returns HTTP 404.
- Preserve existing error response conventions: HTTP 400 for invalid input, 401 for absent/invalid authentication, 404 for missing/deleted channels, 409 for duplicate active names or last-active-channel protection, and 500 for internal failures. Ensure the API/workspace can distinguish duplicate-name and last-channel conflicts for actionable feedback. Update OpenAPI and the route inventory alongside route registration.
- Apply creation’s name rules to renaming: trim surrounding whitespace; require 1–64 bytes consisting only of ASCII letters, digits, spaces, hyphens, or underscores. Names remain unique among active channels after ASCII case folding. Case-only changes succeed; an unchanged valid name succeeds as a no-op.
- Concurrent renames use last-successful-write semantics without an expected-name or version precondition. Renaming preserves channel identity, messages, selection, and draft, and reorders the sidebar using its existing sort convention.
- Exclude deleted channels from all normal channel lists. History reads, message sends, renames, and repeat deletes against a deleted channel return not found. Guard message insertion against deletion races, rather than relying on a separate existence check that can become stale; history access must likewise respect deletion.
- Enforce the last-active-channel invariant atomically on the server, including concurrent deletion attempts. Deleted rows do not count. A failed delete must not set a timestamp or publish a deletion event.
- Publish rename events carrying the updated channel and deletion events identifying the deleted channel through the existing authenticated best-effort stream after successful mutation. No-op renames need not publish an event. Failed operations publish no success events.
- Do not add a new replay, catch-up, polling, or reconciliation policy. A disconnected client can miss a rename/deletion and retain stale cached messages until reopening the workspace; the server still blocks normal access to the deleted channel.
- Apply successful local deletion confirmations and received live deletions to the workspace: remove the channel and discard its cached history, draft, and channel-specific operation state. Ignore obsolete completions and late deliveries for a channel known to be deleted; they must not resurrect it. Session/request identities continue to reject previous-session results.
- When the selected channel is removed, select the first remaining locally known channel in sidebar order and perform the ordinary selection/history behavior. If none are locally known, clear selection rather than inventing a channel or issuing reconciliation reads. Removing an unselected channel leaves selection unchanged.
- Merge live updates and local HTTP confirmations by channel identity, handling either arrival order without duplication or resurrection. A live event may update shared state but does not by itself confirm the originating dialog’s request. Do not expand this feature into a general globally ordered synchronization system.
- Capture the clicked channel ID when opening the context-menu dialog; never derive its target from the currently selected channel. Preserve the existing dialog-only rename input and pre-submission cancel/escape behavior.
- While a mutation is pending, keep its dialog open, represent the pending state, and prevent duplicate submission. Close on its confirmed successful HTTP result. Render validation errors, name conflicts, last-channel protection, missing channels, and request failures inline, preserving rename input where applicable.
- Treat timeouts and unconfirmed responses as uncertain outcomes. Do not automatically retry, roll back a delivered event, or issue reconciliation reads. Replace the deletion warning with “You cannot restore this channel in the app.”

## Testing Decisions

- The user approved the existing workspace-to-real-server integration harness as the primary seam, with focused existing server-route/database and headless sidebar-control tests. Introduce no new production test seam unless an existing one demonstrably cannot exercise a required outcome.
- A good test drives a supported interface and checks externally observable behavior: responses, delivered events, channel lists, selected conversation, draft visibility, rendered feedback, and request counts. Do not assert private helper structure, exact SQL text, or incidental widget nesting. Database inspection is appropriate specifically to prove the persistence/retention contract.
- Extend workspace real-route integration prior art: two authenticated users against real HTTP/SSE and a temporary migrated SQLite database, with existing response/stream gates controlling arrival order. Test shared rename/delete behavior, selection preservation/fallback, late completions, prior-session isolation, and HTTP/event arrival in either order. Confirm no automatic mutation replay or reconciliation reads; an ordinary replacement-channel history read is allowed.
- Extend server route tests using existing Actix application wiring and isolated database fixtures. Cover both users acting on each other’s channels, absent/invalid authentication, valid/invalid request bodies, trimming/length/character rules, case-only/no-op rename, duplicate active names, missing/deleted access, populated-channel deletion, repeat deletion, and last-active-channel conflict.
- Exercise simultaneous deletes through the highest existing route seam to prove that at least one active channel survives. Exercise send/delete and history/delete races with controlled ordering and assert outcomes consistent with deletion, rather than timing-dependent sleeps. Add narrowly targeted operation/publication checks only where existing route gates cannot establish the needed order.
- Verify migrations from the previous schema preserve channel identities, names, messages, and relationships; verify deletion timestamps and retained message rows; verify active-name reuse creates a distinct channel with no inherited conversation. Verify restart does not expose deleted channels or recreate a conflicting bootstrap channel.
- Extend existing live publication tests where useful to prove successful mutations emit matching payloads and failed mutations emit none. Reuse existing controlled live-stream/coordinator fixtures for deterministic late/duplicate deliveries and missed-event/reconnect behavior.
- Extend headless GPUI/Kit sidebar tests through real context menus, input fields, and buttons using semantic controls. Cover targeting an unselected channel, initial name, cancel/escape before submission, revised deletion warning, pending state, duplicate-click prevention, successful closure, inline conflicts/errors, retained rename input, and uncertain outcomes without replay. Do not reach into private view fields to perform the action.
- Exercise the API module’s existing transport substitution seam and decoding tests as needed for new response statuses, malformed responses, and conflict feedback while retaining real-route coverage as primary evidence.
- Run the documented server and client automated format, lint, test, and build checks applicable to the changed components. Automated checks do not establish native desktop acceptance; do not launch desktop automation or access a real credential store for this spec.

## Out of Scope

- Restoration APIs/UI, reversible deletion workflows, or resolving name conflicts during restoration.
- Physically deleting channels/messages, purging retained data, soft-deleting each message separately, or exposing deleted history through normal endpoints.
- Roles, administrators, channel ownership, or other new permission rules.
- Unicode or otherwise changed naming rules, channel-type changes, channel merging, or message transfer when a name is reused.
- Optimistic concurrency/version checks for renames or a global event ordering/revision system.
- Catch-up reads on reconnect, replay/outbox guarantees, polling, refresh controls, or automatic retry of uncertain writes.
- Recovering a deleted channel’s discarded draft elsewhere, persistent local history, or offline queued mutations.
- Native desktop automation, real-keyring verification, and unrelated client/server refactoring.

## Further Notes

The design decisions were confirmed in the preceding interview; implementation has not started. Soft deletion preserves a future option, not a current promise of restoration. Cached content on a client that misses a best-effort deletion notification is an accepted limitation, not evidence that the server still grants access.

The last-active-channel rule is server-authoritative. A client can temporarily know no remaining channels because it missed creation events; its correct fallback is empty selection, even though an active channel still exists on the server.

This spec intentionally builds on existing feature ownership and test infrastructure rather than introducing a general synchronization or permissions framework.
