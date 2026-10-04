# Live updates implementation plan

Status: core design and deliberately simple desktop integration agreed; ready for server-first implementation. No implementation changes have been made. Decisions below are settled; file layout and tuning defaults are implementation proposals, subject to verification.

## Goal

Replace periodic client polling with server-originated updates. Design the server contract first, then the desktop client integration. Adding event types should be straightforward; future non-desktop clients should not require a redesign.

## Resolved decisions

### Recovery: rebuild current state, not replay every change

Accurate current state is sufficient. A client disconnected while a message is created and then deleted need not learn that the message existed.

Use live delivery with authoritative reads on initial connection, reconnect, or detected delivery loss. Do not require a durable event log or replay of missed events in the initial design. The database remains authoritative; this is not event sourcing.

Recovery must remove stale data rather than merely append newly discovered messages. In v1, discard previously loaded server history and reload the selected channel's newest page; do not retain old pages as synchronized data.

A silently missed update while the client still considers itself synchronized is unacceptable. Initial read/stream handoff, mutation/publication coupling, slow-consumer handling, and gap detection require explicit designs.

### Event payloads: direct updates initially

Initial events describe completed changes and carry enough data to apply them without follow-up reads: `MessageCreated` contains the complete message and `ChannelCreated` contains the complete channel. When a separate message-deletion feature is implemented, its expected event shape is `MessageDeleted` containing channel/message IDs; do not ship that unused variant in v1.

Use a concrete event enum as the starting design. Do not add an invalidation flag, unused invalidation variants, or a generic invalidation framework. If a future feature genuinely benefits from invalidation (for example, bulk moderation), add a specific event variant and its client handler then. Reconnect recovery uses authoritative reads independently of event types.

### Subscription scope: all accessible channels

Each client session uses one stream covering changes across all channels that its authenticated user can access. Channel switching does not change the subscription. Receiving a message event does not require loading or retaining that channel's history; clients can update already-loaded histories and ignore unloaded ones.

This favors simple navigation and leaves room for unread indicators over minimizing traffic to the selected channel. All users currently have access to all channels. Future authorization must restrict server-side delivery; client-side filtering is not an access-control boundary. Authorization changes during an open stream remain to be designed.

### Transport: standard SSE with bearer authentication

Use standard SSE framing over the existing bearer-authenticated HTTP API. Client writes remain ordinary HTTP requests. Do not add WebSocket support, cookie authentication, or browser-specific endpoints in v1.

The stream needs heartbeats, reconnect handling, and explicit recovery after gaps. Standard SSE framing enables future clients; native browser `EventSource` authentication convenience is not a v1 requirement. A future browser client can use streaming `fetch` with bearer authentication and an SSE parser, subject to browser deployment/origin policy.

### Recovery UX: reset to newest messages in v1

On recovery, discard cached server history and reload the selected channel's newest page, along with the authoritative channel list. Preserve local drafts and channel selection if the selected channel still exists and remains accessible. Inactive histories are discarded and load normally when selected.

Preserving older loaded pages and reading position is deferred polish, not a v1 requirement. No loaded-range refresh endpoint or reconnect-specific scroll restoration is required for v1. Correct stream/read coordination is still required so a refresh cannot overwrite newer events or miss changes during loading.

### Desktop refresh: remove manual refresh controls

Once live updates are implemented, remove both **Refresh conversation** and **Refresh channels**. They were temporary polling-era controls, not permanent product behavior. Healthy streams apply changes directly; failed streams recover automatically through authoritative reads. Do not retain a user-triggered catch-up workflow solely to support those controls. Older-page loading and retries remain distinct from manual refresh. Connection recovery is automatic only: do not add a **Retry connection** action. While disconnected or recovering with retained content, show a persistent notice that reconnection is underway and messages may be out of date; retain that notice until baseline synchronization succeeds.

### Desktop v1: one recovery path, minimal failure UX

Prioritize making live updates usable over designing comprehensive connection-failure states. Use one conversation-wide automatic recovery path, not independent resource retry policies:

- Connect, wait for `ready`, load the channel list and current selection's newest history, reconcile buffered creations, then process events live.
- A stream failure or a failed required channel/newest-history baseline abandons the attempt and retries the complete sequence with bounded backoff. This includes failure to load an uncached channel selected while the stream was healthy; accept the resulting global reset rather than adding a separate history-retry scheduler.
- Keep existing content visible as stale while recovering where available. Show a basic persistent **Reconnecting… messages may be out of date** notice; initial loading without retained content can show **Connecting…**. No manual refresh or connection-retry controls.
- Ordinary channel selection does not restart a healthy stream. Keep navigation, drafts, and authenticated HTTP writes available during recovery. Existing older-page failures keep their local retry behavior and do not by themselves restart the stream. Uncertain writes do not trigger recovery.
- Keep authoritative authentication rejection and known expiry on the existing session-ending path. Other connection failures do not log the user out.

