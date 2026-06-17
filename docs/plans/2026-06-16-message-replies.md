# Message Replies Plan

Date: 2026-06-16

## Goal

Add inline message replies to channel chat: a user hovers a message, clicks a **Reply** action, and sends a new top-level channel message that is visually attached to the referenced message. This is not a thread; the reply appears in normal channel history and does not open the thread panel.

## Existing State

Hamlet already has thread replies:

- Server stores thread replies as normal `message` rows with `parent_id = root_message_id`.
- `GET /messages/{channel_id}` filters `parent_id IS NULL`, so thread replies are hidden from channel history.
- Thread APIs live under `/thread/{root_message_id}` and publish `thread_reply_created` / `thread_reply_deleted` SSE events.
- The client currently shows a hover toolbar action labeled **Reply** that means “Reply in thread”.

Because `parent_id` already has thread semantics, inline replies need a separate data contract.

## Product Behavior

1. Hovering a non-deleted channel message shows a **Reply** action.
2. Clicking **Reply** selects that message as the channel composer reply target.
3. The composer shows a dismissible “Replying to …” banner with author and a short text preview.
4. Sending creates a normal channel message linked to the target message.
5. The sent reply appears in the channel stream with a compact quoted/reference preview above the message body.
6. The existing thread flow remains available and distinct from inline replies.
7. If sending fails, the reply target and draft remain for retry.
8. If the referenced message is later deleted, existing replies continue to render safely with an “Original message deleted” style preview instead of breaking.

## Server Plan

### Data Model

Add a new nullable field on messages:

- `reply_to_message_id: Option<i64>` — inline reply reference for top-level channel messages.

Keep `parent_id` exclusively for thread replies.

### API Contract

Extend normal message creation only:

```json
POST /message/{channel_id}
{
  "text": "reply body",
  "reply_to_message_id": 123456789012345
}
```

Multipart photo sends include the same value as a `reply_to_message_id` form field.

Extend `MessageResponse` with:

- `reply_to_message_id: Option<i64>` for the durable reference.
- `reply_to: Option<MessageReferenceResponse>` for display metadata.

The compact reference includes id, author identity, channel id, created/deleted state, and text. Attachments are not required for the first slice; attachment-only originals can be represented with a generic preview fallback on the client.

### Validation

For inline replies:

- The referenced message must exist.
- It must be in the same channel as the new message.
- It must be a top-level channel message, not a thread reply (`parent_id IS NULL`).
- It must not be deleted at send time.
- Thread reply endpoints should reject inline reply references if one is provided.

### Reads and SSE

- `GET /messages/{channel_id}` should hydrate `reply_to` for all returned messages in a batch, avoiding N+1 queries.
- Message create SSE (`kind: "message"`) should include the reply metadata so the sender and other subscribers see the preview immediately.
- Message update responses/events should preserve reply metadata when replacing a message client-side.
- Embed and reaction events can continue patching only their scoped fields.

### Delete Semantics

If a message is referenced by inline replies, deleting it should tombstone the original row instead of hard-deleting it, matching the existing thread-root preservation pattern. This preserves the target id and lets reply previews degrade cleanly. Messages with neither thread replies nor inline replies can still hard-delete as today.

### Server Tests

Add/extend tests for:

- Creating a JSON inline reply returns `reply_to_message_id` and `reply_to`, and channel history includes both messages.
- Creating a multipart/photo inline reply preserves the reply reference.
- SSE message-create payload includes reply metadata.
- Invalid reply targets are rejected: missing target, cross-channel target, thread-reply target, and deleted target.
- Deleting a referenced message tombstones it and existing replies continue to load with a deleted reference.

## Client Plan

### API Types and Fetch Helpers

- Extend `Message` with `reply_to_message_id?: number | null` and `reply_to?: MessageReference | null`.
- Add a `sendMessage` options parameter: `{ replyToMessageId?: number | null }`.
- Include `reply_to_message_id` in JSON bodies and photo `FormData` when present.
- Update MSW fixtures/handlers to store and echo reply metadata.

### Channel Composer

- Add `replyTarget` state to `ChannelView`.
- Pass `onStartReply` into `ChannelMessages`.
- Render a dismissible reply banner above the composer when a target is selected.
- Include the target id in `sendMessage`.
- Clear the target only after a successful send or explicit cancel.
- Clear the target on channel switch, hard-delete SSE for that target, or tombstone update for that target.

### Message List UI

- Add a distinct inline **Reply** toolbar action for non-deleted channel messages.
- Keep the existing thread action, but make the visible/accessible copy unambiguous, e.g. **Thread** / “Reply in thread”.
- Render `message.reply_to` as a compact quoted preview before the reply body.
- Preview should show author display name and a one-line text snippet; if the referenced message is deleted or unavailable, show “Original message deleted”.

### Client Tests

Add/extend tests for:

- `sendMessage` JSON and multipart request shapes with `reply_to_message_id`.
- Hover toolbar exposes inline Reply separately from the thread action.
- Clicking Reply opens the composer banner with the selected target.
- Canceling the banner clears the target.
- Sending includes `reply_to_message_id` and clears the target on success.
- Failed send preserves the draft and reply target.
- SSE-created replies render their reference preview.
- Referenced deleted/unavailable messages render an accessible deleted-original fallback.

## Suggested Implementation Slices

1. Server JSON inline reply contract and history hydration.
2. Server multipart/delete/SSE edge cases.
3. Client API/MSW plumbing plus message preview rendering.
4. Client composer reply flow and toolbar behavior.
5. End-to-end/renderer smoke coverage and final cleanup.

## Non-Goals

- No threaded conversation UI changes.
- No nested inline reply chains beyond storing the direct referenced message id.
- No jumping/scrolling to the original message in this first iteration.
- No attachment-rich preview cards beyond text/author/deleted state.
- No database migration framework work beyond updating the current SeaORM schema/entity model.
