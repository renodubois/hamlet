# Simple live updates plan

Status: revised scope; simplification is not yet implemented. This replaces the previous synchronization/recovery plan.

The previous plan's “no implementation changes” status was stale: the repository already contains the shared protocol crate, server SSE/publication code, desktop transport, and baseline-recovery lifecycle. Refactor those modules rather than building another live-update subsystem. Existing component live-update documentation describes that implementation, not the simpler target below; update it when the code changes.

## Goal

Provide a small, best-effort server-to-client event stream for a young, self-hosted chat app. Connected clients should see new messages and channels without polling. This is a foundation for later features, not a reliable synchronization system.

**The database and ordinary HTTP reads are authoritative. The stream is a convenience, not proof that a client has current state.**

## V1 scope and accepted trade-offs

- One bearer-authenticated SSE stream per desktop session, covering all currently accessible channels.
- Existing message-creation and channel-creation events, with complete entity payloads.
- Ordinary HTTP for reads and writes; no fallback periodic conversation polling.
- Simple automatic reconnect after a fixed delay, **without catch-up reads**.
- No manual refresh controls or user-triggered catch-up workflow.
- One write-owning server process, one in-memory hub shared across Actix workers.

Missed events are acceptable in this version. This includes changes during disconnection, server/request cancellation, queue overflow, and the overlap between HTTP reads and live delivery. Reconnection restores future delivery; it does not repair missed updates. Ordinary future reads, such as loading a channel or reopening the conversation, may retrieve missed changes. Do not introduce a user-triggered catch-up workflow; improve these edge cases later if real usage warrants it.

There is no guarantee of delivery, exactly-once application, snapshot consistency, database-commit ordering, or a gap-free initial connection. Do not call the connected state “synchronized.”

This explicitly relaxes the old requirement that every committed change must either be delivered or force all clients into authoritative recovery. Do not rebuild that requirement under different terminology.

## Small module interfaces

### Server: publish completed changes

Keep the existing concrete `EventHub` and shared `hamlet-protocol` types. Their existence is not a reason for another extraction or abstraction project.

Feature operations prepare the entity, perform the database write, then notify the hub in their normal successful path. Notification does not await network delivery or slow subscribers. No subscribers is normal; failed writes must not publish phantom creations. Keep the existing improvement that prepares message/author data before insertion rather than requiring a fallible post-insert author lookup.

Use a bounded broadcast buffer. If a subscriber falls behind, close that stream; the client's ordinary reconnect loop starts a fresh subscription. There is no gap-repair mechanism. Keep existing shared serialized payload storage if useful; no sizing study is a prerequisite.

Remove the global interruption latch and write guards whose purpose is to preserve synchronization after an exceptional write-task failure. One missed notification must not disable live updates for the entire app until process restart. A separately owned write task is not required solely to guarantee notification after the originating HTTP request disappears; simplify that machinery without changing ordinary database validation or write-uncertainty behavior.

No outbox, replay log, mutation coordinator, global write gate, broker, or event/plugin registry.

### Transport: keep the existing SSE contract

`GET /api/v1/events` uses the existing protected HTTP scope and bearer authentication. Writes stay on their existing endpoints.

Keep current wire framing rather than introducing a gratuitous protocol change:

```text
event: ready
data: {}

event: change
data: {"type":"message_created","message":{...}}

event: change
data: {"type":"channel_created","channel":{...}}

: heartbeat

```

`ready` means the subscription is open. It does **not** gate HTTP loading, initiate a baseline, or promise that the cache is current. Heartbeats keep idle streams active. There are no resumable IDs, replay, or `Last-Event-ID` semantics.

Keep incremental SSE parsing, existing payload validation, bounded frame accumulation and bounded delivery queues. Arbitrary network chunks are not event boundaries. Unknown event types and additional JSON fields are ignored; malformed supported events end the attempt. Dropping or ending an attempt may discard its queued events.

Retain existing stream-specific connection/idle deadlines; an indefinite stream must not inherit the ordinary HTTP total-body timeout. No new parser or generic transport framework is needed.