Keep the correctness safeguards even in this simple version: readiness/read/event coordination, entity-ID deduplication and ordering, bounded buffering, cancellation and obsolete-result rejection, and preservation of local drafts/write outcomes. Simplification means fewer policies, not silently continuing after lost updates.

Defer detailed connection-error categories and tailored UI, per-resource automatic retry machinery, recovery progress indicators, user-triggered connection retries, and preservation of older history/reading position across recovery. Revisit these after using the working feature; they are not prerequisites for v1. Parser, buffer, authentication, and session-safety checks remain required.

### Recovery navigation: follow the current selection

Channel navigation remains available during recovery. If selection changes while the selected-history baseline is loading, cancel/invalidate that read and load the new selection's newest page within the same connection attempt. Do not reconnect the stream or reload an already-successful channel baseline merely because selection changed. Continue bounded event buffering; late history completions for previous selections cannot replace current state or complete recovery. Declare recovery successful only when the current selection's baseline and buffered changes have been reconciled (or the authoritative channel list is empty).

### Feature scope: creation events; deletion is separate

V1 publishes updates for existing message creation and channel creation. Message deletion is a separate feature, including its permissions, operation, HTTP endpoint, UI, and event variant. Documenting the expected deletion payload does not add it to the initial executable contract.

### Shared wire types: small Rust protocol crate

Add a small shared protocol crate for the event enum and its message/channel wire types (`Message`, `Author`, `Channel`, and `ChannelType`). Both server events and existing message/channel HTTP responses use these definitions without changing existing HTTP JSON shapes. Authentication types, request types, and pagination wrappers do not need to move for this feature.

Keep client-facing state types and wire-to-client validation/conversion in `client/src/api/`. Keep database decoding, including the existing SeaORM-dependent channel-type parsing, on the server. The shared crate contains serialization definitions and optional server-side OpenAPI support, not database access, HTTP execution, GPUI, or session workflows. No Cargo workspace is required solely to share the crate.

The documented SSE/JSON contract remains language-neutral and authoritative for non-Rust clients. A Rust crate is optional consumer tooling, not a prerequisite for implementing a client. Extend OpenAPI for the endpoint/payloads and document framing, recovery sequencing, compatibility rules, and examples separately under `llm-docs/`. Shared source definitions do not replace compatibility tests for separately released server/client versions or actual stream framing tests.

### Open-stream authentication: periodic validation

Authenticate the handshake using existing bearer middleware, close the stream at its known session expiry, and revalidate the session on the server approximately every 15 seconds. Stop further delivery when revalidation is due until it succeeds. Missing/revoked/expired sessions terminate the stream; database validation failures also terminate it but are not authoritative credential rejection.

Local desktop logout cancels its stream immediately. Server-side revocation has an approximately 15-second validation window rather than immediate cancellation. Do not add a session-to-stream cancellation registry in v1. Extract reusable validation from the existing middleware instead of maintaining separate authentication logic. Already-sent bytes cannot be recalled.

### Desktop focus: keep the stream connected

The stream remains connected while the desktop window is unfocused, provided the session remains active. Focus changes alone do not trigger reconnect or history reset. Suspend, network failure, stream termination, or detected delivery loss still require recovery. This replaces the existing focus-paused polling policy.

### Deployment: one write-owning server process

V1 requires one server process to own all conversation writes. A single in-memory event hub is constructed in shared application state and shared across Actix workers, not instantiated independently per worker. There is no external broker or persisted event log. Multiple write-owning server processes and external database writers are unsupported; adding event types does not change this deployment constraint.

### Initial sizing target: busy community

Plan for approximately 10 community-wide changes per second at peak. This is a sizing target, not an enforced rate limit or verified performance guarantee. Every stream receives all accessible-channel changes, so fanout bandwidth scales with connected clients. Buffer capacities require measurement and tuning.

### Slow consumers: bounded buffers and reset on overflow

Use bounded buffering on both server delivery and client recovery paths. On overflow or detected lag, terminate/discard the affected stream attempt and reconnect with authoritative reload. Never silently skip events and resume as synchronized, grow buffers indefinitely, or delay mutation completion behind slow clients.

