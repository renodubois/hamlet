# User Mentions PRD

## Problem Statement

Hamlet users can write messages, replies, edits, links, and custom emoji, but they cannot explicitly mention another user in a durable, Discord-style way. Plain-text `@name` text is ambiguous, can break when display names change, cannot reliably power future mention notifications, and does not let readers inspect who was mentioned. Users need a keyboard-first mention workflow that works anywhere messages are composed, renders mentions as recognizable inline entities, makes mentions of the current user stand out, and protects private user data while exposing only the public identity fields needed for search and display.

## Solution

Add authenticated user mentions across Hamlet messages. Typing `@` in the shared message editor opens a fuzzy user autocomplete menu. Selecting a user inserts a durable `<@user_id>` marker into message text rather than a mutable display name. The server validates well-formed markers, stores semantic message-to-user mention edges, and hydrates message responses with public user data. Rendered messages show mentions as highlighted inline names, and clicking a mention opens a compact public profile preview with avatar, display name/name fallback, and username. Any non-deleted message that mentions the currently authenticated user receives a visible row-level emphasis.

The first slice covers direct user mentions only. It works in the channel composer, message edits, thread composer, thread reply edits, channel history, thread views, participated-thread previews, and real-time SSE updates. It deliberately does not add push notifications, role/channel/everyone mentions, profile pages, or backfilled semantic mentions for old plain-text `@name` messages.

## User Stories

