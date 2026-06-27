# Channel Unread Notifications PRD

## Problem Statement

Hamlet users can participate in multiple text channels, but today the channel sidebar gives no durable indication that new channel activity appeared while they were looking elsewhere, had the app unfocused, or were scrolled away from the newest messages. Users must manually visit channels to discover whether anything changed, and there is no way to distinguish ordinary unread activity from messages that directly mention them.

This is especially confusing in Hamlet because messages arrive in real time through SSE and the active channel can receive new messages while the user is reading older history. Automatically treating an open channel as read would hide activity the user has not actually seen. Hamlet needs channel-level unread indicators whose read cursor is tied to the user, the channel, message ordering, viewport position, and app focus rather than to message ids alone.

## Solution

Add v1 channel unread notifications for authenticated users and text channels. The channel sidebar shows a lightweight unread treatment for channels with ordinary unread top-level messages and a numeric badge only for unread top-level messages that mention the current user. Ordinary unread message counts are not shown in v1.

A channel becomes read only when the channel is open, the Electron renderer is focused, the user is at or near the bottom of the top-level message list, and the client can report the last visible top-level message. The server stores a monotonic per-user, per-channel read cursor based on message creation ordering. Existing historical messages are baselined as already read when the feature lands so users are not flooded with stale unread state.

For active channels, new messages that arrive while the user is scrolled up stay unread and are surfaced with a small “new messages” divider or jump-to-bottom affordance. Reading a channel on one open client clears the same user’s indicators on their other open clients through a user-scoped read-state SSE event, with snapshot refetches on login, reconnect, and focus as recovery.

## User Stories

