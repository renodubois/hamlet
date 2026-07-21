# User mentions support implementation plan

Date: 2026-06-18

## Goal

Add Discord-style user mentions to Hamlet messages.

Core behavior:

1. Typing `@` in any message editor opens a fuzzy-search list of users, similar to emoji autocomplete.
2. Selecting a user inserts a durable mention marker into the message text, not a mutable display name.
3. Rendered messages show mentioned users as highlighted inline names.
4. Clicking a rendered mention opens a small preview with the user's display name/name, username, and avatar.
5. Messages that mention the currently authenticated user are visually emphasized.

Non-goals for the first slice:

- Push/desktop notifications for mentions.
- Role, group, channel, or `@everyone` mentions.
- Permission-scoped user visibility beyond requiring authentication.
- User profile pages beyond the inline preview popover.
- Backfilling semantic mentions for old plain-text `@name` messages.

## Existing architecture

### Server

- HTTP routes are grouped under `server/src/api/*` and wired through `server/src/startup.rs`.
- All routes inside the default scope are protected by `require_auth`; public auth routes live separately.
- Current user identity is available via the `AuthUser` extractor.
- Users are stored in `server/src/entity/user.rs` with private fields such as `email` and avatar storage fields. `/me` currently returns the private/authenticated `UserResponse` shape.
- There is no public user directory or user search endpoint.
- Messages are plain text rows in `server/src/entity/message.rs`, with rich metadata hydrated in `server/src/api/messages.rs` into `MessageResponse`.
- `MessageResponse` already includes author public-ish fields (`username`, `display_name`, `avatar_url`) plus attachments, embeds, reactions, thread summary, and inline reply reference data.
- Message creates/updates are broadcast over SSE as `MessageResponse` payloads, so any mention metadata must be present in normal HTTP responses and SSE events.
- SQLite schema is initialized through SeaORM schema sync, with explicit integrity/index steps in `server/src/database.rs`.
- IDs are random 15-digit `i64`s and are intentionally safe to round-trip through JavaScript numbers.

### Client

- `client/src/components/message-input.tsx` is the shared `contenteditable` editor for:
  - channel composer (`client/src/pages/channel.tsx`),
  - channel message edits (`client/src/components/channel-messages.tsx`),
  - thread composer and thread message edits (`client/src/components/thread-panel.tsx`).
- The editor already serializes/deserializes custom emoji chips and has inline emoji autocomplete behavior.
- Emoji helpers live under `client/src/emoji/*`; custom emojis are provided by `client/src/contexts/custom-emojis.tsx`.
- Rendered message text with custom emoji support is currently handled in message display components rather than one central rich-text renderer.
- Link rendering is centralized in `client/src/linkify.ts`, but mention parsing does not exist.
- Auth state and the current user live in `client/src/contexts/auth.tsx` and are consumed by channel/thread pages.
- API exports are centralized in `client/src/api/index.ts`.
- MSW handlers in `client/src/test/msw/handlers.ts` model the client integration-test backend.

## Data/API contract

### Durable text marker

Use the marker format:

```text
<@123456789012345>
```

Rules:

- The marker stores only the mentioned user's id.
- Display names/usernames are never written into message text by autocomplete.
- The visible name is hydrated at render time from current user data, so display-name/avatar changes are reflected in old messages.
- Malformed markers remain ordinary text on the client.
- Server-created/edited messages validate well-formed mention markers before accepting the text.

### Public user DTO

Add a shared public user response shape for search results and message mention hydration:

```json
{
  "id": 123456789012345,
  "username": "teo",
  "display_name": "Teo",
  "avatar_url": "/uploads/avatars/..."
}
```

This shape must not include `email`, `email_verified`, credential data, session data, or raw avatar storage paths.

### User search endpoint

Add an auth-gated endpoint:

```http
GET /users?query=te&limit=10
```

Behavior:

- Requires a valid session cookie.
- Returns `PublicUserResponse[]`.
- `query` defaults to the empty string.
- `limit` defaults to 10 and is capped, e.g. `1..=25`.
- Search `username` and `display_name` case-insensitively.
- Ranking should be stable and useful for autocomplete:
  1. exact username/display-name matches,
  2. prefix matches,
  3. substring/fuzzy matches,
  4. deterministic tie-break by normalized username then id.
- Empty query returns a deterministic first page, sorted by username/display name.
- Include the current user in results; self-mentioning is allowed.

### Message response mention metadata

Extend `MessageResponse` and client `Message` with:

```json
{
  "mentions": [
    {
      "id": 123456789012345,
      "username": "teo",
      "display_name": "Teo",
      "avatar_url": "/uploads/avatars/..."
    }
  ]
}
```