1. As a channel participant, I want typing `@` in the channel composer to open user suggestions, so that I can mention someone without remembering exact spelling.
2. As a thread participant, I want typing `@` in the thread reply composer to open user suggestions, so that thread replies have the same mention workflow as channel messages.
3. As a message editor, I want typing `@` while editing an existing channel message to open user suggestions, so that I can add or replace mentions during edits.
4. As a thread reply editor, I want typing `@` while editing a thread reply to open user suggestions, so that edit behavior is consistent everywhere.
5. As a keyboard-first user, I want the first matching user selected by default, so that I can commit the likely mention quickly.
6. As a keyboard-first user, I want Enter to commit the selected mention while the mention menu is open, so that I can stay on the keyboard.
7. As a keyboard-first user, I want Tab to commit the selected mention while the mention menu is open, so that autocomplete behaves like familiar completion UIs.
8. As a keyboard-first user, I want ArrowDown and ArrowUp to move through mention suggestions, so that I can choose a different person.
9. As a keyboard-first user, I want mention suggestion navigation to wrap, so that I can move quickly between the first and last option.
10. As a keyboard-first user, I want Escape to dismiss mention autocomplete before canceling an edit, so that I do not lose an edit while only trying to close suggestions.
11. As a keyboard-first user, I want a second Escape with autocomplete closed to keep the existing edit-cancel behavior, so that current editor shortcuts remain available.
12. As a mouse user, I want clicking a user suggestion to insert the mention, so that autocomplete works without keyboard navigation.
13. As a channel participant, I want typing a lone `@` to show an initial deterministic list of users, so that I can browse without typing a query.
14. As a channel participant, I want typing letters after `@` to filter suggestions, so that I can narrow the list quickly.
15. As a channel participant, I want suggestions to match usernames case-insensitively, so that capitalization does not matter.
16. As a channel participant, I want suggestions to match display names case-insensitively, so that I can find people by the names I see in chat.
17. As a channel participant, I want exact matches to rank above prefix matches, so that the most precise result is easiest to select.
18. As a channel participant, I want prefix matches to rank above substring or fuzzy matches, so that common autocomplete behavior feels predictable.
19. As a channel participant, I want deterministic ordering within ranking ties, so that suggestions do not jump around unexpectedly.
20. As a channel participant, I want the suggestion list to stay compact, so that it does not obscure too much of the conversation.
21. As a channel participant, I want suggestion rows to show avatars, so that I can distinguish similarly named users.
22. As a channel participant, I want suggestion rows to show display name or name fallback, so that I can recognize people by their visible chat identity.
23. As a channel participant, I want suggestion rows to show username, so that I can disambiguate users with similar display names.
24. As a user, I want self-mentions to be allowed, so that I can deliberately refer to myself when useful.
25. As a user, I want failed user search requests not to block typing, so that transient network errors do not disrupt message composition.
26. As a user, I want stale autocomplete responses ignored when I type quickly, so that old search results do not replace newer ones.
27. As a user, I want mention autocomplete hidden when text is selected, so that accepting a suggestion does not unexpectedly replace unrelated text.
28. As a user, I want mention autocomplete to avoid email addresses like `hello@host`, so that writing normal text does not open irrelevant suggestions.
29. As a user, I want mention autocomplete to avoid URL-like text, so that writing links does not trigger mention UI in the middle of a URL.
30. As a user, I want mention autocomplete to end at whitespace or invalid query punctuation, so that suggestions track only the intended token.
31. As a user, I want mention autocomplete to work at the start of a message, so that I can begin a message with a mention.
32. As a user, I want mention autocomplete to work after whitespace, so that mentions can be inserted naturally in sentences.
33. As a user, I want mention autocomplete to work after opening punctuation, so that mentions inside parentheses or after punctuation behave naturally.
34. As a user, I want mention autocomplete and emoji autocomplete to be mutually exclusive, so that competing menus do not appear at the same time.
35. As a user, I want mention autocomplete and the emoji picker to not conflict, so that there is only one active insertion surface.
36. As a user, I want Shift+Enter to keep inserting a newline when no autocomplete menu is open, so that multiline drafting remains unchanged.
37. As a user, I want Enter to submit a message only when autocomplete is closed, so that committing a mention does not accidentally send my draft.
38. As a user, I want committed mentions to insert durable markers rather than visible names in stored text, so that old messages keep pointing to the same person after a name change.
39. As a user, I want the editor to show inserted mentions as inline chips, so that I can visually confirm whom I am mentioning before sending.
40. As a user, I want mention chips to serialize back to `<@user_id>` markers on input, edit, and submit, so that the server receives a stable representation.
41. As a user, I want editing a message that already contains mentions to preserve those mentions, so that edits do not accidentally degrade semantic mentions.
42. As a user, I want known mention markers in edited text to appear as readable chips when user data is available, so that old mentions are understandable during editing.
43. As a user, I want unknown mention markers in edited text to remain safe and readable, so that missing cache data does not corrupt my draft.
44. As a message sender, I want JSON message sends containing mentions to return hydrated mention metadata, so that the UI can render the sent message immediately.
45. As a photo sender, I want multipart/photo message sends containing mentions to return hydrated mention metadata, so that caption mentions work like text-only message mentions.
46. As a thread participant, I want thread replies containing mentions to return hydrated mention metadata, so that thread mention rendering is immediate.
47. As a message editor, I want editing text to add new mention edges, so that the message metadata matches the current body.
48. As a message editor, I want editing text to remove stale mention edges, so that deleted mentions no longer highlight or preview users.
49. As a message reader, I want channel history responses to include mention metadata, so that previously sent mentions render correctly after reload.
50. As a message reader, I want thread root messages to include mention metadata, so that mentions in the message that spawned a thread render correctly.
51. As a thread reader, I want thread reply lists to include mention metadata, so that mentions inside replies render correctly.
52. As a thread browser, I want participated-thread previews to include mention metadata for roots and recent replies, so that preview content is consistent with full views.
53. As a real-time user, I want new-message SSE events to include mention metadata, so that live messages render mentions without a refresh.
54. As a real-time user, I want message-updated SSE events to include recalculated mention metadata, so that edited mentions update live.
55. As a real-time user, I want thread-reply-created SSE events to include mention metadata, so that live thread replies render mentions immediately.
56. As a message reader, I want mentions to render as highlighted inline names, so that they stand out from ordinary text.
57. As a message reader, I want mention labels to use display name when available and username as fallback, so that names match the rest of Hamlet's UI.
58. As a message reader, I want mention labels to include an `@` prefix, so that they are visually recognizable as mentions.
59. As a message reader, I want mention rendering to reflect current display-name and avatar data, so that old messages stay up to date when users change their public identity.
60. As a message reader, I want repeated mentions of the same user in one message to render every inline marker, so that the body text remains faithful.
61. As a message reader, I want repeated mentions of the same user to appear only once in message metadata, so that payloads stay compact and stable.
62. As a message reader, I want multiple distinct mentions to preserve first-appearance order in metadata where practical, so that rendering and previews are predictable.
63. As a message reader, I want malformed mention-looking text to remain ordinary text, so that normal prose is not unexpectedly rejected or transformed.
64. As a message sender, I want unsafe or nonexistent well-formed mention ids to be rejected, so that stored mention metadata remains valid.
65. As a message sender, I want a reasonable cap on unique mentions per message, so that accidental or abusive bulk mentions do not create excessive work.
66. As a message reader, I want messages with unknown or missing hydrated users to render a safe fallback rather than crash, so that bad data does not break the conversation.
67. As a message reader, I want mention rendering to coexist with custom emoji rendering, so that messages can contain both features.
68. As a message reader, I want mention rendering to coexist with safe link rendering, so that messages can contain mentions and URLs together.
69. As a message reader, I want deleted or tombstoned messages not to show body mentions, so that deleted content remains hidden.
70. As a user who is mentioned, I want messages mentioning me to have a visible row-level emphasis, so that I can quickly spot messages that need my attention.
71. As a user who is mentioned in a thread, I want thread replies mentioning me to be emphasized, so that mentions are visible outside main channel rows.
72. As a user who is mentioned in a thread preview, I want previews mentioning me to be emphasized, so that I can discover relevant thread activity.
73. As a user, I want authored-by-me styling and mentioned-me styling to remain distinguishable, so that I can tell why a row is emphasized.
74. As a user, I want inline mentions of me to be styled more strongly than other mentions, so that I can identify the exact mention in a message.
75. As a message reader, I want clicking a mention to open a compact preview, so that I can confirm who the mention refers to without leaving the conversation.
76. As a message reader, I want the mention preview to show avatar, display name/name fallback, and username, so that I can identify the user.
77. As a privacy-conscious user, I do not want mention previews or search results to expose email addresses, verification state, sessions, credentials, or raw avatar storage paths, so that private data stays private.
78. As a message reader, I want clicking another mention to switch the preview, so that I can inspect several mentions quickly.
79. As a message reader, I want Escape to close the mention preview, so that keyboard users can dismiss it easily.
80. As a message reader, I want clicking outside the mention preview to close it, so that it behaves like a lightweight popover.
81. As a message reader, I want the mention preview to close on unmount or scroll-away behavior, so that stale popovers do not linger.
82. As an assistive technology user, I want mention autocomplete exposed with listbox and option semantics, so that I can navigate suggestions accessibly.
83. As an assistive technology user, I want the editor to expose expanded state and active suggestion state, so that I know when mention suggestions are open.
84. As an assistive technology user, I want rendered mentions to have accessible names, so that I can understand whom the mention targets.
85. As an assistive technology user, I want mention previews to have accessible dialog or popover labeling, so that the preview content is understandable.
86. As an authenticated user, I want user search to require a valid session, so that public user directory data is not exposed anonymously.
87. As an authenticated user, I want empty user search to return a deterministic first page, so that the initial `@` menu is stable.
88. As an authenticated user, I want user search limits to be capped by the server, so that clients cannot request unbounded user lists.
89. As a developer, I want a shared public user response shape for search and mention hydration, so that privacy filtering is consistent.
90. As a developer, I want semantic mention edges stored separately from message text, so that future notification or unread features can query mentions efficiently.
91. As a developer, I want mention rows deleted when messages are hard-deleted, so that storage does not accumulate orphaned rows.
92. As a developer, I want tombstoned messages to tolerate existing mention rows, so that delete workflows remain simple and future audit use remains possible.
93. As a developer, I want mention parsing and ranking logic isolated in pure helpers, so that important edge cases are easy to test without UI setup.
94. As a developer, I want message response hydration to batch-load mention data, so that channel and thread views avoid N+1 queries.
95. As a developer, I want the test backend to model user search and mention hydration, so that client integration tests match the server contract.
96. As a developer, I want existing emoji, link, reply, attachment, reaction, thread summary, and SSE behavior preserved, so that mentions are additive rather than disruptive.