1. As a Hamlet user, I want the channel sidebar to show when a text channel has unread activity, so that I know where to look next.
2. As a Hamlet user, I want unread channels to look different from read channels, so that I can scan the sidebar quickly.
3. As a Hamlet user, I want channels that mention me to show a numeric badge, so that direct attention requests stand out from ordinary chat.
4. As a Hamlet user, I do not want ordinary unread message counts in v1, so that the sidebar stays lightweight.
5. As a Hamlet user, I want mention badges to count only unread mentions, so that the number reflects pending attention.
6. As a Hamlet user, I want opening a channel to clear unread state only after I reach the bottom, so that messages below my viewport are not marked read prematurely.
7. As a Hamlet user, I want a channel to remain unread if I am scrolled up in that same channel, so that I do not miss new messages arriving below me.
8. As a Hamlet user, I want a visible new-message divider or jump-to-bottom button while I am scrolled up, so that I can tell new content arrived and move to it easily.
9. As a Hamlet user, I want clicking the jump-to-bottom affordance to take me to the newest messages, so that I can catch up quickly.
10. As a Hamlet user, I want reaching the bottom while focused to clear the active channel’s unread indicator, so that the sidebar reflects what I have actually seen.
11. As a Hamlet user, I do not want a background or unfocused window to mark messages read, so that unread indicators remain trustworthy.
12. As a Hamlet user, I want unread state to survive closing and reopening Hamlet, so that I do not lose track of unread channels across restarts.
13. As a returning user, I do not want every old message in every existing channel marked unread when this feature ships, so that the rollout is not noisy.
14. As a new user, I do not want all pre-existing channel history marked unread, so that my unread state starts from my arrival.
15. As a user in a newly created text channel, I want future messages in that channel to become unread if I have not seen them, so that new channels behave normally.
16. As a message sender, I do not want my own messages to create unread indicators for me, so that sending does not make my sidebar noisy.
17. As a message sender, I do not want sending a message while scrolled up to advance my read cursor, so that intervening unread messages are not accidentally cleared.
18. As a user with two Hamlet windows open, I want reading a channel in one window to clear indicators in the other, so that my clients stay in sync.
19. As a user with two Hamlet windows open, I do not want another user’s read-state changes sent to my client, so that private read state stays private.
20. As a user with an unreliable connection, I want unread state to recover after SSE reconnects, so that missed real-time events do not leave stale badges.
21. As a user who refocuses Hamlet after time away, I want unread state refreshed, so that sidebar indicators catch up with server truth.
22. As a user who logs in or restores a session, I want the unread snapshot loaded automatically, so that the sidebar is correct before I navigate.
23. As a channel reader, I want unread detection to respect Hamlet’s message ordering, so that random message ids do not produce wrong unread state.
24. As a channel reader, I want messages created at the same timestamp to be ordered deterministically, so that cursor comparisons are stable.
25. As a channel reader, I want deleted messages to disappear from unread and mention counts, so that deleted content does not continue to demand attention.
26. As a channel reader, I want tombstoned top-level messages not to count as unread content, so that deleted message placeholders do not create noise.
27. As a channel reader, I want a visible tombstoned top-level message to be usable as the last visible cursor point, so that I can still clear older unread activity after seeing that position in the timeline.
28. As a user, I want thread replies excluded from channel unread indicators in v1, so that opening a channel does not imply I saw replies inside thread panels.
29. As a thread user, I want thread unread state deferred to a dedicated future model, so that thread behavior can be designed correctly.
30. As a user who is mentioned in a thread reply, I do not expect that reply to affect channel mention badges in v1, so that the badge semantics stay clear.
31. As a user who is mentioned in a top-level channel message, I want that unread message to increment the channel’s mention badge, so that I can find direct mentions.
32. As a user who mentions myself, I do not want my own authored message to create a mention badge for me, so that self-authored content remains excluded.
33. As a user, I want edits to old messages not to create new unread or mention notifications in v1, so that unread state is based on message creation rather than later edits.
34. As a user, I want an edited unread message that no longer mentions me to stop contributing to my mention badge, so that badges do not point at content that no longer mentions me.
35. As a user, I want unread indicators to update when a message is deleted, so that badges and dots do not linger for removed content.
36. As a user viewing the active channel at the bottom, I want incoming messages to remain easy to follow, so that normal live chat still feels real time.
37. As a user viewing the active channel while scrolled up, I do not want new messages to force-scroll me to the bottom, so that I can keep reading older context.
38. As a user, I want the current active channel styling to remain distinct from unread styling, so that I can tell where I am and what is unread.
39. As a user, I want voice channels to remain unaffected by channel unread indicators in v1, so that only text-channel message activity is represented.
40. As a keyboard user, I want the jump-to-bottom control to be focusable and clearly labeled, so that I can use it without a mouse.
41. As an assistive technology user, I want unread and mention badges to have accessible names, so that the sidebar state is perceivable.
42. As an assistive technology user, I want new-message affordances to be announced without being disruptive, so that I can understand new activity while reading.
43. As a user, I want read-state updates to be fast and non-blocking, so that scrolling at the bottom does not make chat feel sluggish.
44. As a user, I want transient read-state API failures not to break sending or reading messages, so that chat remains usable.
45. As a user, I want the server to be authoritative for read cursors, so that clients cannot corrupt unread state by sending stale or invalid cursors.
46. As a user, I want stale mark-read requests to be harmless, so that delayed network responses do not move my cursor backward.
47. As a user, I want invalid mark-read requests rejected, so that one channel’s cursor cannot be advanced with another channel’s message.
48. As a user, I want thread reply ids rejected as channel read cursors, so that channel unread state remains top-level only.
49. As a user, I want read-state APIs to require authentication, so that my read state is tied to my session.
50. As a developer, I want a dedicated read-state API rather than unread fields embedded into generic channel responses, so that channel metadata stays user-agnostic.
51. As a developer, I want read cursors stored as a creation-time and message-id tuple, so that Hamlet’s random message ids are not misused for ordering.
52. As a developer, I want cursor updates to be monotonic, so that concurrent clients cannot regress read state.
53. As a developer, I want missing read-state rows initialized predictably, so that migrations, new users, and new channels behave consistently.
54. As a developer, I want unread and mention summaries derived from message rows and mention edges, so that v1 avoids premature counter materialization.
55. As a developer, I want the unread state logic isolated behind a small server service interface, so that ordering, baselining, and counting can be tested thoroughly.
56. As a developer, I want the client unread store isolated behind a context and pure transition logic, so that sidebar and channel view code do not duplicate unread rules.
57. As a developer, I want viewport/read-marker detection isolated from rendering details, so that focus, near-bottom, and last-visible behavior can be tested.
58. As a developer, I want the existing SSE event context extended rather than creating unrelated real-time plumbing, so that Hamlet keeps one real-time event path.
59. As a developer, I want user-scoped read-state events to coexist with existing broadcast message events, so that current chat, voice, emoji, reaction, and thread behavior is preserved.
60. As a developer, I want client recovery to refetch snapshots on reconnect and focus, so that the lack of SSE replay ids does not compromise correctness.
61. As a tester, I want deterministic fixtures for unread snapshots and SSE read-state events, so that UI behavior is easy to verify.
62. As a tester, I want integration tests for scrolled-up and near-bottom behavior, so that the riskiest UX semantics are protected.
63. As a tester, I want server tests for same-timestamp message ordering, so that the tuple comparison is not accidentally simplified.
64. As a tester, I want server tests for deleted messages and mentions, so that badges clear when content is removed.
65. As a maintainer, I want no Electron IPC changes for this feature, so that normal Hamlet application logic remains HTTP plus SSE.

