# Desktop authenticated live updates (#68/#69)

The desktop conversation coordinator owns one authenticated stream per accepted session. Creations arrive without polling; failed delivery triggers readiness-gated baseline recovery. Server framing and deployment constraints remain in [the server contract](../server/LIVE-UPDATES.md).

## Caller boundary

`AuthenticatedClient::events(&Execution) -> EventStream` starts exactly one attempt using the immutable server/credential binding. `EventStream::next(&mut self)` returns `Result<LiveEvent, StreamError>`:

- `Ready`, `ChannelCreated(Channel)`, `MessageCreated(Message)` contain validated client-facing data, not wire entities or HTTP responses.
- `Api(ApiError)` preserves handshake authentication rejection (`AlreadyInvalid`), server failure, unavailable transport, and invalid response distinctions.
- `ReadyTimeout`, `IdleTimeout`, `Overflow`, and `Ended` are terminal. EOF alone is **not** authoritative credential rejection.

The handle owns its execution task and response body. Drop it to cancel an attempt, including pending headers or an idle body. Canceling an individual `next()` wait does not cancel the handle. A terminal result invalidates pending deliveries and remains observable on subsequent `next()` calls. Session expiry/logout/replacement and attempt identity are caller responsibilities; cloning the authenticated client never replaces an existing attempt's credentials.

There is no reconnect, replay, write retry, conversation state, selected-channel policy or fallback polling in the API operation. The conversation coordinator retains its handle in one cancelable forwarding task and cancels it on session/attempt abandonment. No staged-interface dead-code allowances remain.

## Conversation policy

`conversation/live_updates.rs` is a pure connecting/loading-baseline/live/retrying/closed policy. The coordinator waits for Ready, loads channels and only the current selection's newest history, then synchronously merges buffered creations and acknowledges synchronization. Empty channel lists need no history. Selection changes cancel/invalidate only the selected read; completed channels and stream ownership stay with the same attempt.

API delivery, executor delivery, recovery buffering and replacing-read staging each have independent 256-entity bounds. `conversation/delivery.rs` prioritizes a separate terminal lane; stream forwarding never waits for ordinary capacity. HTTP completions can wait, preserving write outcomes. EOF, transport/parser/deadline errors, overflow, required baselines and uncached selection failures enter one recovery path. Retry bases are 1, 2, 4, 8, 16 and capped 30 seconds with equal jitter; only completed reconciliation resets failure count. The clock-anchored timer remains for retry and expiry, never polling.

Read deliveries carry session, attempt and request identities. Writes carry session/operation identity independently and remain usable during recovery. Recovery invalidates histories/cursors but preserves drafts, selection and write outcomes; only selected stale rows are retained for display. The history view consumes an explicit reset revision. HTTP confirmations and events share ID-based entity merges; only confirmed originating operations clear their input. Uncertainty causes neither reads nor reconnect/resend, and matching text never confirms it. Older-page failure retains local retry. Views show Connecting initially and a persistent Reconnecting notice until baseline success; they have no stream tasks, retry schedulers or Refresh controls.

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

Conversation coverage lives in `conversation/tests/{live_updates,live_coordinator,delivery,state,live_state,coordinator,route}.rs`; semantic view coverage includes `views/tests/live_updates.rs` and `history_lifecycle.rs`. The real-route coordinator test records unchanged ordinary GET counts while a second authenticated user's message and channel appear, with application time held fixed. View tests cover unfocused delivery, recreation, input preservation, healthy anchoring and identical-ID recovery reset.

Exact red/green and final check results: [epic ledger](../implement-epic/renodubois-hamlet-71.md). No dependency or lockfile changes were required.

## Primary sources

- [WHATWG server-sent events](https://html.spec.whatwg.org/multipage/server-sent-events.html): UTF-8/BOM, line processing, blank-line dispatch and partial EOF semantics.
- [reqwest 0.12.28 ClientBuilder](https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html): total request/body timeout versus connection/read timeout, redirect and TLS defaults.
- Installed reqwest 0.12.28 source: `src/config.rs:69-79` falls back to client timeout when a request timeout is absent; `src/async_impl/client.rs:2629-2635` installs the total timer; `src/async_impl/response.rs:315-331` provides incremental `chunk()` without enabling the separate stream feature. Clearing `Request::timeout_mut()` would not disable the ordinary client's timeout, which is why streaming uses a separate client.
