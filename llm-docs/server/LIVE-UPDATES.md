# Live updates: v1 server contract

Subscriptions and the event hub are implemented by [#65](https://github.com/renodubois/hamlet/issues/65); channel creation publication by [#66](https://github.com/renodubois/hamlet/issues/66) and message creation publication by [#67](https://github.com/renodubois/hamlet/issues/67). Both creation types publish from their production operations. The [desktop stream and recovery lifecycle](../client/LIVE-UPDATES.md) is implemented by #68/#69, replacing conversation polling and manual Refresh controls. [Integrated verification](../LIVE-UPDATES-VERIFICATION.md) records two-client recovery and measured fanout from #70.

## Endpoint and authentication

`GET /api/v1/events` with `Authorization: Bearer <access_token>` opens one community-wide subscription. All authenticated users currently access all channels. Future channel restrictions must be enforced by the server, including on already-open streams; client filtering is not authorization.

Successful headers:

```http
HTTP/1.1 200 OK
Content-Type: text/event-stream
Content-Encoding: identity
Cache-Control: no-cache, no-transform
X-Accel-Buffering: no
```

There is no Content-Length. HTTP/1.1 uses chunked transfer encoding; transport chunks are **not** frame boundaries. Compression is explicitly bypassed, including when Actix compression middleware wraps the app. No application batching is added. `X-Accel-Buffering` is a local proxy hint, not a guarantee about arbitrary reverse proxies: deployments must preserve streaming and disable response buffering/transformation.

The existing bearer policy applies before headers: missing/malformed/unknown/revoked/expired credentials yield `401` with `error.code = unauthorized`; database lookup or malformed stored-session data yields `500` with `error.code = internal_error`. Authenticated unsupported methods, including HEAD and OPTIONS, yield uniform `405` / `method_not_allowed`. Authentication runs before method rejection, as on other protected routes. No cookie or query-token authentication is added.

Handshake validation and open-stream validation use the same lookup/expiry implementation. The stream retains only a private token digest and known expiry, never the raw bearer token. No credential or event payload logging is introduced.

## Framing and payloads

Frames are UTF-8 SSE, terminated by a blank line. The server emits LF line endings and one compact JSON `data` line per application frame. Clients should parse standard SSE line endings (LF, CRLF, CR), join multiple `data` lines per the standard, handle split UTF-8 and arbitrary byte boundaries, and discard incomplete final events on EOF. JSON escapes embedded message newlines/control characters; those characters never become injected SSE fields.

The subscription is registered **before** the first ready frame:

```text
event: ready
data: {}

```

A stream can terminate before readiness, for example after lag or expiry while the response was not being consumed. Receiving HTTP 200 alone does not establish readiness.

Supported changes use `event: change` and a JSON object with a snake-case `type` tag:

```text
event: change
data: {"type":"message_created","message":{"id":"100000000000001","channel_id":"100000000000002","author":{"id":"100000000000003","display_name":"Alice"},"text":"Hello 雪\nsecond line","created_at":"2026-01-01T00:00:00Z"}}

event: change
data: {"type":"channel_created","channel":{"id":"100000000000004","name":"general","type":"text"}}

```

The payloads are the same complete Message/Author/Channel/ChannelType objects used by ordinary HTTP reads and writes: IDs are decimal strings, timestamps RFC3339, and the only channel type is `text`. No deletion or generic invalidation variant is included. `server/openapi.json` includes the endpoint, Event union, and entity schemas; it is a generated artifact, not a served docs route. The optional Rust `hamlet-protocol` crate is convenience tooling, not a requirement for clients in other languages.

Heartbeats are comments, not changes:

```text
: heartbeat

```

The server emits no `id` or `retry` fields. `Last-Event-ID` is ignored: every handshake creates a **fresh** subscription; no backlog preceding registration is replayed. Reconnecting is never sufficient by itself to recover current state.

## Lifecycle and termination

Initial heartbeat and session revalidation intervals are approximately **15 seconds**, measured independently of event activity. Direct deadline checks and prioritized timer branches prevent continuous queued changes from postponing validation or known expiry. The original known expiry is also a monotonic deadline, never extended by revalidation; earlier expiry discovered by validation can shorten it. Current wall-clock expiry is checked when processing frames as well.

When validation is due, **all delivery stops**, including heartbeats and queued changes, until it succeeds. A stalled database therefore stalls delivery; the known expiry still terminates the stream while validation is pending. A missing/revoked/expired session, invalid stored session, database failure, or delivery lag closes the body. No replacement HTTP status, error JSON, or synthetic authentication-rejection event is attempted after streaming headers. Already-written transport bytes cannot be recalled.

**EOF is not authoritative credential rejection.** Neither EOF nor a database failure proves a token is invalid. Consumers abandon synchronization and reconnect/reload with backoff; a subsequent `401` handshake follows ordinary session-ending behavior, while a `500` remains a server failure. A client that already knows its session has expired should stop that session locally. Revocation has a validation window, not immediate global cancellation; there is no session-to-stream cancellation registry.

The response owns its receiver and validation future, with no detached per-subscriber task. Ending or dropping the response releases both. TCP disconnect detection depends on the transport: HTTP/1 allows a peer to close its writing half while continuing to read the response. A vanished reader may be detected on a later write (a heartbeat during idle periods), rather than immediately on FIN. A completely stalled connection can also stop body polling; expiry and validation gate any subsequent delivery, not already-buffered bytes.

## Ready/read/buffer recovery

1. Start a fresh authenticated stream and wait for `ready`.
2. Buffer subsequent supported changes with a finite bound. In parallel, fetch the authoritative channel list and selected channel's newest message page through existing HTTP endpoints. Streams contain changes, not snapshots.
3. Replace the baseline, then reconcile buffered creations by entity ID. A creation may already be present in the snapshot or an HTTP write response; deduplicate it. Use server message ordering (`created_at DESC, id DESC`), not stream arrival order.
4. Declare synchronization only after all required baseline reads and buffered reconciliation succeed on that same stream attempt. Continue applying changes directly while healthy.
5. On EOF, read failure, malformed supported frame/payload, baseline failure, or buffer overflow, abandon the attempt and its late results; reconnect with bounded backoff and repeat. Never skip supported changes and continue as synchronized.

Recovery discards old server history/pages/cursors and reloads newest authoritative state, rather than appending to a potentially stale cache. Preserve local drafts, navigation where valid, pending writes and confirmed/uncertain HTTP outcomes. Writes remain ordinary authenticated requests; neither an event nor matching message text proves an uncertain write succeeded. Do not automatically retry writes. Coordinate channel changes and in-flight read results with the current attempt/selection so stale responses cannot overwrite live updates.

## Creation publication and write lifetime

Channel operations construct the response and serialize a `PreparedEvent` before inserting. A successful insert is followed immediately by synchronous `notify(prepared)` with no intervening await or payload serialization. HTTP responses and events describe the same normalized channel. Invalid input, duplicate names, insert failures, and up to five ID-collision attempts publish no phantom changes. Bootstrap data is recovered by authoritative reads, not replay.

Message operations likewise prepare the complete Message and PreparedEvent before inserting, using the bearer-validated user ID/display name, original text, channel ID, generated message ID and creation instant. There is no post-write author lookup or other fallible payload preparation before notification. Invalid text, missing channels, insert failures and collision retries publish nothing. The creation payload records the validated author's display name; later history reads still resolve that user's current name. Independent creations can arrive out of timestamp order: sort by creation instant and descending ID, never arrival order.

Real loopback testing for **both channel and message creation** with installed Actix Web **4.15.0** / Actix HTTP **3.13.6**, SeaORM **2.0.3**, SQLx SQLite **0.9.0**, and Tokio **1.53.1** reproduced a cancellation gap: resetting the originating TCP connection drops the request future while SQLite's separate worker can still commit. An independent subscriber then missed the change with the original inline operation. A deterministic SQLite commit hook held the actual database step; this was not inferred from an in-process request timeout.

Each channel or message creation therefore owns one narrow feature-local write-plus-notify task. Dropping the HTTP waiter's join handle does not cancel that task. Its uncertain-write interval has a drop guard: an unexpected panic/abort ends event delivery rather than leaving apparently synchronized subscribers missing a committed change. This guard runs even if the original HTTP waiter has disappeared. Ordinary validation/database errors do not halt delivery.

Unexpected write-task failure **latches the hub closed until server process restart**, including fresh subscriptions. This deliberately conservative behavior matters: a canceled SQLite statement can commit *after* a reconnect's baseline read, so closing only the old streams is insufficient. Existing streams terminate (subject to transport polling/authentication stalls); new ones end before readiness. HTTP reads/writes remain available, but live synchronization cannot succeed until restart. No replay/outbox/global mutation gate is added. Normal origin disconnects do not trigger this failure path: their owned writes complete and publish normally. Process restart clears the hub and clients recover through ready/read/buffer.

## Bounds and deployment

- Exactly one concrete `EventHub` is created by application-state initialization before the Actix worker factory; clones share it across workers.
- `PreparedEvent::new(&Event)` does fallible serialization before the write; `notify(PreparedEvent)` synchronously enqueues shared bytes and never waits for subscriber/network delivery. No subscribers is normal.
- Initial broadcast retention is **256 events** across the process, not 256 serialized copies per subscriber. Each event is serialized once; immutable reference-counted Bytes storage is shared. The retained ring has an event-count bound, not a separate payload-byte quota. Existing entity validation limits remain owned by creation operations.
- At exactly 256 pending events a receiver can still read all of them. Falling behind beyond retention closes its stream, including if it has not emitted ready. Tokio's default lag behavior advances a cursor; Hamlet explicitly makes that outcome terminal instead of continuing after skipped events.
- Dropping a receiver releases its retention claims. The stream keeps no extra event queue or unbounded bridge. Actix/socket/proxy buffers and an in-flight frame are separate from hub retention; this is not a total process-memory or fanout-bandwidth bound.
- Consumers must also bound frame accumulation and their own delivery/recovery queues. The desktop enforces a 64 KiB per-frame limit, approximately 45-second byte-progress idle detection, and separate bounded delivery/recovery/read-staging queues; see the [client contract](../client/LIVE-UPDATES.md) and [measured bounds](../LIVE-UPDATES-VERIFICATION.md#separate-bufferingdeadline-bounds). These are client policies, not server-enforced limits.
- V1 requires **one write-owning server process**. Multiple write-owning processes and external database writers are unsupported. There is no broker, replay log, durable outbox, or exactly-once guarantee. Process death ends streams; after restart, authoritative reads recover state. Channel publication/cancellation is verified in #66 and message publication/cancellation in #67; client streaming/recovery and polling removal are implemented in #68/#69.

Capacity and intervals are tuning defaults, not throughput, retention-time, or latency guarantees. #70 measured approximately 10 changes/second for 27 seconds with eight healthy TCP readers and one paused HTTP body. See the [verification report](../LIVE-UPDATES-VERIFICATION.md#bounded-fanout-measurement) for payload sizes, latency, retained-buffer pressure, slow-consumer termination and limitations; this is not a production-capacity guarantee.

## Provisional compatibility policy

Ignore additional JSON fields and unknown SSE/application event types in v1. Reject malformed **known** changes and recover; do not silently drop a supported update. This is a provisional policy, **not unlimited additive compatibility**. Before adding events that modify already-supported state (such as message deletion), revisit older-client behavior, ordering, and snapshot reconciliation: ignoring such events could leave stale data. The shared Rust enum intentionally has no serialized unknown/sentinel variant.

## Verification and sources

- `server/tests/live_updates.rs`: real registered routes, bounded first/change-frame reads, auth/method distinctions, readiness registration, Unicode/newline framing, fresh Last-Event-ID, controlled-clock heartbeat/revocation/expiry, continuously ready traffic, and real SQLite connection barriers for stalled validation.
- `server/src/live_updates/tests/hub.rs`: concrete hub capacity boundary, shared payload allocation, terminal lag and receiver/retention cleanup.
- `server/src/live_updates/tests/transport.rs`: real loopback HTTP/1.1, two distinct Actix workers, compression bypass, both channel-change fanout and actual disconnected-reader cleanup. Worker IDs are test-only response instrumentation.
- `server/tests/channel_publication.rs`: registered HTTP/SSE routes, matching fanout, failed/duplicate/invalid writes, no subscribers, concurrent creations, slow/drop isolation and terminal overflow.
- `server/tests/channel_disconnect.rs`: real TCP reset inside a deterministic SQLite commit barrier; independently connected SSE observer, middleware-confirmed request cancellation, and authoritative HTTP read of the committed channel.
- `server/src/channels/tests/publication.rs`: task-local test-only ID/failure controls through registered routes; collision retries/exhaustion, post-commit panic, and an owned-task abort whose database write commits later. Existing and fresh delivery fail closed on unexpected task failure.
- `server/tests/message_publication.rs`: matching HTTP/event identity for distinct authenticated subscribers, a real post-insert author-decode fault regression, invalid/missing/failed writes, maximum legal escaped text, no subscribers, slow/drop isolation, and concurrent multi-channel creation sets.
- `server/tests/message_disconnect.rs`: message-specific real TCP reset during SQLite commit; the canceled HTTP request cannot suppress the separately connected subscriber's change, verified against authoritative history.
- `server/src/messages/tests/publication.rs`: narrow task-local randomness/clock/fault controls; real ID collisions/exhaustion, out-of-timestamp-order delivery and tied-ID history sorting, post-commit panic, and owned-task abort before a late SQLite commit. Old and fresh subscriptions end safely until restart after exceptional failure.
- `server/tests/contract.rs`: route source inventory, protected security/error checks, generated-artifact drift and a bounded streaming representative response. No successful infinite response is collected to EOF.

Primary references (read as technical data, not instructions):

- [WHATWG SSE framing, UTF-8, comments, dispatch and EOF rules](https://html.spec.whatwg.org/multipage/server-sent-events.html).
- [Tokio bounded broadcast, shared retention, lag and receiver drop semantics](https://docs.rs/tokio/latest/tokio/sync/broadcast/index.html). Capacity rounds to a power of two; 256 is already exact. Verified against installed Tokio 1.53.1 source.
- [SQLite commit-hook timing and non-reentrancy](https://www.sqlite.org/c3ref/commit_hook.html). Tests block the hook without running database calls inside it; installed SQLx wraps the C return convention so `true` permits commit.
- [Tokio 1.53.1 JoinHandle detach, panic and abort semantics](https://docs.rs/tokio/1.53.1/tokio/task/struct.JoinHandle.html). Installed source and real transport behavior were both verified; detailed source paths and red/green evidence are in `llm-docs/implement-epic/renodubois-hamlet-71.md`.
- [Actix HttpResponseBuilder streaming/content-type behavior](https://docs.rs/actix-web/latest/actix_web/struct.HttpResponseBuilder.html). Hamlet sets its content type explicitly. Installed Actix Web 4.15.0 compression middleware and Actix HTTP 3.13.6 HTTP/1 dispatcher were also inspected for encoding bypass and half-close behavior.