## Implementation Decisions

- V1 covers authenticated text-channel unread state for top-level channel messages only. Voice channels, thread replies, and the persistent Threads navigation item do not receive unread indicators in this slice.
- The sidebar displays a dot, bar, bold treatment, or similarly lightweight state for ordinary unread activity. It displays a numeric badge only when the unread mention count is greater than zero. V1 does not expose or render ordinary unread counts.
- Add a dedicated `user_channel_read_state` persistence model keyed by `(user_id, channel_id)`. It stores `last_read_created_at`, `last_read_message_id`, and `updated_at` in addition to the key fields.
- Represent an empty-channel baseline with a sentinel cursor before all real messages, such as `(0, 0)`, so cursor fields can remain non-null and future messages in an empty channel become unread normally.
- Store and compare read cursors by the tuple `(created_at, id)`. A message is after the cursor when its `created_at` is greater than `last_read_created_at`, or when timestamps are equal and its id is greater than `last_read_message_id`.
- Never use `message_id > last_read_message_id` as an unread comparison. Hamlet message ids are random safe JavaScript integers, not chronological ids.
- Cursor writes are monotonic. A mark-read request whose derived cursor is behind or equal to the stored cursor is an idempotent no-op and must not move the cursor backward.
- Schema setup should be migration-shaped and safe for persistent SQLite. The feature adds the read-state table and supporting indexes without requiring users to delete local data.
- Backfill existing users and text channels when the feature lands. For each pair, initialize the cursor to the latest existing top-level message by `(created_at, id)`, or the empty sentinel if the channel has no top-level messages.
- New user registration initializes read states for existing text channels to the latest top-level message at that time, so new users are not assigned all historical messages as unread.
- New text-channel creation initializes read states for existing users to the empty sentinel, so future messages in that new channel can become unread.
- Snapshot handling may repair missing read-state rows defensively, but missing rows should be rare after migration, user-creation, and channel-creation hooks.
- Add a dedicated authenticated read-state snapshot endpoint, `GET /read-states`. It returns one summary per text channel visible to the user.
- Each read-state summary includes `channel_id`, `has_unread`, `mention_count`, `last_read_created_at`, `last_read_message_id`, and `updated_at`. It intentionally omits ordinary `unread_count`.
- Add an authenticated mark-read endpoint, `PUT /channels/{channel_id}/read-state`, with a body containing `last_visible_message_id`.
- The mark-read endpoint validates that the channel exists, is a text channel, and that the supplied message exists in that channel as a top-level message. Thread replies are rejected as channel cursors.
- A visible tombstoned top-level message may be accepted as a cursor target because it still represents a position in the top-level timeline. Hard-deleted messages cannot be used because no row remains to validate.
- The server derives the authoritative cursor from the validated message row. The client never sends timestamps or cursor tuples directly.
- The mark-read endpoint returns the authoritative summary for the affected channel after the monotonic update, whether the request advanced the cursor or was a no-op.
- Unread summaries count only top-level messages after the cursor where `deleted_at` is null and the author is not the current user.
- `has_unread` is true when at least one such message exists. It does not require counting every ordinary unread message.
- `mention_count` counts unread top-level messages that mention the current user, excluding deleted messages, self-authored messages, messages at or before the cursor, and thread replies.
- Mention badge semantics are based on message creation for v1. Edit-created mentions must not create new mention notifications. Implementations should preserve enough first-created mention metadata, or equivalent creation-time mention edges, to distinguish mentions present at message creation from mentions added later.
- If an unread message that originally mentioned the user is edited so it no longer mentions the user, the mention badge should drop because the current message no longer contains that mention.
- Message edits do not advance read cursors and do not make old read messages unread again.
- Message deletion removes the message from unread and mention summaries. Tombstoned messages remain usable for timeline ordering but do not count as unread content.
- Sending a message never directly advances the sender’s cursor. If the sender is focused and near the bottom, the normal mark-read flow will advance the cursor after the message is visible.
- Existing top-level message creation SSE events are used for optimistic client unread updates. Self-authored messages are ignored for the sender’s unread state.
- Add a `read_state_updated` SSE event containing the updated read-state summary for one channel.
- Read-state SSE delivery is user-scoped. The SSE subscriber model should know the authenticated user id and support publishing an event to only that user’s live connections. Existing broadcast events continue to fan out as they do today.
- Publish a read-state update event only after a successful authoritative cursor write. Stale no-op writes may return the current summary without emitting if no observable state changed.
- Because Hamlet’s current SSE stream has no replay ids, the client treats SSE as best-effort and refetches the read-state snapshot on login/session restore, SSE connection or reconnection, and renderer focus or visibility return.
- A periodic recovery refetch is optional and should be added only if focus/reconnect recovery proves insufficient.
- Add a client read-state API wrapper for snapshot and mark-read calls. The wrapper uses normal authenticated HTTP, not Electron IPC.
- Add an unread/read-state provider in the authenticated client tree near the existing event, channel, and voice contexts so both the channel sidebar and channel view can consume it.
- The client unread provider owns a map of channel id to read-state summary, exposes selectors for `hasUnread` and `mentionCount`, exposes snapshot refetch, and exposes a mark-read action.
- Keep client unread state transitions in a pure reducer-like module. Inputs include server snapshots, message-created events, deletion/tombstone invalidations, and read-state-updated events.
- The client may optimistically mark a channel unread on a new top-level message event when the message is not authored by the current user, is after the known cursor, and the channel is not immediately eligible to be marked read.
- Client handling of deletes, tombstones, or ambiguous update events should favor snapshot refetch over guessing when mention counts could be wrong.
- The channel view must stop unconditional auto-scroll on every incoming message. It should auto-scroll only if the user was already at or near the bottom before the new message was inserted.
- The channel view tracks whether the user is near the bottom using a small threshold so minor layout differences do not prevent read clearing.
- The channel view determines the last visible top-level message from rendered top-level message rows, not from thread replies or hidden state.
- Mark-read calls are debounced or coalesced so ordinary scrolling and layout changes do not flood the server.
- A channel is eligible for mark-read only when all required conditions are true: active channel, focused/visible renderer, near-bottom message list, and known last visible top-level message.
- If a channel has no top-level messages, there is no last visible message to send. The empty sentinel baseline remains in effect until real messages exist.
- The active channel’s new-message divider or jump control appears when new messages arrive below the current viewport while the user is not near the bottom.
- The jump-to-bottom action scrolls to the newest top-level message. If the renderer is focused and the bottom is reached, the normal mark-read flow clears unread state.
- Sidebar unread treatment applies to text-channel links and must remain visually distinct from the active route styling.
- Mention badges should use accessible labels that include the channel name and count, such as indicating unread mentions in that channel.
- The read-state feature must not change message send, edit, reaction, attachment, custom emoji, voice, or thread APIs except where those systems must emit or preserve data needed for unread correctness.
- No Electron IPC changes are required. Hamlet continues to use HTTP APIs and SSE between the renderer and server for normal application behavior.
- Extract a server read-state service as a deep module with a compact interface for ensuring baselines, fetching snapshots, marking read, comparing cursors, and deriving unread summaries.
- Extract or centralize a server cursor-order helper so tuple comparison and monotonic update rules are not duplicated across handlers and tests.
- Extend the existing real-time event transport with user-targeted publish support rather than adding a separate SSE endpoint for read states.
- Extract a client viewport/read-marker helper or hook that reports near-bottom state, new-message-below-viewport state, and last visible top-level message id through a simple interface.
- Keep implementation phases dependency-safe: server schema and read-state service first, server APIs second, user-scoped SSE third, client API/provider fourth, sidebar/channel view UI fifth, and final QA last.