### Authentication and ownership: preserve basic safety

Handshake authentication, existing periodic server session validation, and known expiry remain. They enforce access, not delivery reliability; there is no need to redesign them as part of this simplification. Do not add an immediate-revocation registry or permission infrastructure.

Logout, expiry, or session/server replacement cancel the desktop stream. Late deliveries from an obsolete session or attempt are ignored. An authoritative authentication rejection follows the existing session-ending path; EOF and network errors alone do not log the user out.

Keep credentials out of URLs, views, logs, and event payloads. All users currently access all channels; future channel permissions must restrict delivery on the server before that feature ships.

## Desktop behavior

### Initial load and navigation

Start the stream and perform the usual channel/history HTTP loading independently. Neither waits for the other. Navigation and ordinary read failures retain their own behavior; a failed history read does not restart the stream or reset the whole conversation.

Apply received creations to already-loaded state. Ignore message events for histories that are not loaded; selecting those channels loads them normally. Do not allocate a history cache for every event recipient.

No temporary recovery buffer, baseline-completion handshake, read retargeting within a recovery attempt, or initial read/stream barrier is required. A creation overlapping a replacing HTTP read can be missed or superseded by that read. This is an accepted edge case for now, not a reason to introduce manual refresh or recovery machinery. Reuse cheap existing merges where convenient, but do not retain staging machinery solely to guarantee a gap-free handoff.

Keep ordinary request identities so a late history response cannot replace the *wrong selected channel* or update a different session. Accepting best-effort freshness is not permission to corrupt navigation or session state.

### Live event application and writes

- Merge creations by entity ID so an HTTP write response and its event do not produce duplicates.
- Keep message ordering by parsed creation time and descending ID, not event arrival order; preserve existing channel ordering.
- A remote channel creation does not select that channel. Local confirmed creation keeps its existing selection behavior.
- Keep normal reader anchoring, pagination, drafts, and HTTP write behavior.
- Only the originating confirmed write clears its input. An event or matching text does not confirm an uncertain send.
- Never automatically resend writes. A send timeout does not trigger a stream restart or special catch-up read.

ID deduplication is a local UI rule, not an exactly-once delivery guarantee.

### Disconnection and reconnect

Use a small lifecycle: **connecting → connected → waiting to retry**, plus session teardown. Reuse the existing executor and task ownership.

On EOF, transport/parser error, idle timeout, or queue overflow:

1. End the affected attempt and retain current conversation state.
2. Show **Live updates disconnected — reconnecting.**
3. Wait a fixed delay (initially 3 seconds), then open a fresh stream for the same active session.

Repeat while the session remains active. No exponential backoff, jitter, failure taxonomy, recovery phases, or special navigation rules. Keep only one attempt/retry task alive. A healthy stream stays connected while the window is unfocused.

A successful reconnect changes only connection status. It does not fetch channels/history, discard pages/cursors, reset reading position, or affect drafts and pending writes. A connected indicator means future events can arrive, not that earlier gaps were repaired. Do not promise synchronization in the interface or add a catch-up action. No persistent freshness-tracking model is required.

### No manual catch-up workflow

Keep **Refresh conversation** and **Refresh channels** absent. Users should normally receive updates without taking action. Missed-update edge cases are accepted for now and can be improved later; adding refresh controls is not part of this feature.

Ordinary loading and navigation continue using existing HTTP reads. Their errors are local read failures, not connection failures. Older-page retries remain local. No separate **Retry connection** control is needed.

## Implementation sequence

Paths below are relative to the repository root. This is a refactor plan, not authorization to discard unrelated changes.