A shared bounded broadcast buffer is the server starting design; avoid duplicating retained serialized payloads per subscriber. A capacity of 256 events is an illustrative initial tuning candidate (about 25 seconds of backlog at the peak target), not a performance guarantee. Account separately for transport buffering and enforce client frame/body limits.

### Unknown events: ignore in v1, revisit before expanding the contract

V1 clients ignore unknown event types. This is a provisional policy chosen to avoid premature compatibility machinery while the client and feature set are small; do not implement reload-on-unknown or upgrade-required behavior now.

Ignoring unknown changes is not a general guarantee of accurate state across independently released versions. Before adding events that alter already-supported state (for example, message deletion), revisit compatibility: an older client ignoring those events could retain stale messages. High-frequency or unrelated future events also require deliberate treatment. The language-neutral contract must state the provisional policy rather than promise unlimited additive compatibility.

### Initial loading and recovery: stream first, then existing HTTP reads

Keep SSE changes-only, with a `ready` control frame; do not send channel/history snapshots inside the stream. The server registers the subscription before emitting `ready`. The client waits for `ready`, buffers subsequent changes, fetches the authoritative channel list and selected channel's newest history using existing HTTP endpoints, applies the buffered changes with entity-ID deduplication, then continues live processing.

If the stream fails, buffers overflow, or baseline reads fail before recovery completes, do not declare synchronization successful. Discard stale attempt results and retry with bounded backoff. Authentication rejection remains a session-lifecycle outcome, not a generic retry.

Reads and event application must also coordinate during channel selection, older-page loads, and write-response reconciliation; old read completions must not overwrite live state. Preserve originating session/request identity checks. Details remain to be specified. V1 state changes are creation-only; this handoff must be revisited for mutation types whose buffered events can conflict with snapshots.

### Wire framing: ready/change SSE frames and tagged JSON

Each connection starts with `event: ready` and JSON `data: {}` after subscription registration. Application updates use `event: change`; their JSON data is the shared event enum, internally tagged by `type` with snake-case names, for example `{"type":"message_created","message":{...}}` and `{"type":"channel_created","channel":{...}}`.

Heartbeats are SSE comments. Do not emit resumable event IDs or implement replay from `Last-Event-ID`; every new connection uses the ready/read/reconcile sequence. Ignore additional JSON fields and unknown event types in accordance with the provisional compatibility policy. A malformed known event or invalid known framing terminates the attempt and triggers recovery, rather than silently losing a supported update.

### Writes remain available during stream recovery

Ordinary authenticated HTTP writes remain available while the live stream is disconnected or recovering. Confirmed HTTP responses update local state; stream events arriving before or after those responses are deduplicated by entity ID. An uncertain response remains uncertain; do not match message text to infer confirmation and never automatically retry writes.

Report that loaded live state may be stale while disconnected. Preserve current session, draft, and write-uncertainty semantics; a stream outage alone is not session invalidation.

### Uncertain sends: no extra history reconciliation

A send timeout or other uncertain write outcome does not itself trigger a history read or stream restart. If the stream remains healthy, continue applying its events normally; if it fails, normal automatic recovery applies. Preserve the draft and uncertainty warning. Neither an event nor a history read identifies the originating request by matching message text, and neither clears that uncertainty. Never automatically resend. Remove the polling-era uncertain-send catch-up scheduling rather than preserving it as a second synchronization mechanism.

### Publication: simple write, then notify

Use a concrete event hub in shared server application state. Its `notify(Event)` synchronously enqueues the completed change in bounded broadcast delivery; it does not await individual subscriber/network delivery. Feature operations own event selection and call notification after successful database writes. Handlers do not define publication semantics. No event trait, plugin registry, generic mutation coordinator, or global write gate is required for v1.

Independent creation events do not require broadcast order to equal database commit order. Clients deduplicate by entity ID and use the server's message ordering (`created_at DESC, id DESC`), not arrival order. Revisit ordering guarantees before introducing dependent edits/deletions.

Fix the existing message operation's insert-then-author-lookup gap: an unsuccessful lookup after a committed insert must not silently prevent notification. Prefer preparing the response/event data before writing, or use a transaction if reads are needed to produce it.

Task cancellation can occur while awaiting database work; a write might commit even if its awaiting future is dropped. It does not arbitrarily interrupt straight-line synchronous notification after an await returns. Verify request/task cancellation behavior during implementation and use narrowly scoped write-task lifetime protection if needed to preserve notification after commit; do not introduce a generic mutation framework. Unexpected post-commit task/publication failure must not leave apparently synchronized streams missing supported changes.