## Testing Decisions

- Good tests should assert externally observable behavior: HTTP status and JSON shape, persisted cursor values, unread/mention summaries, emitted SSE events, visible sidebar indicators, visible jump-to-bottom affordances, focus/scroll-driven mark-read calls, and accessible labels. Tests should not depend on private signal names, internal component state, or incidental DOM class names unless those classes are the public styling contract under test.
- Server schema tests should verify the read-state table exists with the expected uniqueness invariant for `(user_id, channel_id)` and useful indexes for snapshot and mark-read queries.
- Server migration/backfill tests should seed existing users, channels, empty channels, and historical top-level messages, then verify initial snapshots do not mark historical messages unread.
- Server initialization tests should cover new user registration baselining existing channels and new text-channel creation initializing existing users.
- Server cursor comparison tests should cover messages with increasing timestamps, equal timestamps with increasing ids, and random ids that are not chronological.
- Server mark-read tests should cover successful cursor advancement, stale no-op updates, concurrent monotonic behavior, invalid channel ids, voice channels, cross-channel message ids, thread reply ids, nonexistent message ids, and tombstoned top-level cursor targets.
- Server snapshot tests should verify `has_unread` and `mention_count` for unread top-level messages after the cursor.
- Server exclusion tests should verify self-authored messages, deleted messages, messages at or before the cursor, and thread replies do not contribute to unread or mention summaries.
- Server mention tests should verify top-level unread messages that mention the current user increment `mention_count`, self-authored mentions do not, and edit-created mentions do not create new v1 mention badges.
- Server deletion tests should verify hard-deleted and tombstoned messages disappear from unread and mention summaries and trigger enough real-time or refetch behavior for clients to recover.
- Server API tests should verify `GET /read-states` and `PUT /channels/{channel_id}/read-state` require authentication and return only the current user’s read-state data.
- Server SSE tests should verify `read_state_updated` is delivered to all live connections for the same authenticated user and not delivered to other users.
- Server regression tests should verify existing message creation, message update, thread reply, reaction, typing, voice, channel, custom emoji, and embed SSE events still serialize as before.
- Client API tests should verify snapshot and mark-read wrappers call the correct endpoints, send `last_visible_message_id`, and parse read-state summaries.
- Client pure state tests should cover applying snapshots, applying new message events, excluding self-authored messages, incrementing mention badges from created messages that mention the current user, applying read-state-updated events, and choosing refetch for ambiguous delete/update cases.
- Client event-context tests should cover the new `read_state_updated` subscription and any connection/reconnection notification used by snapshot recovery.
- Client unread provider tests should verify snapshot fetch after authenticated mount, refetch on focus/visibility return, refetch on SSE reconnect, and cleanup on logout/unmount.
- Channel sidebar component tests should verify text channels show unread treatment, mention badges show numeric counts, read channels show neither, active channel styling remains distinct, and voice channels do not render unread badges.
- Channel view integration tests should verify initial near-bottom load marks the channel read only when focused and a last visible top-level message exists.
- Channel view integration tests should verify incoming messages while near the bottom auto-follow and clear through the normal mark-read flow.
- Channel view integration tests should verify incoming messages while scrolled up do not auto-scroll, keep the channel unread, and show the new-message divider or jump-to-bottom control.
- Channel view integration tests should verify clicking jump-to-bottom reaches the bottom and then sends a mark-read request when focused.
- Channel view integration tests should verify sending a message while scrolled up does not mark intervening messages read unless the user returns to the bottom.
- Client focus tests should verify blur, hidden visibility, or unfocused renderer state prevents mark-read calls even if the active channel is near the bottom.
- Client multi-client tests using fake SSE should verify a read-state-updated event clears another mounted view for the same user.
- Client deletion/update tests should verify message deletion or tombstone events cause badge correction through snapshot refetch or equivalent externally visible state correction.
- Accessibility tests should verify mention badges, unread state labels, the new-message divider, and jump-to-bottom control are perceivable and keyboard-operable.
- Prior art for server tests includes Hamlet’s existing Actix message lifecycle tests, schema setup tests, broadcast tests, mention hydration tests, and deletion/tombstone tests.
- Prior art for client tests includes API wrapper tests, context provider tests, channel sidebar tests, channel view MSW integration tests, fake EventSource helpers, and existing accessibility checks.
- Before implementation is considered complete, run the relevant server formatting, linting, and test commands, plus the client formatting, linting, typecheck, and Vitest suite. Run Electron or Playwright E2E if the implementation materially affects login, shell launch, or send-message smoke flows.

