# Message deletion

Status: implemented by #85–#88; original automated verification is recorded in [message-deletion-verification.md](message-deletion-verification.md). Subsequent fixes and isolated two-client native deletion coverage are recorded in [the adversarial review](message-deletion-adversarial-review.md); device-specific native acceptance remains incomplete. Paths are relative to the repository root.

## Agreed requirements

- Only a message's author may delete it; the server enforces ownership by authenticated user identity, not display name. There is no deletion age limit. Moderator/admin deletion of other users' messages is outside this feature's scope.
- Deletion is soft: retain the message row, including its original text, author, channel and creation time, and mark it with a deletion timestamp (`deleted_at`, matching channel terminology). Deleted messages are excluded from all normal message reads and disappear entirely from the client's conversation (no visible placeholder).
- No message restoration feature is included. Confirmation must not describe retained content as permanently erased; use “You can't restore this message in the app.”
- Successful deletion publishes a change through the existing live-event system.
- The client offers a red trash icon in a message's upper-right hover action tray, only on the current user's messages, and a red **Delete Message** context-menu action. Both paths require confirmation before issuing deletion.
- Receiving a deletion event removes the message from client state.
- Delivery remains best-effort, consistent with the existing live system. A missed deletion event may leave a message visible until authoritative history is loaded again. Reconnect does not initiate catch-up reads, replay or synchronization. A newly issued server read excludes already-deleted messages.

## Delete endpoint semantics

- Add authenticated `DELETE /api/v1/channels/{channel_id}/messages/{message_id}` with no request body.
- Both IDs follow existing 15-digit decimal-string path validation; malformed IDs return 400. The message must belong to the channel in the path; a mismatch returns 404.
- An authenticated author deleting an active message receives 204 with no response body.
- A repeat deletion by the same author in an active channel also returns 204; preserve the original deletion timestamp and emit no duplicate event.
- Another user's message returns 403. Missing messages and messages in deleted/missing channels return 404. An invalid or expired session follows existing 401 handling.
- Ownership checks also apply to repeat requests; idempotence is not permission to delete another user's message.
- Errors use the existing error envelope: 400 `bad_request`, 401 `unauthorized`, 403 `forbidden`, 404 `not_found`, and 500 `internal_error`. Update the shared API decoding, OpenAPI assembly and route inventory alongside the endpoint.

## Persistence and live event

- Add a nullable `messages.deleted_at` timestamp; existing messages begin undeleted. Keep deletion state out of ordinary message DTOs because deleted messages are not returned.
- The first deletion atomically checks the authenticated author, message/channel association, active channel and undeleted state when marking the row. Concurrent requests must not overwrite the timestamp or publish duplicate deletion events. A channel deletion that wins first makes message deletion return 404; a message deletion that wins first may complete normally.
- Apply `m.deleted_at IS NULL` to newest and older-page history queries before ordering and the `limit + 1` pagination probe. Preserve chronological ordering, timestamp ties and existing opaque cursor semantics. An old cursor remains valid when its referenced message has been deleted.
- History retains the existing SQLite snapshot semantics: a read whose snapshot predates deletion may finish with the old message; a snapshot acquired after deletion excludes it. Client-side observed-deletion protection handles overlapping responses.
- Add the shared protocol event `MessageDeleted { channel_id, message_id }`, serialized as `{"type":"message_deleted","channel_id":"…","message_id":"…"}` in the existing SSE `change` frame. Do not include message text, author details or deletion time in this event.
- Prepare the event before mutation and notify synchronously after a successful first deletion, using the existing best-effort write-then-notify system. Emit nothing for a repeat deletion or rejected request. Cancellation/process-failure publication gaps remain accepted; no outbox, replay or new synchronization mechanism is introduced.

## Client mutation behavior

- Confirmation submits a deletion request; do not optimistically remove the message.
- Remove the message on either a successful HTTP deletion response or its live deletion event, whichever arrives first. Applying both is harmless.
- Disable duplicate submission while pending. A definite failure leaves the message visible and shows an error. A timeout/transport failure explains that deletion may have succeeded and permits a deliberate retry; never automatically resend.
- Once deletion has been observed (by event or HTTP success), a late history page, creation event or send response must not reinsert that message. This includes deletion while history is loading, even if the message is not currently loaded.
- Apply observed deletion to active and inactive loaded history and relevant pending confirmation overlays; do not initiate a read solely because an event arrives.

## Interaction and presentation