A process crash after a write may lose its notification; all streams end and clients reload after reconnect. This is accepted and does not require durable delivery.

## Proposed implementation layout

Paths in this section are relative to the repository root. New paths describe intended implementation outputs, not existing files.

| Area | Home | Responsibility |
| --- | --- | --- |
| Shared wire definitions | `protocol/Cargo.toml`, `protocol/src/lib.rs` | A small `hamlet-protocol` crate: event enum and message/channel payloads; Serde and optional OpenAPI support. |
| Server stream feature | `server/src/live_updates/` | Concrete event hub, subscriptions, SSE handler/framing, heartbeat/authentication lifecycle. Start small; split cohesive files only where useful. |
| Server wiring | `server/src/lib.rs`, `server/src/main.rs` | Construct one hub with shared state before the Actix worker factory; register the protected route. |
| Publication | `server/src/messages/operations.rs`, `server/src/channels/operations.rs` | Write successfully, then notify with a completed event. |
| Authentication | `server/src/http/auth.rs` | Reusable session lookup, including expiry; preserve existing HTTP error distinctions. |
| OpenAPI and inventory | `server/src/contract.rs`, `server/openapi.json`, `server/tests/contract.rs` | Endpoint, payload schemas, bearer protection, method/status coverage, and streaming-aware representative tests. |
| Client stream transport | `client/src/api/events.rs`, `client/src/api/client.rs` | Bound authenticated connection, incremental SSE parsing, validation, stream-specific deadlines, controlled transport seam. |
| Client coordination | `client/src/conversation/mod.rs`, a focused `client/src/conversation/live_updates.rs` if useful | One stream per conversation/session lifetime; readiness, recovery, bounded delivery/buffering, attempt identity, reconnect policy. |
| Client transitions | `client/src/conversation/state.rs` | Creation-event application, deduplication, ordering, snapshot recovery, read races, and write-response reconciliation. |
| Presentation | Existing conversation/sidebar/workspace views | Display live/reconnecting/stale status; preserve existing controls and normal live-update reader behavior. |
| Language-neutral contract | Planned `llm-docs/server/LIVE-UPDATES.md` | Endpoint, SSE examples, payload schemas, authentication, recovery recipe, limits, and provisional compatibility policy. |

### Minimal desktop implementation approach

Use the existing ownership boundaries rather than creating a parallel client subsystem:

1. **`api/`: one stream operation.** Own bound authentication, SSE decoding/validation, frame limits, and stream-specific deadlines. Deliver validated readiness/creation events or a terminal outcome; do not decide reconnection here.
2. **`conversation/live_updates.rs`: replace the polling policy.** A small pure lifecycle with connecting, loading baseline, live, and retrying phases owns attempt identity and one reconnect backoff. Do not retain independent polling/resource retry schedules or introduce a generic state-machine framework.
3. **`conversation/mod.rs`: execute the lifecycle.** Own one stream task plus the existing request/task support. Selection changes retarget the current history read. Required baseline failure takes the same recovery path as stream failure. Bound the stream-to-coordinator delivery path; keep write completions governed by session/operation identity, not by a disposable stream attempt.
4. **`conversation/state.rs`: merge entities and separate recovery from teardown.** Share ID-based creation merges between events and confirmed HTTP responses. Stage concurrent creations during replacing reads; merge older pages without overwriting live additions. Remove polling-era catch-up/confirmed-read/uncertain-read workflows once these replacements are covered. A reconnect reset invalidates server history and cursors, not drafts or pending writes.
5. **Views: small presentation changes.** Remove both refresh controls, reuse the workspace connection-status area for the simple loading/reconnecting notice, and expose a recovery-reset revision so the history view deliberately jumps to newest even when the selected channel is unchanged. Healthy updates retain existing anchoring behavior. Views do not own stream tasks or retry decisions.

The initial implementation does not need separate recovery strategies for every failure reason. Exercise the common recovery path with representative transport/read failures and retain the explicit safety tests below.

Do not modify legacy/alternative client directories. Keep human-authored documentation, including all README files, unchanged. Do not add general event-bus, command, or application-framework abstractions.

## Delivery stages

Complete stages 1–3 and verify the server contract before implementing client streaming. Stage 1 touches existing client wire imports solely to keep both builds working during the shared-type refactor.

### Stage 1 — Extract shared wire definitions without changing behavior