## Implementation Decisions

- Add direct user mentions only. Role mentions, group mentions, channel mentions, `@everyone`, and notification delivery are out of scope for this PRD.
- Use `<@123456789012345>` as the durable mention marker stored in message text. The marker stores only the mentioned user's id and never stores username or display name.
- Hydrate display names, usernames, and avatar URLs at render time from current user data, so historical messages reflect public identity changes without rewriting message bodies.
- Treat malformed marker-looking text as ordinary text on the client. Well-formed numeric markers extracted from submitted text are subject to server validation.
- Add an authenticated public user directory endpoint that returns an array of public user DTOs. The query defaults to an empty string, the limit defaults to ten, and the server caps limits to a small bounded range such as one through twenty-five.
- The public user DTO contains id, username, display name, and avatar URL only. It must not expose email, email verification state, credentials, sessions, or raw avatar storage paths.
- User search matches username and display name case-insensitively. Ranking is exact match first, then prefix match, then substring or fuzzy match, with deterministic ties by normalized username and id.
- Empty user search returns a deterministic first page sorted by public identity fields. The current user is included in results, and self-mentions are allowed.
- Add a semantic message mention persistence model that records one edge per mentioned user per message. The table includes a generated id, message id, user id, and creation timestamp.
- Enforce a uniqueness invariant for message/user mention pairs and maintain indexes by message/user and user/message to support hydration and future notification queries.
- Delete mention rows when a message is hard-deleted. Tombstoned message rows may keep mention rows, but clients should not render body mentions for deleted/tombstoned message bodies.
- Define a server-side cap such as fifty unique mentioned users per message. Reject create or edit requests whose unique mention count exceeds the cap.
- Reject submitted messages when a well-formed extracted marker id is non-positive, outside Hamlet's safe JavaScript-round-trippable id range, or references a nonexistent user.
- Reuse the existing invalid-request error style for mention validation failures unless later implementation needs a more specific public error code for test clarity.
- Extract a pure mention parser/validator deep module on the server. The parser extracts well-formed numeric `<@id>` markers in first-appearance order and deduplicates ids; validation checks id safety, existence, and cap limits.
- Validate all mentioned users with a batch query rather than per-id lookups. Preserve first-appearance order when returning hydrated mention users.
- Write mention rows during all message creation paths, including normal text sends, multipart/photo sends with text, and thread reply sends.
- On message edit, replace the message's mention rows with rows derived from the edited text so metadata tracks the latest body exactly.
- Perform message row writes and mention row writes in the same transaction where practical, so message text and semantic mention edges do not diverge.
- Extend full message responses with a `mentions` array. The array is always present and is empty when the message has no semantic mentions.
- Include mention metadata anywhere a full message payload is returned: channel history, channel sends, message edits, thread roots, thread replies, participated-thread previews, and real-time message events.
- Batch-hydrate mention metadata for message lists and thread preview collections to avoid N+1 query behavior.
- Preserve all existing message response enrichments while adding mentions, including author fields, attachments, embeds, reactions, thread summary data, and inline reply references.
- Add a client public user API wrapper and shared mention user type that matches the server public DTO.
- Extend the client message type so every message has a `mentions` array. Fixtures and builders should default this to an empty array.
- Add pure client mention marker helpers as a deep module. The helper parses text into mention/text tokens, detects the active autocomplete token, replaces the token with a durable marker and trailing space, and ranks users for local cache/server result presentation.
- Client autocomplete token detection requires a collapsed selection, activates on boundary-valid `@`, supports an empty query, and suppresses word-attached, email-like, and URL-like contexts.
- Client mention replacement operates on serialized editor text ranges rather than direct DOM mutations wherever possible. It returns the next serialized value and a caret position suitable for the editor to apply.
- Client-side user ranking mirrors the server ranking enough to keep cached or local result ordering predictable: exact and prefix matches before substring/fuzzy matches, across username and display name, with deterministic ties.
- Add a mention user cache/provider that exposes user search, caches users by id, primes the cache from message mention arrays, and ignores stale search responses.
- Mention search failure in the composer is quiet: it closes or shows no results rather than blocking typing or surfacing disruptive errors.
- Extend the shared contenteditable message editor rather than creating a separate composer. This ensures channel composition, message edit, thread composition, and thread reply edit paths all receive the same behavior.
- Reuse or lightly refactor existing autocomplete mechanics for selected index, keyboard handling, menu positioning, Escape suppression, and accessibility semantics.
- Mention autocomplete, emoji autocomplete, and the emoji picker are mutually exclusive. Opening one closes the others.
- Mention suggestion rows show avatar, display name fallback, and username. The first result is selected by default.
- While the mention menu is open, Enter and Tab commit the selected mention, Arrow keys move selection, Escape dismisses the mention menu first, and mouse click commits a mention.
- Inserted mentions render in the editor as inline mention chips and serialize back to durable `<@id>` markers on input, change, submit, and edit save.
- Editing existing message text deserializes known mention markers into chips when user data is available. Unknown markers remain safe readable text rather than throwing or corrupting the value.
- Centralize rendered message body handling into a shared rich text renderer that combines mention markers, custom emoji markers, and safe link rendering in one pass.
- The rich text renderer accepts message text, hydrated mentions, custom emoji resolution, optional current user id, and an optional mention-click handler.
- Rendered mention labels use `@display_name` when a display name is present and `@username` otherwise. Mentions are highlighted inline, and mentions of the current user receive stronger inline styling.
- If a marker has no matching hydrated user, the renderer displays the raw marker or a safe `@unknown user` fallback without throwing.
- Render mentions as interactive controls when a preview can be opened, with accessible labels such as `Mention Teo (@teo)`.
- Add a compact non-modal mention preview popover. It shows only public identity fields: avatar, display name/name fallback, and username.
- Mention previews close on outside click, Escape, unmount, scroll-away behavior, or when another mention is opened.
- Add row-level message emphasis for non-deleted messages whose hydrated mentions include the current authenticated user's id. The emphasis should be noticeable but not disruptive and distinct from authored-by-current-user styling.
- Prime the mention user cache from all message responses that include mention metadata so renderers and editors can resolve recently seen users without unnecessary searches.
- Update the client test backend to support public user search, public-only user data, mention parsing in sent and edited messages, mention arrays in returned messages, and SSE helper payloads that include mentions.
- No Electron IPC changes are required because normal Hamlet application behavior remains HTTP/SSE between the renderer and server.