Rules:

- Always return an array; use `[]` when there are no mentions.
- Preserve first-appearance order from message text where practical.
- Deduplicate repeated mentions of the same user within one message.
- Include `mentions` anywhere a full message is returned:
  - `GET /messages/{channel_id}`,
  - `POST /message/{channel_id}` JSON and multipart/photo sends,
  - `PUT /message/{message_id}`,
  - thread root/replies endpoints,
  - participated-thread previews,
  - SSE `message`, `message_updated`, and `thread_reply_created` payloads.

### Mention storage

Add a `message_mention` join table so the server stores semantic mention edges independently of text rendering.

Suggested columns:

- `id: i64` primary key, generated with the existing id utility.
- `message_id: i64`.
- `user_id: i64`.
- `created_at: i64`.

Suggested indexes/invariants:

- Unique `(message_id, user_id)` to enforce dedupe.
- Index `(message_id, user_id)` for hydration.
- Index `(user_id, message_id)` for future unread/notification queries.

Deletion behavior:

- When a message is hard-deleted, delete its `message_mention` rows in the same operation.
- When a message is tombstoned, rows may remain; the client should not render body mentions for deleted/tombstoned message bodies.

Validation/caps:

- Define a server cap such as `MAX_MESSAGE_MENTIONS = 50` unique users per message.
- Reject create/edit when:
  - a marker id is not a safe positive user id,
  - a marker references a nonexistent user,
  - the unique mention count exceeds the cap.
- Use the existing generic invalid-request error unless a more specific error is needed for tests.

## Backend tasks

### 1. Add public user search API

Files:

- New: `server/src/api/users.rs`.
- Update: `server/src/api/mod.rs`.
- Update: `server/src/startup.rs`.

Tasks:

- Define `PublicUserResponse` and convert from `entity::user::Model` using existing avatar URL helper logic.
- Define `UsersQuery { query: Option<String>, limit: Option<u64> }`.
- Implement `GET /users` with auth required by registering it inside the existing authenticated scope.
- Keep query matching SQL-portable enough for SQLite:
  - trim query,
  - lowercase query in Rust and use SQLite `LOWER(...) LIKE ...`, or use SeaORM expressions,
  - apply deterministic ordering.
- Ensure the response never serializes private `UserResponse` fields from `/me`.

### 2. Add message mention entity and indexes

Files:

- New: `server/src/entity/message_mention.rs`.
- Update: `server/src/entity/mod.rs`.
- Update: `server/src/entity/message.rs` and `server/src/entity/user.rs` with `has_many` relationships if useful.
- Update: `server/src/database.rs` managed index list.

Tasks:

- Create SeaORM entity for `message_mention`.
- Add relationships to message and user entities.
- Add managed indexes/unique constraints:
  - `ux_message_mention_message_user`,
  - `idx_message_mention_message_user`,
  - `idx_message_mention_user_message`.
- Add migration/integrity test coverage for schema creation and index existence.

### 3. Add mention parsing and validation helpers

Files:

- Update: `server/src/api/messages.rs`, or add focused module such as `server/src/mentions.rs` if the logic grows.

Tasks:

- Implement a pure parser for `<@id>` markers.
- Return marker ids in first-appearance order and dedupe them.
- Reject ids that are non-positive or above `MAX_SAFE_MESSAGE_ID`.
- Validate that all mentioned users exist in one batch query.
- Preserve first-appearance order in the returned public-user list.
- Add unit tests for parser behavior:
  - no mentions,
  - one mention,
  - repeated mention deduped,
  - malformed marker ignored or rejected according to parser/validator split,
  - unsafe id rejected,
  - cap exceeded rejected.

Recommended split:

- Parser: extracts well-formed numeric markers and ignores malformed text.
- Validator: rejects invalid extracted ids, missing users, and cap violations.

### 4. Write mention rows during create/edit

Files:

- Update: `server/src/api/messages.rs`.

Tasks:

- On normal channel message create, parse/validate mentions before committing the message response.
- On multipart/photo message create, apply the same logic to the text field.
- On thread reply create, apply the same logic.
- On message edit, replace existing mention rows with rows derived from the edited text.
- Perform message row and mention row changes in the same transaction where practical.
- On hard delete, delete mention rows for that message.
- Ensure tombstone updates do not fail because of stale mention rows.

### 5. Hydrate mentions in all message response paths

Files:

- Update: `server/src/api/messages.rs`.
- Potential helper extracted inside the same module to avoid N+1 queries.

Tasks:

- Extend `MessageResponse` with `mentions: Vec<PublicUserResponse>`.
- For single-message responses, load mention users and attach them.
- For list responses, batch-load all mention rows for the returned message ids, then batch-load all users.
- Use the same hydration for:
  - channel history,
  - create/edit responses,
  - thread root/reply responses,
  - participated-thread previews,
  - SSE broadcast payloads.
- Keep existing attachment/embed/reaction/reply/thread-summary hydration intact.
- Ensure message update events include the recalculated `mentions` array.

### 6. Server error and regression cleanup

Files:

- Update: `server/src/error.rs` only if a specific mention error improves API clarity.
- Update tests under `server/src/api/messages.rs` or existing integration-style test modules.

Tasks:

- Prefer reusing `AppError::InvalidRequest` for invalid mention text unless tests require distinguishing `unknown_mention_user`.
- Add helper assertions that private user fields are absent from public user responses.
- Ensure seed/dev users work with `/users` immediately.

## Frontend tasks

### 1. Add user API and public user type

Files:

- New: `client/src/api/users.ts`.
- Update: `client/src/api/index.ts`.
- Update: `client/src/api/messages.ts`.

Tasks:

- Add `PublicUser` or `MentionUser` type matching the server public DTO.
- Add `listUsers({ query, limit })` using `apiFetch`.
- Extend `Message` with `mentions: MentionUser[]`.
- Ensure all message fixtures/builders include `mentions: []` by default.

### 2. Add mention marker/search helpers

Files:

- New: `client/src/mentions/mention-markers.ts`.
- New: `client/src/mentions/mention-search.ts`.

Tasks:

- Implement `parseMentionMarkers(text)` returning text/mention tokens for `<@id>` markers.
- Implement `findMentionAutocompleteToken(value, selection)`:
  - active only for collapsed selections,
  - starts at a boundary-valid `@`,
  - supports empty query so typing `@` opens the list,
  - ends at whitespace or punctuation that cannot be part of the query,
  - ignores email addresses and ordinary words like `hello@host`,
  - suppresses inside URL-looking text.
- Implement `replaceMentionAutocompleteToken(value, token, user)` returning text with `<@id>` marker and a trailing space.
- Implement `rankMentionUsers(query, users, limit)` for client-side ranking of server results/cache:
  - exact/prefix before substring/fuzzy,
  - search both username and display name,
  - deterministic ties.
- Unit-test marker parsing and token/ranking behavior independently of React components.

### 3. Add mention user cache/provider

Files:

- New: `client/src/contexts/mention-users.tsx`.
- Update: `client/src/App.tsx` or the authenticated app shell to mount the provider.

Tasks:

- Provide an on-demand `searchUsers(query, limit)` function for autocomplete.
- Cache users by id so message renderers and editor chips can resolve recently seen mentions.
- Expose a `primeUsers(users)` helper so message responses can seed the cache from `message.mentions`.
- Handle request races by ignoring stale autocomplete results.
- Keep failure behavior quiet in the menu: if search fails, close or show no results rather than blocking typing.

### 4. Extend `MessageInput` for `@` autocomplete and chips

Files:

- Update: `client/src/components/message-input.tsx`.

Tasks:

- Reuse/refactor existing emoji autocomplete mechanics for mention autocomplete:
  - selected index,
  - keyboard handling,
  - menu positioning above the editor,
  - `aria-controls`, `aria-expanded`, `aria-activedescendant`, listbox/option semantics,
  - Escape suppression for the active token session.
- Ensure emoji picker/autocomplete and mention autocomplete are mutually exclusive.
- Trigger mention search when typing `@` or editing the query after `@`.
- Show rows with avatar, display name fallback, and username.
- Default-select the first result.
- Keyboard behavior while mention menu is open:
  - `Enter` commits selected mention,
  - `Tab` commits selected mention,
  - `ArrowDown`/`ArrowUp` move selection and wrap,
  - `Escape` dismisses mention autocomplete before edit-cancel behavior,
  - mouse click commits a mention.
- Insert a serialized `<@id>` marker and render it in the editor as an inline mention chip.
- Serialize mention chips back to markers on input/change/submit.
- When editing an existing message, deserialize existing `<@id>` markers into chips if user data is available; otherwise leave readable fallback text.
- Keep multiline behavior unchanged: `Shift+Enter` inserts newline only when no autocomplete menu is open.

### 5. Centralize rich message rendering

Files:

- New: `client/src/components/message-rich-text.tsx`.
- Update: `client/src/components/channel-messages.tsx`.
- Update: `client/src/components/thread-panel.tsx`.
- Update: `client/src/pages/threads.tsx` if it renders message bodies/previews.