- [ ] Create `hamlet-protocol` with `Event::MessageCreated { message }` and `Event::ChannelCreated { channel }`, using `#[serde(tag = "type", rename_all = "snake_case")]`.
- [ ] Move `Message`, `Author`, `Channel`, and `ChannelType` wire definitions into the crate. Derive the serialization/deserialization and clone traits needed by both consumers; make OpenAPI derives an optional feature enabled by the server.
- [ ] Replace server definitions with imports/re-exports and client `WireMessage`/`WireAuthor`/`WireChannel` duplicates with shared types. Keep history/list wrappers local and keep existing public client-state types.
- [ ] Retain database-specific decoding on the server; move SeaORM-dependent `ChannelType::parse` out of the shared type. Preserve text-channel validation when the client changes from a string kind to the shared enum.
- [ ] Reuse client wire-to-state conversion for HTTP and later event decoding; do not bypass existing ID/channel/author validation because the Rust types are shared.
- [ ] Add path dependencies and update component lockfiles. Do not introduce a workspace solely for this change.
- [ ] Test exact wire examples, Unicode/newline message bodies, timestamp/ID representations, and both event variants. Confirm existing HTTP JSON and OpenAPI schemas remain compatible.

**Exit gate:** existing server and client verification passes; their current behavior is unchanged. No streaming client or deletion variant is added.

### Stage 2 — Add the event hub and protected SSE endpoint

Proposed route: `GET /api/v1/events`, using the existing protected scope and uniform method-error handling.

- [ ] Implement a concrete `EventHub` with a small application-facing interface: synchronous `notify(Event)` and subscription acquisition. Hide broadcast/SSE implementation details from feature operations. No-subscriber notification is normal and must not fail a committed write.
- [ ] Use a bounded shared broadcast buffer, with immutable shared retained payloads rather than a serialized copy per subscriber. Tokio broadcast is a candidate, not a reason to expose its receiver type throughout the application.
- [ ] Ensure `AppState` clones share the same hub across workers. Do not create a hub inside each worker factory. Dropping a subscriber releases its retained state.
- [ ] Register the subscription before yielding the first `ready` frame. If it lags before or after readiness, terminate; do not continue from the oldest surviving event.
- [ ] Return `200` with `Content-Type: text/event-stream` and an appropriate no-cache policy. Avoid response compression or buffering that batches small frames. Keep proxy-specific behavior local to this endpoint; do not redesign hosting or add a proxy setup feature.
- [ ] Emit complete, blank-line-terminated UTF-8 frames. JSON serialization must safely escape message newlines. Send heartbeats as comments, not application changes.
- [ ] Extract session validation so the handshake and periodic check share one implementation. Retain token digest and known expiry internally, never log credentials or message payloads.
- [ ] Prioritize expiry and due revalidation over queued events, even under continuous traffic. If revalidation stalls, stop delivery until it succeeds; on validation failure close the stream. Once streaming headers are sent, do not try to return a new HTTP error status.
- [ ] Document that normal end-of-stream is not itself authoritative session rejection. Reconnect returns `401` for a revoked credential; database failures are retryable transport/server failures, not proof the credential is invalid.
- [ ] Do not emit `id` fields. A supplied `Last-Event-ID` does not resume delivery; the response starts a fresh subscription and `ready` sequence.
- [ ] Update the OpenAPI assembly, checked-in artifact, and inventory. The source inventory currently scans only `lib.rs`, channels, and messages: include the new route registration source and resource count. Adapt representative response checks to read a bounded first frame rather than await the whole infinite body.
- [ ] Create the language-neutral contract documentation under `llm-docs/server/`, including ready/read/buffer sequencing and the provisional unknown-event policy.

**Exit gate:** real-route tests prove authenticated readiness, framing, multiple subscribers, worker sharing, bounded lag handling, expiry/revocation, and cleanup. An unbounded stream is never fully collected by a test helper.

### Stage 3 — Publish existing creation changes