## Testing Decisions

- Good tests should assert externally observable behavior: HTTP status and JSON shape, persisted/hydrated mention metadata, visible autocomplete rows, committed editor values, rendered inline mentions, highlighted rows, preview content, accessibility roles, and SSE-driven UI updates. Tests should not depend on private signal names, internal component state, or database implementation details beyond public schema/invariant behavior.
- Server tests should cover the authenticated user search API, including unauthenticated rejection, public-field-only responses, username and display-name matching, limit capping, and deterministic ordering.
- Server privacy tests should assert that public user search and mention hydration do not serialize email, email verification state, credential data, session data, or raw avatar storage paths.
- Server parser tests should cover no mentions, one mention, repeated mention deduplication, malformed marker text, unsafe ids, nonexistent users, and cap overflow.
- Server message creation tests should cover JSON text sends with mentions, multipart/photo sends with mention text, returned mention arrays, and SSE create events that include mentions.
- Server message history tests should verify that channel history hydrates mentions for multiple messages while preserving expected ordering and existing metadata.
- Server edit tests should verify that adding a mention inserts semantic rows, removing a mention deletes stale rows, and edit SSE events include recalculated mention arrays.
- Server thread tests should verify mention hydration in thread roots, replies, real-time thread reply events, and participated-thread previews.
- Server delete tests should verify hard-delete cleanup and tombstone tolerance.
- Server database setup tests should verify the mention table and uniqueness/index invariants through schema inspection or behavior.
- Client pure helper tests should cover marker tokenization, active `@` token detection, email/URL suppression, replacement with `<@id>` plus trailing space, and deterministic user ranking.
- Client API tests should verify that the user search wrapper calls the correct endpoint, serializes query and limit, and consumes the public user DTO.
- Message editor component tests should cover opening suggestions on `@`, filtering/ranking as the query changes, default selection, Enter and Tab commit, Arrow navigation, Escape dismissal precedence, mouse commit, emoji/mention mutual exclusion, Shift+Enter behavior, and marker serialization.
- Rich text renderer tests should cover mention display, current-user inline styling, mixed mentions/custom emoji/links, repeated markers, missing hydrated users, and deleted/tombstoned body behavior where applicable.
- Mention preview tests should cover opening on click, public-only content, avatar/name/username display, Escape close, outside-click close, switching between mentions, and accessible labeling.
- Message row tests should cover highlighting channel messages, thread replies, and thread previews that mention the current user, while leaving non-mentioned messages unhighlighted.
- Client integration tests using the test backend should cover sending, editing, and receiving SSE messages with mention arrays so the UI updates without reload.
- Accessibility tests should cover autocomplete listbox semantics, active descendant state, mention button accessible names, preview labels, and existing axe-covered component flows.
- Prior art for tests includes Hamlet's existing message editor tests for keyboard/form behavior, emoji autocomplete tests for token detection and suggestion behavior, custom emoji marker rendering tests, channel and thread integration tests, MSW-backed API tests, and server message lifecycle/SSE tests.
- Relevant server checks before completion are formatting, clippy with warnings denied, and the Rust test suite.
- Relevant client checks before completion are formatting, linting, typechecking, and the Vitest suite. Run browser or Electron E2E if implementation materially affects smoke-tested login, send-message, edit-message, or shell-launch flows.