- Show the action tray on hover or keyboard focus, anchored to the message row's upper-right corner within the conversation pane, not to the desktop window. It must remain reachable while moving from the message to the tray and must not shift message layout.
- Only the author's messages expose a Delete action, in both the tray and context menu. Keyboard activation opens the same confirmation dialog.
- Selecting/copying text must continue to work. Clicking the tray must not start text selection; pressing Delete while typing or selecting text must not delete a message.
- Confirmation: **Delete Message**, a bounded message preview, the warning **You can't restore this message in the app.**, **Cancel**, and a red **Delete** button. Initially focus Cancel to avoid accidental confirmation.
- Dialog lifecycle: close on successful deletion or an observed event deleting its target; close safely if the originating channel/session disappears. Failures show feedback in the dialog with a deliberate retry option. While the request is pending, disable repeated confirmation and follow the existing channel-dialog pending-dismissal policy.
- Preserve the reader's viewport when messages disappear; if the anchor is deleted, use the nearest surviving message. Preserve bottom-follow if already at the bottom. Do not jump a scrolled-up reader to latest.
- Keep pagination based on server ordering cursors, even if the cursor's message is deleted. Removal alone must not trigger an automatic backfill request. Empty displayed history is not proof that older history is exhausted.
- If deletion removes every loaded message and older history is available, show **Load older messages**. Preserve an in-flight older read or its local retry control if loading has already begun or failed. Only show the exhausted/empty-conversation state when no older history remains.

## Existing constraints

- `server/src/messages/operations.rs` uses timestamp-and-ID cursor positions, not a lookup of the cursor's message row. History queries currently run in a SQLite read snapshot.
- `server/src/channels/operations.rs` already uses `deleted_at` for channel deletion. Repeated channel deletion returns not found; deleted channels reject history and message creation.
- `protocol/src/lib.rs` owns the shared event union. Existing channel deletion also uses best-effort notifications.
- `client/src/chat/mod.rs` and `client/src/chat/state.rs` own request outcomes and live-event application. Views should not become a second state owner.
- `llm-docs/adr/0001-channel-soft-deletion.md` deliberately retains deleted channels and conversations; it adds no restoration API.

## Acceptance coverage

### Server and wire contract

- An author can delete recent or old messages; retained text/identity/creation values are unchanged, and the deletion timestamp is set.
- Another user cannot delete an active or already-deleted message (403); unauthenticated/expired requests follow 401 handling. Invalid path IDs return 400; missing messages, wrong channel association and deleted channels return 404.
- Sequential and concurrent repeat requests by the author return 204, retain the first timestamp and publish only the first transition's event on the normal success path.
- Newest and older-page reads exclude deleted messages. Mixed deleted/live rows still produce correct page sizes, timestamp-tie ordering and next cursors; a cursor pointing to a now-deleted message remains usable.
- Existing database initialization applies the new nullable column without discarding existing rows. Deleting a channel still retains its messages, and its messages remain inaccessible.
- Verify deletion/channel-deletion races and history/deletion snapshot races under controlled execution.
- Exercise the real DELETE route with an authenticated event subscriber; verify event IDs/payload, no event on rejection/repeat, and shared protocol/OpenAPI/route inventory coverage.

### Client state and requests

- Either HTTP success or an event removes the message, regardless of arrival order. Duplicate events, repeated responses and an event for an unloaded message are safe.
- Late newest-history/older-page responses, creation events and send confirmations cannot resurrect a deletion observed in this session, including when the message was not loaded at event time. Deletion bookkeeping is session-owned and cleared when the chat lifetime ends.
- Remove from inactive as well as active loaded history and pending confirmation overlays. Do not create unloaded history or schedule catch-up reads merely to process deletion.
- Pending submission disables duplicate requests. Definite failures leave the message visible with feedback; timeout/transport uncertainty allows only deliberate retry. An event confirming deletion takes precedence over a later failed HTTP response and must not restore the message or reopen a closed dialog.
- Channel removal, session end and late outcomes cannot mutate another channel/session's state or leave an orphaned dialog. Reconnect preserves the existing best-effort policy and issues no catch-up reads or automatic delete retries.

### Presentation

- Hover/focus exposes a red, accessibly labeled trash button only on the current author's messages. The red **Delete Message** context-menu action has the same ownership restriction and confirmation path.
- Opening or cancelling confirmation sends no request. Confirming sends one request for the captured message/channel, not whichever message is currently hovered. Preview is bounded; Cancel initially has focus; Delete is red.
- Successful deletion, including from another window, closes the target dialog. Failure feedback supports deliberate retry. Mouse/keyboard operation preserves text selection/copy and composer behavior.
- Tray appearance must not shift layout or become unreachable when moving the pointer onto it.
- Deletion above, within and below a scrolled viewport preserves its surviving anchor; deletion of the anchor selects a nearest surviving row. Bottom-follow remains bottom-follow; scrolled-up readers do not jump to latest.
- Deleting all loaded rows preserves the older-history control, loading state or retry state as applicable. It does not automatically fetch older pages or incorrectly declare exhaustion.

## Scope exclusions

- No restoration, hard deletion/content erasure, moderator deletion, bulk deletion, editing, additional tray actions, deletion age limit, automatic backfill or durable event synchronization.
- No desktop automation or real-keyring access is authorized by this spec; native verification requires the existing separate safety consent.

## Open decisions

None.