- [ ] Pass the hub explicitly to message/channel operations (or a focused state reference), leaving HTTP handlers responsible for extraction and response mapping only.
- [ ] On each successful message/channel insert, synchronously notify exactly once in the normal successful execution path. Validation failure, duplicate channel names, failed inserts, and ID-collision retries must not emit phantom/duplicate events.
- [ ] Remove the message insert-then-author-lookup failure window. First try constructing the response from already-validated author identity and insertion values; if additional reads are genuinely necessary, complete them within a transaction before commit. Do not add a transaction abstraction or migration just to emit events.
- [ ] Ensure HTTP and event payloads describe the same entity and values. Prepare any fallible event data before committing. Notification never awaits a slow client and succeeds operationally when no client is connected.
- [ ] Verify actual handler/DB cancellation behavior using the current Actix/SeaORM/SQLx versions. Test a disconnected originating HTTP client while another client remains subscribed; use deterministic write barriers rather than timing guesses.
- [ ] If cancellation can commit a write without continuing to notify, add only narrow task-lifetime protection around the affected write-plus-notify operation and supervise its result. Do not add a generic mutation coordinator, global write gate, or durable outbox. Do not leave this risk unresolved while claiming synchronization correctness.
- [ ] Avoid panicking/fallible work after commit before enqueue. If an unexpected post-commit failure can leave healthy streams missing a known change, make that failure terminate delivery safely; verify recovery rather than silently log and continue. If this requires machinery beyond the agreed scope, return to the user with the concrete reproduction and alternatives.
- [ ] Exercise concurrent creation traffic. Do not impose database-commit order on notification arrival; test payload identity and client-sortable ordering instead.
- [ ] Do not emit a bootstrap-channel event before subscriptions exist; initial authoritative reads cover startup data. No message-deletion operation or event is added.

**Server exit gate:** stages 1–3 pass full server checks and the client still builds. A second authenticated client can observe a newly created message/channel without polling; failed writes produce no event. The cancellation check has a documented outcome or a narrowly tested fix.

### Stage 4 — Integrate the desktop stream and recovery

- [ ] Add a session-bound event-stream operation to `AuthenticatedClient`. Keep endpoint paths, bearer attachment, URL/TLS/redirect policy, parsing, and deadlines in `api/`; do not expose raw credentials or `reqwest::Response` to views/coordinators.
- [ ] Give streaming a distinct transport deadline policy: finite connection/readiness timeout and heartbeat-based idle detection, not the ordinary eight-second total body timeout. Preserve ordinary request policy unchanged. Keep production and controlled stream outcomes behind the same binding/decoding seam.
- [ ] Parse SSE incrementally across arbitrary byte boundaries, including split UTF-8, CR/LF boundaries, multiline data, comments, and partial final frames. Enforce limits per frame/incomplete-frame accumulation, not per network chunk. Unknown event types/extra JSON fields are ignored; malformed known changes terminate recovery attempts.
- [ ] Make `conversation/` own exactly one stream task per accepted session, independent of selected channel, focus, view recreation, and number of observers. `api/` owns transport, not reconnect decisions or conversation state.
- [ ] Replace polling scheduling with a pure connection/recovery lifecycle: connecting, loading baseline, live, and retrying/stale. Use one automatic recovery/backoff path for stream failures and required channel/newest-history baseline failures, including an uncached selection load failing while live. Do not add independent resource retry schedulers. Integrate it with the existing execution/time support, not a new runtime or event framework.
- [ ] Tag all stream deliveries and baseline reads with session generation and connection-attempt identity. Cancel obsolete reads/stream tasks on logout, expiry, server/session replacement, or a failed attempt; late results cannot alter the new state.
- [ ] On `ready`, begin bounded buffering and load channels, then the appropriate selected newest history page. Apply buffered creations with deduplication only after the baseline succeeds and the same stream attempt is still valid. For an empty channel list, no history request is necessary. Allow selection changes during recovery: invalidate the old history read and retarget the baseline without restarting the stream; only the current selection can satisfy the history baseline.
- [ ] On recovery, invalidate all old cached history/cursor/older-page server state and replace it with the newest selected page. Preserve drafts, pending write identities, confirmed/uncertain outcomes, and valid selection. Do not call the current full `Conversation::clear()`, which also clears local drafts and operation state.
- [ ] While disconnected, retained messages may remain visible as explicitly stale until replacement; never present old pages as recovered/synchronized. Recovery resets the selected reader to newest messages. Reading-position preservation across recovery remains out of scope.
- [ ] Bound the executor-to-coordinator event queue as well as the temporary recovery buffer: the current coordinator delivery channel is unbounded, so adding events directly to it without another limit would defeat the bounded-buffer decision. Keep terminal/recovery signals deliverable without waiting behind an unbounded backlog.
- [ ] Keep the stream active while unfocused. Remove focus-driven resource polling, but preserve session expiry and other non-polling timers. Do not add fallback periodic polling when SSE fails.
- [ ] Remove the **Refresh conversation** and **Refresh channels** controls and their manual-refresh entry points. Preserve older-page navigation/retries, drafts, and HTTP writes under their session/validation rules. Report stream failure separately from session invalidity.