## Out of Scope

- Push notifications, desktop notifications, unread mention counts, or mention inboxes.
- Role mentions, group mentions, channel mentions, `@everyone`, or server-wide broadcast semantics.
- Permission-scoped user visibility beyond requiring authentication for user search and message APIs.
- Full user profile pages or navigation from mention previews.
- Backfilling old plain-text `@name` messages into semantic mentions.
- Changing username registration or username validation rules unless required to avoid a blocking bug.
- Advanced full-text search infrastructure for users. SQLite-compatible matching is sufficient for the first slice.
- Electron IPC changes.
- Visual redesign of message rows beyond mention-specific inline and row-level emphasis.
- Notification-specific database workflows beyond the semantic mention edge table needed by this feature.

## Further Notes

- The semantic mention edge table intentionally prepares Hamlet for future mention notifications and unread queries, but this PRD does not require emitting notifications.
- The safest implementation order is: public user API, mention storage/parser, message create/edit/hydration integration, client API and fixtures, client helper/cache modules, composer autocomplete, rich text rendering and preview, current-user emphasis, and final regression checks.
- Server public user API work can proceed in parallel with client pure helper work. Rich text rendering can begin after message types and fixtures include mention arrays. Composer autocomplete depends on user search, provider/cache, and marker replacement helpers.
- Batch hydration is a key performance requirement for channel history, thread pages, and participated-thread previews.
- Renderer changes are the highest regression risk because they touch custom emoji, links, message edits, channel messages, and thread messages. Mixed-content tests should protect existing behavior while adding mentions.