Tasks:

- Create a component that takes:
  - `text`,
  - `mentions`,
  - custom emoji resolver,
  - optional `currentUserId`,
  - optional mention-click handler.
- Render in one pass without losing current behavior:
  - custom emoji markers as images,
  - URLs as safe links using `linkifyText`,
  - mention markers as inline buttons/spans.
- Mention rendering:
  - visible label `@${display_name ?? username}`,
  - highlighted styling for all mentions,
  - stronger styling when the mentioned id equals `currentUserId`,
  - accessible label such as `Mention Teo (@teo)`.
- If a marker has no matching hydrated user, render the raw marker or an unclickable `@unknown user` fallback without throwing.
- Replace duplicated custom emoji rendering in channel/thread components with this shared renderer.

### 6. Add mention preview popover

Files:

- New: `client/src/components/user-mention-preview.tsx` or colocate with rich text component.
- Update: message display components to own popover state.

Tasks:

- Clicking a mention opens a compact popover near the clicked mention.
- Popover content:
  - avatar via existing `Avatar` component,
  - display name/name fallback,
  - `@username`,
  - no private fields.
- Close on outside click, Escape, scroll away/unmount, or clicking another mention.
- Use button semantics for mentions and accessible dialog/popover labeling.
- Keep the preview small and non-modal.

### 7. Emphasize messages mentioning current user

Files:

- Update: `client/src/components/channel-messages.tsx`.
- Update: `client/src/components/thread-panel.tsx`.
- Update: `client/src/pages/threads.tsx` if it shows full message rows.

Tasks:

- Compute `messageMentionsCurrentUser(message, currentUserId)` from `message.mentions`.
- Apply a row-level emphasis style to non-deleted messages mentioning the current user.
  - Example: subtle yellow/blue background, left border, or ring.
  - Avoid making authored-by-current-user styling ambiguous; mention emphasis should be independently visible.
- Pass `currentUserId` into all rich text renderers so the inline mention chip can also be emphasized.
- Ensure thread replies and thread preview messages honor the same current-user highlight.

### 8. MSW and fixtures

Files:

- Update: `client/src/test/msw/handlers.ts`.
- Update test builders/fixtures as needed.

Tasks:

- Add `GET /users` handler with public fields only.
- Store users with avatars/display names in the in-memory MSW database.
- Parse `<@id>` markers in sent/edited messages and attach `mentions` arrays to returned messages.
- Update message edit/send/thread handlers so responses match the server contract.
- Ensure SSE helpers can push messages with `mentions`.

## Testing plan

### Server tests

Add or extend tests for:

1. `GET /users`:
   - unauthenticated requests return 401,
   - authenticated requests return public user fields,
   - email/email verification/raw avatar path do not appear,
   - query matches username and display name,
   - limit is capped and ordering is deterministic.
2. Mention parser/validator:
   - extracts marker ids,
   - dedupes repeated ids,
   - rejects unsafe ids,
   - rejects nonexistent users,
   - rejects cap overflow.
3. Message create:
   - JSON send with `<@id>` returns `mentions`,
   - multipart/photo send with `<@id>` returns `mentions`,
   - SSE create event includes `mentions`.
4. Message list/history:
   - `GET /messages/{channel_id}` hydrates mentions for multiple messages without order loss.
5. Message edit:
   - adding a mention inserts join rows and returns them,
   - removing a mention deletes stale join rows,
   - edit SSE includes recalculated mentions.
6. Thread flows:
   - thread root/replies include mentions,
   - `thread_reply_created` SSE includes mentions,
   - participated-thread previews include mentions in root and recent replies.
7. Delete flows:
   - hard-delete removes mention rows,
   - tombstoned messages do not break mention hydration.
8. Database setup:
   - `message_mention` table exists after initialization,
   - unique/index constraints exist or are behaviorally verified.

Run from `server/`:

```bash
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

### Client tests

Add or extend tests for:

1. Pure helpers:
   - mention marker parsing,
   - mention autocomplete token detection,
   - mention replacement with `<@id>` marker,
   - user ranking/fuzzy matching.
2. `MessageInput` autocomplete:
   - typing `@` opens user suggestions,
   - typing query filters/ranks suggestions,
   - Enter/Tab commit selected mention,
   - arrows move selection,
   - Escape dismisses without canceling edit until a second Escape,
   - mouse click commits,
   - mention autocomplete and emoji autocomplete/picker are mutually exclusive,
   - committed mention serializes to `<@id>`.
3. Rich text rendering:
   - mentions render as highlighted names,
   - current-user mention gets stronger inline styling,
   - custom emoji and links still render correctly with mentions in the same text,
   - unknown/missing mention user fallback does not throw.
4. Mention preview:
   - click opens popover with name, username, avatar,
   - no email/private fields are rendered,
   - Escape/outside click closes it.
5. Message row emphasis:
   - channel message mentioning current user is highlighted,
   - non-mentioned message is not highlighted,
   - thread replies and thread previews behave consistently.
6. API/MSW/integration:
   - `/users` client helper calls the right endpoint,
   - send/edit message responses carry mentions,
   - SSE message/update events with mentions update rendered state.
7. Accessibility:
   - autocomplete listbox semantics remain valid,
   - mention buttons/popovers have accessible names,
   - run existing axe-covered component tests where applicable.

Run from `client/`:

```bash
npm run fmt
npm run lint
npm run typecheck
npm run test
```

Run `npm run test:e2e` if the final UI changes affect smoke-tested login/message-send flows or if the implementation touches Electron shell behavior.

## Rollout/order

Implement in dependency-safe slices:

1. **Server public users API**
   - Add `PublicUserResponse` and `GET /users`.
   - Test auth, search, and private-field leakage.
   - This can ship independently and enables client autocomplete work.

2. **Server mention storage/parser**
   - Add `message_mention` entity/table/indexes.
   - Add parser/validator tests.
   - No client-visible behavior yet except schema readiness.

3. **Server message integration**
   - Write mention rows on create/edit/delete.
   - Extend `MessageResponse` with `mentions` everywhere.
   - Update SSE and thread/participated flows.
   - Update server tests for message lifecycle behavior.

4. **Client API/MSW foundation**
   - Add user API, mention types, message `mentions`, fixtures, and MSW `/users` support.
   - Keep UI unchanged except type-compatible defaults.

5. **Client helper modules and provider**
   - Add marker parsing/search/ranking helpers with unit tests.
   - Add mention user provider/cache and mount it.

6. **Composer autocomplete**
   - Extend `MessageInput` to support `@` autocomplete and mention chips.
   - Add component tests for keyboard/mouse behavior.

7. **Rich text rendering and preview**
   - Centralize text rendering for custom emojis, links, and mentions.
   - Add mention preview popover.
   - Replace duplicated render paths in channel/thread views.

8. **Current-user message emphasis**
   - Add row-level highlight in channel/thread views.
   - Add integration tests using current auth user and MSW messages.

9. **Final regression pass**
   - Run server/client check suites.
   - Run E2E if message send/edit smoke coverage is affected.
   - Verify manually: send mention, edit mention, thread mention, current-user highlight, click preview, and old messages.

Parallelization notes:

- Server users API can be built in parallel with client pure helper modules.
- Server mention storage/parser should land before message response contract changes.
- Client rich text rendering can start after `Message.mentions` type and fixtures exist.
- Composer autocomplete depends on user API/provider and marker replacement helpers.

## Risks/open decisions

1. **Search semantics vs. scale**
   - SQLite `LIKE` is sufficient for the current app size. If user count grows, introduce normalized search columns or FTS.

2. **Username validation is loose today**
   - Registration currently permits broad username strings. Mention markers avoid storing usernames, but autocomplete token parsing will work best with simple typed queries. Do not expand username validation as part of this feature unless necessary.

3. **Malformed manual markers**
   - Proposed decision: malformed markers are plain text; well-formed numeric markers that reference missing/unsafe users are rejected on create/edit. This keeps storage semantic without punishing ordinary `<@not a marker>` text.

4. **Mention rows for tombstoned messages**
   - Keeping rows on tombstone is harmless and may help future audit/notification features. The UI should not render body mentions for deleted content.

5. **N+1 hydration risk**
   - Batch hydration is required for message lists, thread pages, and participated-thread previews. Avoid per-message user lookups.

6. **Renderer regression risk**
   - Centralizing text rendering touches links, custom emojis, message edits, channel messages, and thread messages. Preserve existing custom emoji/link tests and add mixed-content cases.

7. **Autocomplete interaction complexity**
   - `MessageInput` already has emoji autocomplete and picker behavior. Refactor shared autocomplete mechanics only as much as needed; keep changes incremental and test Escape/Enter/Tab precedence carefully.

8. **Current-user emphasis styling**
   - The highlight should be noticeable but not disruptive. Prefer a subtle row background/border and stronger inline mention chip over loud colors.

9. **Future notification use**
   - The join table intentionally supports future mention inbox/notification features, but this plan does not emit notifications or unread counts.
