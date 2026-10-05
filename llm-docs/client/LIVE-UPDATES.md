# Desktop authenticated live updates (#68/#69/#72)

The desktop conversation coordinator owns one authenticated stream per accepted session. Creations arrive best-effort without polling; failed delivery reconnects after a fixed three seconds without catch-up reads. Server framing and deployment constraints remain in [the server contract](../server/LIVE-UPDATES.md).

## Caller boundary

`AuthenticatedClient::events(&Execution) -> EventStream` starts exactly one attempt using the immutable server/credential binding. `EventStream::next(&mut self)` returns `Result<LiveEvent, StreamError>`:

- `Ready`, `ChannelCreated(Channel)`, `MessageCreated(Message)` contain validated client-facing data, not wire entities or HTTP responses.
- `Api(ApiError)` preserves handshake authentication rejection (`AlreadyInvalid`), server failure, unavailable transport, and invalid response distinctions.
- `ReadyTimeout`, `IdleTimeout`, `Overflow`, and `Ended` are terminal. EOF alone is **not** authoritative credential rejection.

The handle owns its execution task and response body. Drop it to cancel an attempt, including pending headers or an idle body. Canceling an individual `next()` wait does not cancel the handle. A terminal result invalidates pending deliveries and remains observable on subsequent `next()` calls. Session expiry/logout/replacement and attempt identity are caller responsibilities; cloning the authenticated client never replaces an existing attempt's credentials.

There is no reconnect, replay, write retry, conversation state, selected-channel policy or fallback polling in the API operation. The conversation coordinator retains its handle in one cancelable forwarding task and cancels it on session/attempt abandonment. No staged-interface dead-code allowances remain.

## Conversation policy

`conversation/live_updates.rs` is a pure connecting/connected/waiting-to-retry/closed policy. Initial channel reads and the selected channel's newest-history read start independently of Ready. Ready marks the stream usable, not the local data synchronized. Selection changes cancel/invalidate only the selected read; they do not replace the session stream.

Channel creations merge only into a loaded channel list; message creations merge only into loaded histories, including inactive caches. Creations for unloaded data or during a replacing read are ignored. There is no readiness/baseline buffer, replacing-read staging or overflow-triggered data reset. Missed offline events and read-overlap events are accepted losses, potentially permanent: reconnect neither replays them nor fetches a catch-up baseline. Future delivery resumes after Ready.

API and executor delivery retain their independent 256-entity bounds. `conversation/delivery.rs` prioritizes a separate terminal lane; stream forwarding never waits for ordinary capacity. HTTP completions can wait, preserving write outcomes. EOF, transport/parser/deadline errors and overflow terminate the stream attempt and schedule exactly one retry after a fixed three seconds, with no backoff or jitter. Failed channel/history reads remain read failures and do not trigger stream reconnect. The clock-anchored timer serves reconnect and expiry, never polling. The authenticated SSE transport, framing, deadlines and error classifications are unchanged.

Read deliveries carry session and request identities, not a stream-attempt identity. Stream deliveries retain session/attempt guards; writes retain session/operation guards. Stream failure does not cancel reads or writes, issue automatic GETs, reset history/pages/cursors or discard drafts, selection or write outcomes. The history view has no recovery-reset revision; disconnect/reconnect does not reset reading position. HTTP confirmations and events share ID-based entity merges; only confirmed originating operations clear their input. Uncertainty causes neither reads nor reconnect/resend, and matching text never confirms it. Older-page failure retains local retry. Views show Connecting initially and **Live updates disconnected — reconnecting.** after failure until the replacement stream is ready, regardless of cached rows or read outcomes; they have no stream tasks, retry schedulers or Refresh controls.

## Ownership and bounds

- `client/src/api/events.rs`: bound request, handshake validation, incremental SSE decoding, deadlines, bounded delivery and cleanup.
- `client/src/api/client.rs`: separate reusable HTTP and stream clients share URL/localhost/no-proxy/redirect/TLS policy. Ordinary HTTP keeps its eight-second total request/body timeout. The streaming client has **no total-body timeout**.
- `client/src/api/channels.rs` and `messages.rs`: HTTP and SSE use the same wire-to-client conversion. IDs, author identity, text-channel type and timestamps retain HTTP validation; empty message channel IDs cannot self-validate without an HTTP request context.
- Connection **and readiness together** have an eight-second deadline from submission. Headers or heartbeat bytes do not extend this budget. After readiness, any nonempty body chunk (including comments or partial lines) renews a 45-second byte-progress idle deadline; empty chunks do not.
- A frame has a 64-KiB encoded-byte bound, including fields, comments and line endings. Accumulation resets at frame boundaries, not transport chunk boundaries. Partial EOF frames are discarded. Network/socket buffers and the currently received chunk are separate from parser accumulation; this is not a total process-memory quota.
- The delivery channel holds at most **256 supported deliveries, including readiness**. Its producer never waits for consumer capacity: the next supported delivery ends the attempt with `Overflow`. A separate one-result task-completion channel makes termination observable without draining the backlog. Terminal results take priority, so already-queued creations may be discarded when an attempt ends; callers abandon the failed attempt's pending deliveries and reconnect, accepting these losses.