### Stage 5 — Apply events safely and verify end-to-end behavior

- [ ] Apply `ChannelCreated` by ID, retaining the server's channel-list ordering and current selection. Another user's channel creation must not auto-select their channel; local confirmed creation retains its existing selection behavior.
- [ ] Apply `MessageCreated` to already-loaded histories. Do not load or allocate full history caches for every channel merely because an event arrives.
- [ ] Reuse/refactor the existing confirmed-message merge helper for ID deduplication and `created_at DESC, id DESC` ordering; parsed instants, not timestamp-string comparison, handle fractional precision/ties. Do not simply prepend in stream arrival order.
- [ ] During initial/selected-channel loads and recovery baseline reads, stage relevant creation events and confirmed HTTP entities that arrive after the read starts, then merge them into the accepted result. Bound staging; overflow causes full stream recovery.
- [ ] During older-page loads, merge pages into current history rather than replacing live additions; preserve the older cursor from that page. Request/session identities continue to reject obsolete completions.
- [ ] Apply confirmed HTTP writes through the same idempotent entity merge as events. Handle event-before-response, response-before-event, and response-during-recovery. Clear drafts/creation inputs only according to their own confirmed operation identity, not an arbitrary event or matching text.
- [ ] Remove post-confirmation history reads when direct HTTP/event payloads suffice, and remove history catch-up triggered solely by uncertain sends. Preserve uncertain-write warnings/drafts; healthy events continue normally and stream failures use automatic recovery. Never retry writes automatically.
- [ ] During normal healthy delivery, preserve existing reader anchoring/jump-to-latest behavior. Only reconnect recovery deliberately resets older history/position. Give the view an explicit recovery-reset revision; comparing selected channel/message IDs alone cannot reliably signal a reset.
- [ ] Replace polling-specific tests with meaningful live-update coverage, retaining current session-binding, write-uncertainty, pagination, view-lifetime, and draft scenarios. Delete unused polling code only after replacement coverage passes.
- [ ] Complete real-route two-client scenarios and a bounded burst/fanout check near the 10-change/second target. Record client count, payload sizes, latency, and buffer pressure; this is not a production scalability guarantee.

**Final exit gate:** all component checks pass; no periodic conversation polling remains; creation updates reach a second client without another read; reconnect rebuilds accurate newest state; drafts/pending writes survive correctly; late attempts cannot update a new session; bounded overflow and malformed known events cause visible recovery rather than silent loss.

## Required verification scenarios

Keep server external route tests under `server/tests/` (a new `live_updates.rs` is appropriate). Client in-crate suites belong in their owning feature's `tests/` directory; do not add inline or sibling suites. New shared-crate suites can use `protocol/tests/`. Use narrow owner-local test seams rather than widening production interfaces.

| Subject | Scenarios |
| --- | --- |
| Shared contract | Both variants; unchanged message/channel HTTP JSON; round-trip payloads; maximum valid message bodies; OpenAPI with and without the optional feature. |
| HTTP/SSE | Missing/invalid bearer; method rejection; correct content type; ready before changes; complete escaped frames; heartbeat comments; no replay IDs; teardown on client drop. |
| Publication | Success reaches all subscribers; no subscribers; invalid/duplicate/failed writes; retry collisions; payload matches HTTP response; post-write lookup regression; origin disconnect during write. |
| Authentication | Idle and busy-stream revocation within the validation policy; expiry even with queued events; pending/failed revalidation stops delivery; database error is not authoritative credential rejection. |
| Bounds | Slow server receiver; bounded client delivery/recovery/staging; oversized incomplete frame; overflow never resumes with silently dropped known events. |
| Read handoff | Change after readiness but before a read, during a read, and already present in a read; no duplicates or overwritten creations. |
| Ordering | Concurrent writes delivered out of timestamp order; tied timestamps; differing RFC3339 precision; channel ordering and selection. |
| Recovery | EOF, idle timeout, malformed known event, server restart, failed baseline read; newest-only reload discards stale old pages/cursors and preserves drafts/valid local operations. |
| Session lifetime | Logout, expiry, server replacement, old-generation rejection, view recreation/multiple observers, unfocused window; only one stream and no leaked credentials/tasks. |
| Writes and UI | Both event/response arrival orders; uncertain write with a delivered event; no automatic resend; writes during stream outage; normal scroll anchoring; recovery reset; older-page/read races; no manual refresh controls. |
| Compatibility | Unknown kinds ignored without creating serialized sentinel variants; extra fields accepted; malformed known changes rejected; `Last-Event-ID` does not imply replay. |