1. **Simplify desktop connection ownership.** In `client/src/conversation/live_updates.rs` and `mod.rs`, replace baseline/recovery scheduling with fixed-delay reconnect. Remove readiness-gated reads, automatic reload on stream failure, and global recovery on ordinary read failure. Keep one session-owned stream and obsolete-delivery checks.
2. **Simplify state and presentation.** In `client/src/conversation/state.rs` and the existing views, remove recovery-only buffers, stale-history copies and recovery-reset revisions. Keep manual refresh controls absent and show only basic connection status. Retain reusable entity merges, normal request identity checks, pagination, and local write outcomes. Remove read-staging/confirmation overlays only where they serve the abandoned synchronization guarantee, not where they protect drafts or write confirmation.
3. **Simplify server publication.** In `server/src/live_updates/hub.rs` and message/channel operations, remove the global interruption latch and exceptional-write synchronization guards. Keep bounded fanout, write-then-notify, payload identity and existing authentication/heartbeat behavior. Remove detached-write machinery introduced solely for notification guarantees, preserving existing HTTP success/error semantics and uncertain-write warnings.
4. **Replace obsolete expectations and document the new contract.** Update owner-local tests, `llm-docs/client/LIVE-UPDATES.md` and `llm-docs/server/LIVE-UPDATES.md`. Keep wire/OpenAPI shapes unchanged unless an actual contract change requires regeneration. Historical verification/implementation records stay historical; do not present them as verification of this refactor.

Reuse `protocol/`, `client/src/api/events.rs`, and bounded delivery support rather than replacing already-useful code. Simplification is principally deleting coordination requirements, not rebuilding working transport. No changes to legacy clients, deployment infrastructure, database schema, or human-authored documentation/README files.

## Verification: small and behavioral

Retain parser/authentication and ordinary conversation tests where they still apply. Replace tests that demand the old recovery guarantee rather than weakening unrelated safety assertions.

Essential scenarios:

- A second authenticated client receives new messages and channels without polling or follow-up entity reads. Failed writes produce no event; HTTP/event payloads match.
- Event-before-response and response-before-event each produce one entity. Remote channel creation preserves selection and message ordering remains correct.
- Stream failure triggers one retry after the fixed delay and **no automatic channel/history GETs**. Reconnection preserves drafts, pending writes, pages, selection and reading position.
- A change made during disconnection may remain absent after reconnect; later ordinary reads can retrieve it. This is an accepted behavior to test, not a synchronization failure or a reason to add a catch-up action.
- Initial/read-overlap events do not require buffering or restart. A read failure does not restart a healthy stream.
- Logout/session replacement cancels delivery; late results cannot update the new session. Transport failure is not credential rejection.
- Bounded queues and frames remain bounded; overflow closes/retries without catch-up. A publication failure no longer latches all future subscriptions closed.
- No manual refresh or connection-retry controls appear. Ordinary navigation/reads preserve local inputs/write outcomes. Existing pagination and uncertain-send behavior remains intact.

Use finite, time-bounded SSE reads and controlled retry clocks. No peak-throughput target, fanout benchmark, cancellation-proof delivery investigation, native desktop automation, or keyring access is a release prerequisite.

From each of `protocol/`, `server/`, and `client/`, run the applicable existing component checks:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Also build the client and run existing optional OpenAPI/contract checks where touched. No implementation builds/tests have been run for this plan revision.

**Done means:** healthy connected clients see creation updates; dropped streams retry simply; no manual catch-up workflow, polling or baseline-recovery subsystem remains; basic authentication, bounded memory, session isolation and local write behavior still work.

## Explicitly deferred

- Guaranteed read/stream handoff, gap detection and catch-up after reconnect.
- Snapshot reconciliation, recovery buffers, automatic cache reset and recovery-specific scrolling.
- Durable delivery, replay/cursors, outbox and exactly-once semantics.
- Cancellation-proof publication and global fail-closed delivery supervision.
- Adaptive reconnect/backoff policies and detailed outage UX. Manual refresh/catch-up controls are not a desired workflow and are excluded, rather than a planned follow-up.
- Multi-process fanout, brokers, scaling targets and performance tuning projects.
- Typing indicators, edits, deletion, unread counts, presence and channel permissions as implemented features.

For a later feature, add its concrete event, publish it from the owning operation, and handle it in the client. Ephemeral events such as typing indicators can simply expire locally; they do not need replay. Edits/deletions may justify stronger freshness or compatibility rules when those features actually exist. Revisit reliability after real usage demonstrates a need—not as a prerequisite for sending the first useful events.