SSE handles optional initial BOM, arbitrary UTF-8/CRLF splits, CR/LF/CRLF line endings, multiline data, comments, last event-field selection and standard ignored fields. WHATWG UTF-8 replacement decoding is used for invalid sequences. Unknown SSE event names and valid unknown application types are ignored; additional JSON fields are accepted. Readiness requires an object and occurs once, before supported creations. Malformed supported JSON/entities, duplicate recognized fields/discriminators, wrong media type and oversized frames terminate the attempt. No-data frames follow standard SSE non-dispatch behavior.

## Verification

Owner-local suites: `client/src/api/tests/events.rs` (controlled transport/time through the same authenticated operation) and `client/src/api/tests/event_http.rs` (production reqwest adapter, actual server routes, real TCP cancellation).

Coverage includes every two-chunk split of a BOM/CRLF/CR/multiline/Unicode fixture, one-byte input, discarded final frames, exact 65,536/65,537-byte boundaries for each line ending, four maximum server-serialized 4,000-character control-text messages in a single >64-KiB chunk, and 4,000 astral characters encoded as 48,000 bytes of JSON surrogate escapes. These fit independently below the frame bound. Queue capacity 256 succeeds; delivery 257 terminates before queued results are drained. Controlled heartbeat traffic remains live across multiple ordinary-request deadlines, then ends exactly at the no-progress limit.

Real-loopback tests observe both creation types through independently authenticated consumers without polling or follow-up entity reads; invalid/duplicate/failed writes emit neither. The maximum legal escaped message retains HTTP/event identity. Additional tests prove redirect rejection without bearer forwarding, revoked-handshake rejection, real socket cleanup on handle drop, streaming past 8.2 seconds, and unchanged ordinary HTTP body timeout at approximately eight seconds. The network tests use controlled scheduling on a current-thread runtime, but the production transport/binding/parser are unchanged; they are not native UI acceptance.

Conversation coverage lives in `conversation/tests/{live_coordinator,delivery,state,live_state,coordinator,route}.rs`; semantic view coverage includes `views/tests/live_updates.rs` and `history_lifecycle.rs`. Tests exercise `ConversationHandle` with controlled HTTP/SSE/time rather than private scheduling internals. Real routes retain independent authenticated consumers, both HTTP/event confirmation orders, ID deduplication, loaded-only delivery and unchanged ordinary GET counts during healthy delivery. The actual-server-restart scenario verifies no reconnect before three seconds, zero catch-up GETs, accepted missing offline creations, retained active/inactive histories, older pages/cursors, drafts and selection, future creation delivery and explicit pagination through the retained cursor. Semantic views cover unfocused delivery, recreation, input/selection preservation and unchanged reading position across reconnect. Authentication rejection, expiry/logout and stale-session/attempt safety remain required; transport EOF is never authoritative rejection.

Historical #71 red/green and final check results remain in the [epic ledger](../implement-epic/renodubois-hamlet-71.md); its baseline-recovery assertions are not the current #72 policy. No dependency or lockfile changes are required by this policy change.

## Primary sources

- [WHATWG server-sent events](https://html.spec.whatwg.org/multipage/server-sent-events.html): UTF-8/BOM, line processing, blank-line dispatch and partial EOF semantics.
- [reqwest 0.12.28 ClientBuilder](https://docs.rs/reqwest/0.12.28/reqwest/struct.ClientBuilder.html): total request/body timeout versus connection/read timeout, redirect and TLS defaults.
- Installed reqwest 0.12.28 source: `src/config.rs:69-79` falls back to client timeout when a request timeout is absent; `src/async_impl/client.rs:2629-2635` installs the total timer; `src/async_impl/response.rs:315-331` provides incremental `chunk()` without enabling the separate stream feature. Clearing `Request::timeout_mut()` would not disable the ordinary client's timeout, which is why streaming uses a separate client.