Test finite frames with explicit time bounds or controlled clocks. Do not read an SSE response to EOF in success tests. Use real loopback HTTP where disconnect/transport behavior matters; an in-process route helper alone cannot establish that behavior. Do not start native desktop automation or access a real keyring without separate consent.

## Proposed tuning defaults

These are implementation starting points, not additional negotiated product requirements. Keep them as named internal values and test with smaller controlled limits; do not build a new configuration system.

- Server broadcast capacity: 256 events initially; measure actual retained payload bytes and transport buffers.
- Client recovery/staging/delivery: bounded event counts, initially comparable to the server capacity; document each separate bound and avoid an unbounded executor bridge.
- Heartbeat/session revalidation: approximately 15 seconds; known expiry has its own deadline and priority.
- Stream handshake/readiness: finite, initially aligned with ordinary eight-second network policy.
- Client stream idle detection: initially about 45 seconds without byte progress, including heartbeat comments; no total streaming-body deadline.
- Reconnect: exponential backoff starting around 1 second, capped around 30 seconds, with jitter; reset backoff after successful baseline synchronization, not merely TCP connection/readiness.
- Maximum buffered SSE frame: an initial 64 KiB candidate, verified against maximum legal message serialization and incremental parser tests. A single network chunk may legitimately contain several frames.

## Verification commands

Run commands from the named component directory. No builds/tests have been run for this documentation-only planning session.

From `protocol/` after the crate exists:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --features openapi
cargo clippy --all-targets --features openapi -- -D warnings
```

From `server/`:

```sh
cargo fmt --check
cargo fmt --manifest-path migration/Cargo.toml --check
cargo clippy --locked --all-targets -- -D warnings
cargo run --locked --quiet --bin generate-openapi > openapi.json
cargo test --locked
cargo test --locked --test contract
```

From `client/`:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --locked
```

Refresh component lockfiles deliberately when adding the path dependency, then retain locked verification. A root workspace, new deployment infrastructure, or migration is not a prerequisite.

## Engineering checks and deliberate revisits

These are not permission to expand v1 scope silently:

1. **Write cancellation:** establish whether an in-flight commit can escape notification in the actual server. Add narrow lifetime protection only if needed; return to the user if the simple interface cannot meet the known-change correctness contract without significant additional machinery.
2. **Failure/cleanup:** ensure unexpected publication failures, stream termination, and shutdown cannot leave seemingly live delivery silently incomplete. Do not promise durability or exactly-once writes.
3. **Read races:** validate the creation-only staging/merge approach against current request serials, cursors, confirmed writes, and recovery. Full `clear()` is session teardown, not reconnect reset.
4. **Future dependent changes:** revisit ordering, unknown-event compatibility, and snapshot reconciliation before adding edits, deletion, permission changes, or other state changes that creation-only merging cannot represent safely.
5. **Future authorization:** v1 has no per-channel permissions. Adding permissions requires authorization-aware delivery and open-stream access-change handling before it ships, not only hidden client views. Do not add permission infrastructure now.
6. **Measured capacity:** the 10-change/second target and 256-event example are not hard limits or verified capacity. Tune with real payloads/client counts; defer multi-instance delivery.

## Adding a future event type

For an independent supported change: add its shared enum variant/payload, emit it after the feature's successful write, add client validation/application, update contract schemas/examples, and add tests. No transport route, event trait implementation, registration table, or generic invalidation flag is required.

If it modifies already-supported state, revisit compatibility and ordering first. For a genuinely useful bulk invalidation, add that specific variant and handler at that time. Message deletion remains a separate feature.

## Existing implementation constraints

- Server: one process, SQLite, Actix Web. No publisher, event log, or stream endpoint exists.
- Message/channel creation currently performs database writes without an event-delivery boundary. Message deletion is not implemented.
- Authenticated users currently have access to all channels. Authentication uses bearer headers.
- Client: Rust/GPUI; `api/` owns transport, `conversation/` owns loaded conversation state and polling, `session/` owns authentication lifecycle.
- Current HTTP transport has an eight-second total request/body deadline; an indefinite stream needs a distinct deadline policy.
- Native browser `EventSource` does not expose custom request headers. Future browser compatibility does not by itself require introducing cookie authentication now.

## References

- [Server architecture](server/ARCHITECTURE.md)
- [Client architecture](client/ARCHITECTURE.md)
- [WHATWG SSE standard](https://html.spec.whatwg.org/multipage/server-sent-events.html)
- [Tokio bounded broadcast and lag detection](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html)
