# Message Replies PRD

## Problem Statement

Hamlet users can send channel messages, upload photos, react with emoji, and reply in threads, but they cannot reply inline to a specific channel message while keeping the reply in normal channel history. The current visible **Reply** action opens a thread, which is useful for focused conversation but too heavy for lightweight contextual responses. Users need a Discord-like inline reply affordance that preserves message chronology, clearly distinguishes itself from threads, survives live updates and reloads, and degrades safely when the referenced message is deleted.

## Solution

Add inline message replies as normal top-level channel messages with a separate durable reference to another top-level message in the same channel. A user hovers a non-deleted channel message, chooses **Reply**, sees a dismissible composer banner identifying the reply target, and sends text and/or photos as usual. The new message appears in the channel stream with a compact quoted reference above its body. Thread replies remain available through a distinct thread action and continue to use the existing thread data model. The server validates reply targets, hydrates compact reference metadata in message-shaped payloads and SSE creation/update events, and tombstones referenced originals on deletion so existing replies can render an “Original message deleted” fallback instead of breaking.

## User Stories

1. As a channel participant, I want to reply inline to a message, so that I can add context without opening a thread.
2. As a channel participant, I want an inline reply to appear in normal channel history, so that everyone reading the channel sees it chronologically.
3. As a channel participant, I want inline replies to be separate from threads, so that lightweight responses do not create focused thread conversations.
4. As a channel participant, I want the existing thread flow to remain available, so that longer side conversations still have a dedicated place.
5. As a user, I want the message action toolbar to offer a clear inline Reply action, so that I can start a reply from the message I am reading.
6. As a user, I want the thread action to have unambiguous copy, so that I do not accidentally open a thread when I intended an inline reply.
7. As a keyboard user, I want the inline Reply action to be focusable, so that I can start replies without a mouse.
8. As an assistive technology user, I want the inline Reply action to have a clear accessible label, so that I know which message will be referenced.
9. As an assistive technology user, I want the thread action to announce that it opens or replies in a thread, so that thread and inline reply actions are distinguishable.
10. As a user, I want deleted messages not to expose a Reply action, so that I cannot reference content that is already removed.
11. As a user, I want inline Reply to be available on messages written by others, so that I can respond directly to them.
12. As a user, I want inline Reply to be available on my own messages, so that I can add follow-up context.
13. As a user, I want inline Reply to work on messages that also have photos, embeds, reactions, or thread summaries, so that all normal channel messages can be referenced.
14. As a user, I want clicking Reply to select that message as my composer target, so that my next sent message references it.
15. As a user, I want the composer to show a “Replying to …” banner, so that I know my next message will be sent as a reply.
16. As a user, I want the reply banner to show the target author, so that I can verify who I am replying to.
17. As a user, I want the reply banner to show a short preview of the target text, so that I can verify the exact message.
18. As a user, I want attachment-only reply targets to have a useful generic preview, so that the banner is still understandable when there is no text.
19. As a user, I want the reply banner to be dismissible, so that I can cancel replying without clearing my draft.
20. As a keyboard user, I want the dismiss button in the reply banner to be focusable, so that I can cancel the reply target accessibly.
21. As an assistive technology user, I want the banner and dismiss button to announce clearly, so that reply state is not only visual.
22. As a user, I want selecting a new Reply target to replace the previous target, so that I can correct which message I am replying to.
23. As a user, I want my typed draft to remain when I select a reply target, so that I can decide to add context after drafting.
24. As a user, I want my typed draft to remain when I cancel a reply target, so that canceling the target does not discard work.
25. As a user, I want selected photos to remain when I select a reply target, so that photo composition and reply targeting work together.
26. As a user, I want selected photos to remain when I cancel a reply target, so that canceling the target does not discard attachments.
27. As a user, I want sending with a selected reply target to create a linked top-level message, so that the reply is visually tied to the referenced message.
28. As a user, I want sending an inline reply with text only to work, so that normal chat replies are fast.
29. As a user, I want sending an inline reply with photos and text to work, so that visual replies can include captions.
30. As a user, I want sending a photo-only inline reply to work, so that a photo can be the response.
31. As a user, I want the reply target to clear after a successful send, so that my next message starts clean.
32. As a user, I want the draft and selected photos to clear after a successful send, so that successful reply sends behave like normal successful message sends.
33. As a user, I want focus to return to the composer after a successful send, so that I can continue chatting quickly.
34. As a user, I want the draft, photos, and reply target to remain if sending fails, so that I can retry without reconstructing the reply.
35. As a user, I want sending without a reply target to behave exactly as normal message sending does today, so that the feature does not disrupt existing chat.
36. As a reader, I want inline replies to show a compact reference preview above the reply body, so that I understand what the reply is responding to.
37. As a reader, I want the reference preview to show the original author, so that the quoted context is attributable.
38. As a reader, I want the reference preview to show a one-line text snippet, so that the reply context is scannable.
39. As a reader, I want long referenced text to be truncated, so that the message list remains readable.
40. As a reader, I want custom emoji markers in referenced text to render or degrade safely, so that previews do not break on unavailable emoji.
41. As a reader, I want attachment-only originals to show a generic attachment preview, so that a reply to a photo still has context.
42. As a reader, I want unavailable reference metadata to show a safe fallback, so that inconsistent data does not break the message list.
43. As a reader, I want deleted referenced messages to show “Original message deleted,” so that I know the context was removed.
44. As a reader, I want a deleted inline reply message itself to use the normal deleted-message treatment, so that tombstones stay consistent.
45. As a reader, I want the reference preview not to show removed text or photos after the original is deleted, so that deletion semantics are respected.
46. As a reader, I want inline reply previews to appear before message text, photos, embeds, reactions, and thread summaries, so that context is read before the response.
47. As a reader, I want message reactions to remain below the reply body and attachments, so that reactions still apply to the whole reply.
48. As a reader, I want thread summaries to remain below the message content and reactions, so that thread metadata stays visually separate.
49. As a channel participant, I want inline replies to keep chronological ordering by creation time, so that channel history stays predictable.
50. As a channel participant, I want inline replies not to increment thread reply counts, so that thread summaries only reflect actual thread replies.
51. As a channel participant, I want inline replies not to open the thread panel automatically, so that the feature remains lightweight.
52. As a thread user, I want thread replies to continue appearing only in the thread panel, so that existing thread semantics do not change.
53. As a thread user, I want a top-level message that is also an inline reply to still be usable as a thread root, so that users can start a thread from any visible channel message.
54. As a thread user, I want a thread opened from an inline reply to show the inline reply as the thread root, so that threading remains anchored to the selected top-level message.
55. As a user, I want inline replies not to create nested preview chains, so that a reply to a reply shows only its direct target.
56. As a user, I want inline replies to support direct references to any non-deleted top-level message in the same channel, so that follow-up replies remain flexible.
57. As a user, I want cross-channel reply references to be rejected, so that channel context is not confusing.
58. As a user, I want replies to thread replies to be rejected for the inline channel feature, so that thread and channel data models remain clear.
59. As a user, I want replies to missing messages to be rejected, so that stale clients cannot create broken references.
60. As a user, I want replies to already-deleted messages to be rejected, so that removed content is not newly referenced.
61. As a user, I want malformed reply target identifiers to be rejected clearly, so that stale or buggy clients fail safely.
62. As a user, I want existing clients that do not send a reply target to keep working, so that rollout is backward-compatible.
63. As a user with two Hamlet windows open, I want an inline reply sent in one window to appear with its preview in the other, so that live chat stays synchronized.
64. As a user with two Hamlet windows open, I want an edited referenced message to update visible reply previews where practical, so that live context stays current.
65. As a user with two Hamlet windows open, I want a deleted referenced message to update visible reply previews to the deleted fallback, so that live deletion semantics are consistent.
66. As a user composing a reply, I want the selected target to clear if that target is deleted before I send, so that I do not attempt a guaranteed-failing send.
67. As a user composing a reply, I want the selected target to clear when I switch channels, so that I do not accidentally reply in the wrong channel.
68. As a user composing a reply, I want a target hard-delete event to clear the banner, so that stale selected targets do not linger.
69. As a user composing a reply, I want a target tombstone update to clear the banner, so that I do not reply to content that has just been removed.
70. As a message author, I want deleting a message referenced by inline replies to remove its visible content but preserve reply previews safely, so that deletion does not break conversation history.
71. As a message author, I want deleting a message that is not referenced and has no thread replies to behave as a normal hard delete, so that unnecessary tombstones are avoided.
72. As a message author, I want deleting an inline reply that is itself referenced by another inline reply to tombstone it, so that downstream references remain safe.
73. As a message author, I want deleting an inline reply that is not referenced to hard-delete normally, so that normal cleanup behavior remains.
74. As a reader, I want a reply to remain visible when its original is tombstoned, so that the channel conversation stays understandable.
75. As a reader, I want reloading the channel history to show the same reply previews or deleted fallbacks, so that live and loaded state match.
76. As a reader, I want participated-thread previews to preserve reply metadata for roots when applicable, so that opening threads from an inline-reply root remains consistent.
77. As a developer, I want inline reply references stored separately from thread parent identifiers, so that thread behavior is not overloaded.
78. As a developer, I want message-shaped API responses to include both the durable reference id and compact display metadata, so that clients can render immediately without extra requests.
79. As a developer, I want reference metadata batch-loaded for message lists, so that channel history avoids per-message query loops.
80. As a developer, I want message creation SSE payloads to include reply metadata, so that subscribers do not need to refetch after every reply.
81. As a developer, I want message update payloads to preserve reply metadata, so that client replacement logic does not accidentally drop previews.
82. As a developer, I want scoped embed and reaction events to remain scoped, so that inline replies do not broaden unrelated event contracts.
83. As a developer, I want JSON and multipart message creation to share reply validation, so that text and photo replies have the same behavior.
84. As a developer, I want thread reply creation to reject inline reply references if provided, so that a request cannot be both a thread reply and an inline reply.
85. As a developer, I want tests for the reply reference loader, creation contracts, deletion behavior, SSE payloads, and composer UI, so that future message changes do not regress replies.
86. As a tester, I want MSW fixtures to store and echo reply metadata, so that client tests can exercise the feature without a real server.
87. As a tester, I want renderer smoke coverage for sending an inline reply, so that the end-to-end chat path is validated.
88. As an Electron app user, I want inline replies to work without new Electron IPC, so that the desktop app remains a normal HTTP and SSE client.

