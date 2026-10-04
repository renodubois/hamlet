# Desktop authenticated stream API (#68)

The transport is implemented but **not activated by the desktop**. Conversation polling, recovery policy and views remain unchanged until #69. Server framing and deployment constraints remain in [the server contract](../server/LIVE-UPDATES.md).

## Caller boundary

`AuthenticatedClient::events(&Execution) -> EventStream` starts exactly one attempt using the immutable server/credential binding. `EventStream::next(&mut self)` returns `Result<LiveEvent, StreamError>`:

- `Ready`, `ChannelCreated(Channel)`, `MessageCreated(Message)` contain validated client-facing data, not wire entities or HTTP responses.
- `Api(ApiError)` preserves handshake authentication rejection (`AlreadyInvalid`), server failure, unavailable transport, and invalid response distinctions.
- `ReadyTimeout`, `IdleTimeout`, `Overflow`, and `Ended` are terminal. EOF alone is **not** authoritative credential rejection.

The handle owns its execution task and response body. Drop it to cancel an attempt, including pending headers or an idle body. Canceling an individual `next()` wait does not cancel the handle. A terminal result invalidates pending deliveries and remains observable on subsequent `next()` calls. Session expiry/logout/replacement and attempt identity are caller responsibilities; cloning the authenticated client never replaces an existing attempt's credentials.

There is no reconnect, replay, write retry, conversation state, selected-channel policy or fallback polling in this operation. #69 must retain one handle per conversation attempt, avoid adding an unbounded forwarding bridge, and drop it on session/attempt abandonment. Narrow dead-code/unused-export allowances identify the unactivated interface and can be removed at cutover.

## Ownership and bounds

- `client/src/api/events.rs`: bound request, handshake validation, incremental SSE decoding, deadlines, bounded delivery and cleanup.
- `client/src/api/client.rs`: separate reusable HTTP and stream clients share URL/localhost/no-proxy/redirect/TLS policy. Ordinary HTTP keeps its eight-second total request/body timeout. The streaming client has **no total-body timeout**.
- `client/src/api/channels.rs` and `messages.rs`: HTTP and SSE use the same wire-to-client conversion. IDs, author identity, text-channel type and timestamps retain HTTP validation; empty message channel IDs cannot self-validate without an HTTP request context.
- Connection **and readiness together** have an eight-second deadline from submission. Headers or heartbeat bytes do not extend this budget. After readiness, any nonempty body chunk (including comments or partial lines) renews a 45-second byte-progress idle deadline; empty chunks do not.
- A frame has a 64-KiB encoded-byte bound, including fields, comments and line endings. Accumulation resets at frame boundaries, not transport chunk boundaries. Partial EOF frames are discarded. Network/socket buffers and the currently received chunk are separate from parser accumulation; this is not a total process-memory quota.
- The delivery channel holds at most **256 supported deliveries, including readiness**. Its producer never waits for consumer capacity: the next supported delivery ends the attempt with `Overflow`. A separate one-result task-completion channel makes termination observable without draining the backlog. Terminal results take priority, so already-queued creations may be discarded when an attempt ends; callers must abandon synchronization, not apply the remainder as a successful stream.

SSE handles optional initial BOM, arbitrary UTF-8/CRLF splits, CR/LF/CRLF line endings, multiline data, comments, last event-field selection and standard ignored fields. WHATWG UTF-8 replacement decoding is used for invalid sequences. Unknown SSE event names and valid unknown application types are ignored; additional JSON fields are accepted. Readiness requires an object and occurs once, before supported creations. Malformed supported JSON/entities, duplicate recognized fields/discriminators, wrong media type and oversized frames terminate the attempt. No-data frames follow standard SSE non-dispatch behavior.

## Verification

Owner-local suites: `client/src/api/tests/events.rs` (controlled transport/time through the same authenticated operation) and `client/src/api/tests/event_http.rs` (production reqwest adapter, actual server routes, real TCP cancellation).

Coverage includes every two-chunk split of a BOM/CRLF/CR/multiline/Unicode fixture, one-byte input, discarded final frames, exact 65,536/65,537-byte boundaries for each line ending, four maximum server-serialized 4,000-character control-text messages in a single >64-KiB chunk, and 4,000 astral characters encoded as 48,000 bytes of JSON surrogate escapes. These fit independently below the frame bound. Queue capacity 256 succeeds; delivery 257 terminates before queued results are drained. Controlled heartbeat traffic remains live across multiple ordinary-request deadlines, then ends exactly at the no-progress limit.

Real-loopback tests observe both creation types through independently authenticated consumers without polling or follow-up entity reads; invalid/duplicate/failed writes emit neither. The maximum legal escaped message retains HTTP/event identity. Additional tests prove redirect rejection without bearer forwarding, revoked-handshake rejection, real socket cleanup on handle drop, streaming past 8.2 seconds, and unchanged ordinary HTTP body timeout at approximately eight seconds. The network tests use controlled scheduling on a current-thread runtime, but the production transport/binding/parser are unchanged; they are not native UI acceptance.

Exact red/green and final check results: [epic ledger](../implement-epic/renodubois-hamlet-71.md). No dependency or lockfile changes were required.

## Primary sources

- [WHATWG server-sent events](https://html.spec.whatwg.org/multipage/server-sent-events.html): UTF-8/BOM, line processing, blank-line dispatch and partial EOF semantics.
- [reqwest 0.12.28 ClientBuilder](https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html): total request/body timeout versus connection/read timeout, redirect and TLS defaults.
- Installed reqwest 0.12.28 source: `src/config.rs:69-79` falls back to client timeout when a request timeout is absent; `src/async_impl/client.rs:2629-2635` installs the total timer; `src/async_impl/response.rs:315-331` provides incremental `chunk()` without enabling the separate stream feature. Clearing `Request::timeout_mut()` would not disable the ordinary client's timeout, which is why streaming uses a separate client.