## Out of Scope

- Ordinary unread message counts in the sidebar or API response.
- Thread unread state, thread mention badges, and a `user_thread_read_state` model.
- Push notifications, desktop notifications, sounds, notification preferences, mutes, or a notification inbox.
- Edit-created mention notifications or durable notification history beyond the v1 creation-time mention semantics needed for badges.
- Rich per-message read receipts or showing which users have read a message.
- Role mentions, channel mentions, group mentions, or `@everyone` semantics.
- Permission-specific channel visibility beyond Hamlet’s current authenticated channel model.
- Materialized unread counter tables or background counter maintenance unless query-derived summaries prove too slow later.
- Cross-device push delivery or mobile-specific behavior.
- Electron IPC changes.
- Visual redesign of the sidebar beyond unread styling and mention badges.
- Voice-channel activity indicators.

## Further Notes

- The critical correctness rule is that “open” does not mean “read.” A channel is read only when the focused user has returned to the bottom and the client reports a validated top-level message cursor.
- The safest implementation path is to make the server snapshot and mark-read behavior authoritative before adding optimistic client behavior.
- Query-derived summaries should be acceptable for v1 because Hamlet’s local SQLite dataset is small. The cursor tuple and indexes leave room for later optimization if channel histories grow.
- The client should prefer correctness over clever local bookkeeping. When deletes, tombstones, reconnects, or focus changes make local state uncertain, refetch the snapshot.
- This PRD intentionally keeps thread unread work separate. Opening a channel does not imply the user saw replies hidden inside a thread panel.