## Implementation Decisions

- Keep thread replies and inline replies as distinct concepts. Thread replies continue to use the existing parent-message relationship; inline replies use a new nullable message reference dedicated to channel-history replies.
- Store the inline reply reference on message rows as an optional safe-integer message identifier. The field is meaningful only for top-level channel messages; thread replies should persist no inline reply reference.
- Preserve existing top-level channel history semantics: channel history continues to return messages whose thread parent is absent. Inline replies are included because they are normal top-level messages.
- Preserve existing thread history semantics: thread APIs continue to return messages whose thread parent is the thread root. Inline replies do not become thread replies and do not affect thread reply counts or last-reply timestamps.
- Extend normal message creation to accept an optional inline reply target identifier for both JSON and multipart requests.
- Treat absent and explicit null reply target values as “no inline reply.” Treat malformed multipart values, unsafe integers, non-numeric values, or otherwise invalid target identifiers as invalid requests.
- Include the reply target field in multipart creation as a normal form field alongside text and photo fields. Photo validation and upload behavior remain otherwise unchanged.
- Reject inline reply target identifiers on thread reply creation endpoints. The thread endpoint may parse the field to detect it, but must not silently accept or ignore it when provided.
- Validate inline reply targets in shared message-creation logic so JSON and multipart sends cannot drift.
- An inline reply target must exist, be in the same channel as the new message, be a top-level message, and not be deleted at send time.
- A message may directly reference a top-level message that is itself an inline reply. Rendering still shows only the direct referenced message and never expands recursive chains.
- Cross-channel references, references to thread replies, references to deleted messages, and references to missing messages are rejected before inserting the new message.
- Use existing authentication middleware and current-user identity for all reply creation. Inline replies introduce no anonymous or unauthenticated creation path.
- Extend the canonical message response contract with both the durable reply target identifier and optional compact reference metadata.
- The compact reference metadata should include the referenced message id, channel id, author identity needed for display, creation timestamp, deletion state, and text needed for a short preview.
- The compact reference metadata intentionally omits full embeds, reactions, thread summaries, and rich attachment metadata in the first slice.
- Attachment-only referenced messages should be representable by clients through a generic preview fallback rather than by adding full attachment preview cards to the reference contract.
- Deleted referenced messages should hydrate as deleted references where the tombstoned row exists. If reference metadata is unexpectedly unavailable, clients should use the same deleted/unavailable fallback rather than fail rendering.
- Build or extract a server-side reply reference loader as a deep module. Its stable interface should accept message identifiers and return compact reference responses grouped by message id, including deleted-state handling and author display data.
- Use batch loading for reply references anywhere a list of message-shaped responses is built, avoiding per-message database lookups for channel history and preview surfaces.
- Apply the expanded message response contract consistently to channel history, message creation responses, message update responses, thread root responses, thread reply responses, older thread-reply pagination, participated-thread previews, and SSE payloads that carry full messages.
- For newly created top-level inline replies, the HTTP response and message-create SSE event should include both the durable reference id and compact reference metadata so the sender and subscribers render the preview immediately.
- For thread-created events, full message payloads should include the reply reference fields with null values unless the payload message is a top-level root that already has an inline reference.
- Message edit responses and message-updated SSE events must preserve the edited message’s inline reply metadata when replacing the full message client-side.
- Existing scoped embed update, embed suppression, and reaction update events can continue to patch only their scoped fields; they do not need to include reply reference metadata.
- Deleting a message must account for both existing thread replies and incoming inline reply references. If either exists, deletion should tombstone the message instead of hard-deleting it.
- Tombstoning a message should clear visible message content, attachments, embeds, and reactions while preserving the row identifier and enough metadata for referencing replies to degrade safely.
- Hard deletion remains valid for messages that have no thread replies and no inline reply references.
- Deleting a referenced target should not remove or rewrite the referencing messages’ durable reply target identifiers.
- Deleting a thread reply should continue to follow thread-reply deletion semantics because thread replies cannot be valid inline reply targets.
- If a top-level inline reply is itself referenced by another inline reply, deleting it should tombstone it just like any other referenced top-level message.
- No new SSE event kind is required for inline reply creation. The existing message-created and message-updated full-message events carry the necessary canonical payload.
- Extend client message types with the durable reply target field and compact reference metadata field. Fixture builders should default both to null or absent values consistently.
- Change the client message send helper to accept a stable options bag for optional photos and optional reply target data, avoiding additional positional-argument churn.
- Include the reply target identifier in JSON request bodies when a reply target is selected.
- Include the reply target identifier in multipart form data when a reply target is selected and photos are attached.
- Do not include a reply target field for ordinary messages unless the user selected one; ordinary message creation should preserve current behavior.
- Update MSW state and handlers to parse, store, validate lightly where useful, and echo reply target metadata for JSON and multipart message creation.
- Add channel composer state for the selected reply target. The selected target should store enough already-loaded metadata to render the banner immediately without a fetch.
- Pass a start-reply callback from the channel page into the message list so the toolbar can select the composer target.
- Render a reply banner above the channel composer when a target is selected. The banner should show author display name, a short preview, and a clear/dismiss control.
- Clear the selected reply target only after explicit cancel, successful send, channel switch, or a live event proving that the target was removed or tombstoned.
- Preserve the selected reply target, draft text, and selected photos when a send fails or returns a non-success response.
- On successful send, clear the reply target along with the draft and selected photos, then return focus to the composer as current successful sends do.
- Add a distinct inline Reply action to the channel message toolbar for non-deleted top-level channel messages.
- Relabel the existing thread action to visible and accessible copy that clearly indicates thread behavior, such as “Thread” with an accessible “Reply in thread” label.
- Keep ownership rules for edit and delete unchanged. Inline Reply and reaction actions are not limited to the message author.
- Do not add inline reply controls to deleted-message tombstones.
- Render a compact reply reference preview before a message’s body. The content order for non-deleted messages becomes reference preview, text, attachments, embeds, reactions, then thread summary.
- Do not render a reply reference preview for a message that is itself deleted or tombstoned.
- Build or extract a small client-side message-reference preview module/component used by both the composer banner and message list preview where practical. Its behavior should cover author labeling, one-line truncation, attachment-only fallback, deleted fallback, and unavailable fallback.
- Keep the reply preview presentational and non-navigational in the first iteration. It should not scroll or jump to the original message.
- When a full message-updated event arrives for a message that is currently referenced by visible replies, patch those visible reply previews to reflect edited text or deleted state where practical.
- When a message-deleted event arrives for the selected composer target, clear the selected reply target. If any visible stale references somehow point at that id, mark them unavailable rather than failing.
- When a message-updated tombstone event arrives for the selected composer target, clear the selected reply target and update visible references to the deleted fallback.
- When a new message SSE event contains reply metadata, append it to the active channel store using the canonical payload so the reference preview renders immediately.
- Reaction, embed, typing, voice, screen-share, camera, avatar, and custom emoji flows do not require contract changes for inline replies beyond rendering existing message content in the new order.
- No Electron main-process, preload, shell, static-server, or IPC changes are required because message replies use existing authenticated HTTP and SSE paths from the renderer.

## Testing Decisions

- Good tests should assert externally observable behavior: request and response contracts, validation status codes, durable reply target identifiers, hydrated reference metadata, visible composer banners, accessible action labels, retained drafts on failure, live SSE rendering, deletion fallbacks, and stable thread behavior. Tests should not assert private signal names, exact internal query structure, or fragile CSS classes except where they represent user-visible state.
- Server API integration tests should cover creating a JSON inline reply and receiving both the durable reference id and compact reference metadata.
- Server API integration tests should cover channel history returning both the original message and the inline reply as top-level messages.
- Server API integration tests should cover multipart/photo inline replies preserving the reply target identifier and returning hydrated reference metadata.
- Server broadcast tests should cover message-create SSE payloads for inline replies, using the existing quiet broadcaster test-client pattern rather than the production ping loop.
- Server update tests should cover editing an inline reply and preserving its reply metadata in the full updated message response and broadcast.
- Server target-update tests should cover editing or tombstoning a referenced message and ensuring subsequent history loads hydrate the updated or deleted reference state.
- Server validation tests should cover missing reply targets, cross-channel targets, thread-reply targets, deleted targets, malformed target ids, and unauthenticated requests.
- Server thread endpoint tests should cover JSON and multipart thread reply requests rejecting provided inline reply target identifiers.
- Server deletion tests should cover deleting a referenced original tombstoning it, preserving existing inline replies in channel history, and hydrating deleted reference metadata or fallback state.
- Server deletion tests should cover hard-deleting an unreferenced message still publishing the normal deletion event.
- Server deletion tests should cover deleting an inline reply that is itself referenced by another inline reply, verifying tombstone behavior.
- Server reference-loader tests should cover multiple replies referencing multiple originals, several replies referencing the same original, deleted originals, missing/inconsistent references where practical, and author display-name/avatar data.
- Server response-shape tests should cover message-shaped payloads across channel history, creation, updates, thread roots, thread replies, older replies, and participated-thread previews so reply fields do not drift.
- Client API tests should cover JSON message creation with and without a reply target.
- Client API tests should cover multipart message creation with photos and a reply target, including form field shape and ordinary photo fields.
- Client API tests should verify that normal messages omit or null out reply target data without changing existing behavior.
- MSW handler tests should cover storing and echoing reply metadata for JSON and multipart sends.
- Client message list component tests should cover the inline Reply action appearing on non-deleted messages and not appearing on deleted messages.
- Client message list component tests should cover the thread action remaining available with unambiguous visible and accessible copy.
- Client message list component tests should cover clicking inline Reply invoking the composer-target callback with the selected message.
- Client composer/page integration tests should cover opening the reply banner with author and preview text.
- Client composer/page integration tests should cover dismissing the banner while preserving draft text and selected photos.
- Client composer/page integration tests should cover successful send including the reply target identifier and clearing the target afterward.
- Client composer/page integration tests should cover failed send preserving draft text, selected photos, and reply target.
- Client composer/page integration tests should cover channel switch clearing the selected reply target.
- Client SSE/page integration tests should cover incoming inline reply messages rendering their reference previews without a refetch.
- Client SSE/page integration tests should cover target tombstone or hard-delete events clearing the composer banner when that target is selected.
- Client SSE/page integration tests should cover visible reply previews updating to edited text or deleted fallback when a referenced target update arrives.
- Client reference preview component tests should cover author display, text truncation, attachment-only fallback, deleted fallback, unavailable fallback, and accessible labeling.
- Client rendering tests should cover reference preview order relative to message text, attachments, embeds, reactions, and thread summaries.
- Client accessibility tests should include axe coverage where practical for the message toolbar, composer banner, and reply preview rendering.
- Renderer E2E coverage should include at least one smoke test that logs in, sends an inline reply in the general channel, and verifies the preview appears in the channel stream.
- Electron E2E does not need new reply-specific coverage unless the implementation unexpectedly touches shell launch or packaged renderer behavior.
- Prior art for server tests includes message CRUD tests, thread tests, photo multipart tests, reaction tests, and broadcaster tests that attach a test subscriber.
- Prior art for client tests includes message API tests, channel message component tests, message input tests, composer photo-selection tests, channel page integration tests, MSW handler tests, fake SSE tests, and existing axe helpers.
- Relevant pre-completion checks for implementation are the server formatter, clippy, and tests plus the client formatter, linter, typecheck, and tests. Add renderer E2E because the feature changes the core send-message flow.

## Out of Scope

- Changing the existing thread data model or making inline replies a kind of thread reply.
- Removing the thread panel, thread reply composer, participated-thread page, or thread summary behavior.
- Inline replies inside thread reply lists.
- Cross-channel reply references.
- Replying to deleted messages.
- Rich attachment preview cards inside the compact reference preview.
- Recursive nested quote rendering beyond the direct referenced message.
- Jumping, scrolling, highlighting, or permalink navigation to the original message.
- Quoting selected text ranges or partial-message snippets chosen by the user.
- Editing a sent message to add, remove, or change its reply target.
- Notifications, mentions, unread counts, or push behavior based on inline replies.
- Moderation tools for rewriting or removing other users’ reply references.
- Per-channel permission changes for who can reply or read reply references.
- Database migration-framework work beyond updating the current schema/entity model used by Hamlet.
- Electron shell, preload, IPC, packaging, signing, or static-renderer changes.
- Creating issue tracker tickets or implementing code as part of this PRD.

## Further Notes

- The design intentionally reserves the existing thread parent relationship for thread replies and adds a separate reference for inline channel replies.
- Hamlet currently uses in-memory SQLite in development, but reply references should be modeled as durable data that can migrate cleanly when persistent SQLite is adopted.
- The feature should lean on small deep modules for server-side reference hydration and client-side reference preview behavior, keeping handlers and components focused on orchestration.
- Inline replies should integrate with existing message capabilities—photos, embeds, reactions, edits, deletes, SSE, and custom emoji—without creating parallel message rendering paths.
