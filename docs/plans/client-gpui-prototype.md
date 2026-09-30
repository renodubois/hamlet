# GPUI client prototype — agreed direction

Status: design interview finalized. Implementation has not started. Verify the Linux GUI test harness before committing to detailed framework integration.

## Settled direction

- Evolve the existing `client-gpui/` starter into a foundation for the eventual client, rather than a disposable prototype.
- Use GPUI with gpui-kit as the base.
- Linux is the first verified platform. Verification of other platforms is deferred; do not deliberately block them.
- Deliver a run-from-source workflow with documented native dependencies, server/client startup, and repeatable test commands. Defer release artifacts, installers, and desktop packaging.
- Integrate with `server-rewrite/`: signup, login, channel creation, message history viewing, and message creation.
- Render messages as selectable/copyable plain text with preserved line breaks, author, and timestamp. Defer Markdown, automatic link handling/previews, attachments, editing, and reactions.
- Layout: channel list on the left; history fills the remaining main pane; composer below history. No dedicated full-screen composer is required.
- Sending waits for server confirmation rather than inserting optimistic messages. Show a sending state, prevent duplicate submission while pending, retain the draft on failure, and clear the submitted draft after confirmed success.
- Make the originating channel’s composer read-only while its send is pending, with a bounded request timeout. Channel navigation and other channels remain usable; completion affects only the originating channel.
- Keep the server unchanged for uncertain send outcomes. Retain the draft, refresh history, and warn that the message may already have been published; never automatically retry message creation. Do not treat matching text as proof of successful submission. Re-examine this trade-off later, particularly server-supported idempotent sending.
- Keep per-channel drafts in memory across channel switches. Clear them on logout; do not persist them across restarts.
- Enter sends; Shift+Enter inserts a newline. Confirming IME composition with Enter must not send. Keep a send icon available.
- Open channels at the newest history page and load older pages when scrolling upward. Preserve the reading position; incoming messages must not force a reader of older history to the bottom. Provide a jump-to-latest control when appropriate.
- The user owns visual design. Keep presentation easy to modify, providing only basic layout initially.
- Start with styled gpui-kit controls rather than building a custom design system on unstyled primitives. Keep feature behavior separate from presentation, and make app layout, theme values, and icon choices easy to locate and replace.
- Use icon-first action controls with tooltips and accessible names. Keep readable text for form labels, validation errors, and connection/session failures. Start with bundled icons and centralize the app’s icon mapping for later replacement.
- Test GUI functionality rather than exact pixel positions; visual redesigns should not break behavioral tests.
- Use broad application-behavior tests plus focused headless GPUI interaction tests that exercise real controls and keyboard actions. Do not rely on behavior tests alone to verify GUI wiring. Verify tooling feasibility before settling test mechanics; full GUI journeys against a real server are not required as the primary test strategy.
- Prefer maintainable feature modules and real server integration; no speculative extension framework.
- Use future live-conversation features (server push, unread indicators, typing state, reconnects) as the primary architecture pressure test, not as current implementation scope.
- Poll the selected channel while the app is active and provide manual refresh. Do not add server push for this prototype. Refresh defaults and history-correctness requirements are recorded below.
- During transient network failures, preserve loaded history and drafts in memory, show connection status, and retry reads with backoff. Do not persist history to disk or queue writes for later delivery.
- Automatically refresh the channel list while active, at a lower frequency than the selected conversation, and on reconnect/manual refresh. Do not fetch histories for unselected channels or implement unread counts yet.
- Provide an editable server URL on the login screen; connect to one server at a time.
- Require HTTPS for non-local connections; allow HTTP only for loopback development addresses. Passwords and bearer tokens must not be sent over insecure remote connections.
- Remember login across launches using the Linux secret store for bearer tokens; never store passwords. If secure storage is unavailable, use an in-memory session and explain that login will not persist.
- Treat session expiry or an authoritative authentication rejection like logout: clear the token, session data, and drafts, stop protected work, and return to login. Draft-preserving reauthentication is out of scope.
- Explicit logout clears local credentials and conversation state immediately and attempts server revocation with a bounded timeout. Return to login even if the server is unreachable; warn when server revocation cannot be confirmed.

## Architecture direction

- Keep a single client crate, organized into feature modules for session, channels, and conversation behavior.
- Keep workflow state and decisions independent of GPUI in ordinary Rust modules: session lifecycle, drafts, request state, and history merging.
- GPUI views own rendering, focus, scrolling, and input, bridging to feature behavior through explicit interfaces.
- HTTP integration hides bearer handling, wire formats, opaque cursor plumbing, and error decoding from views.
- Centralize app presentation conventions (theme values and icon mappings); keep layout in identifiable view files.
- Do not introduce a generic plugin framework or speculative abstractions for unimplemented features.

## Existing constraints

- `client-gpui/` currently contains a minimal gpui-kit 0.6 starter.
- `server-rewrite/` exposes an HTTP contract in `openapi.json`, including logout and current-user lookup.
- Message history is paginated using opaque cursors.
- The server has no live message delivery endpoint.

## Tooling findings

- The locked gpui-kit 0.6.1 source includes `test-support`, headless UI interaction helpers, and bundled icon assets. This supports the proposed testing direction; execution on this Linux environment remains unverified.
- Official upstream documentation describes styled controls over an unstyled behavior layer, headless interaction testing, and bundled Lucide icons: <https://github.com/longbridge/gpui-kit>.

## Accepted operational defaults

- Poll selected-channel messages every 3 seconds and the channel list every 15 seconds. Pause periodic polling while the window is unfocused; refresh on return. Keep timing easy to change without editing views.
- Prevent overlapping polls for the same resource; back off read retries on transient failures. Manual refresh and reconnect must not introduce duplicate concurrent work.
- Default to `http://127.0.0.1:8081`. Remember the last successfully used server URL separately from its secure token.
- Signup enters the authenticated app immediately, using the session returned by the server.
- Initially select the first channel in the server's alphabetical list. After successful channel creation, select the created channel. Handle an empty list explicitly rather than assuming bootstrap always provides a channel.
- Keep requests asynchronous and bounded; network and secret-store work must not block the UI thread.
- Ignore stale responses after logout or server changes. Route channel-specific completions to their originating channel, never whichever channel happens to be selected later.
- Reconcile overlapping history pages by message identity without duplicates or silently skipped messages. Fetching only the newest page is insufficient when more than one page arrived between polls.

## Correctness requirements for implementation

These follow from the agreed workflows and should guide both module interfaces and tests:

- Scope credentials and conversation state to the selected server and authenticated user. Clear session-owned state on logout, expiry, or authentication rejection; invalidate outstanding work so late completions cannot repopulate it.
- Restore stored sessions through the server's current-user lookup. Distinguish a temporarily unreachable server from a definitively invalid session; a network timeout alone must not delete an otherwise valid login.
- Never forward passwords or bearer tokens to another server through a redirect. Keep normal TLS certificate verification enabled, and do not log credentials or message payloads.
- Report secure-store failures honestly. Memory-only fallback must be visible, and failed credential deletion must not be presented as successful removal from secure storage.
- Retain history already loaded during refresh failures. Keep first-load, loading-older, refreshing, empty, and failed states distinguishable without depending on a particular visual design.
- Preserve server ordering, including equal timestamps. Treat returned history cursors as opaque and use only server-issued cursors for pagination.
- Handle a burst larger than one page by catching up through history until continuity is established. Do not present disconnected history segments as a complete contiguous conversation. Catch-up work must remain cancellable and must not block interaction.
- Preserve the reader's position when prepending older messages or incorporating new ones. Follow new messages when already at the latest position, not when reading older history.
- Sending completion updates only the originating channel within the originating session. Polling and send confirmation may observe the same message; it must appear only once.
- Do not automatically replay state-changing requests after ambiguous transport failures. In particular, message creation must never silently retry. Keep uncertainty distinct from an explicit server rejection.
- Match server validation and display useful errors without clearing recoverable user input. The server remains authoritative.

## Test strategy

### Application behavior

Exercise the same feature interfaces used by the views, with controlled request outcomes and time. Cover:

- Signup/login success and rejection; session restoration; expiry; logout; unavailable secure storage.
- Channel loading, creation, duplicate-name rejection, selection, and per-channel draft retention.
- Initial history, older-page loading, overlap deduplication, bursts larger than one page, and refresh recovery.
- Confirmed send, explicit rejection, uncertain outcome, repeated submission prevention, and completion after channel switching.
- Out-of-order responses, logout during pending work, and responses belonging to an old server/session.
- Poll scheduling, focus pause/resume, backoff, and cancellation without real-time sleeps where possible.

### GUI interaction and wiring

Use gpui-kit headless interaction tests with stable semantic control IDs, not fixed coordinates, screenshots, or presentation-specific tree paths. Exercise actual controls and input, then assert observable behavior:

- Login/signup form submission and validation feedback.
- Channel selection and creation controls.
- Composer input, send-button wiring, Enter/Shift+Enter, pending read-only state, and retained input after failure.
- History scrolling that requests older pages, plus jump-to-latest behavior without hard-coded pixel assertions.
- Refresh/logout controls, keyboard focus, disabled states, and accessible names for icon actions.

A test may discover a control's current bounds to interact with it; it must not assert that the control lives at a predetermined coordinate. Styling changes should not require changing behavioral expectations.

### Integration and manual verification

- Test HTTP integration against the rewrite server's documented contract, including bearer handling, response decoding, pagination, and error mapping. Use targeted integration tests rather than requiring every GUI test to launch the server.
- Manually smoke-test the Linux application against a running rewrite server, ideally with a second client/user for channel and message discovery.
- Verify Linux secret-store behavior, native text selection/copy, and IME composition manually where headless input cannot faithfully exercise the platform integration.
- Document build prerequisites and repeatable format, lint, and test commands. Do not claim headless behavior tests verify native rendering or packaging.

## Implementation milestones

1. **Feasibility gate:** prove the locked gpui-kit version builds on Linux and that a real input/button interaction test runs headlessly with semantic IDs. Check variable-height history scrolling and selectable message text support. If a required capability is unavailable, report the limitation before substituting a weaker design or test strategy.
2. **First vertical slice:** establish the single-crate feature/view structure and build real login → channel selection → history → send, with behavior tests and focused GUI wiring tests. Keep rendering minimal.
3. **Complete workflows:** add signup, channel creation, secure session restoration/logout, expiry handling, per-channel drafts, older-history loading, polling, and failure recovery. Add tests with each behavior rather than batching them at the end.
4. **Prototype exit:** run the complete verification suite and manual two-user smoke test; document startup and test commands, presentation-editing locations, and remaining limitations. Review the design against replacing polling with server push without rewriting views.

## Deferred work and revisit points

- Re-examine uncertain sends and server-supported idempotency before promising reliable retry or adding an offline outbox.
- Server push, unread indicators, typing state, and richer reconnect protocols are future architecture pressure tests, not current features.
- No Markdown, link previews, attachments, editing, reactions, additional channel types, administration, or multi-server navigation.
- No persistent history cache, durable drafts, queued sends, packaging, or cross-platform verification yet.
- The user retains ownership of visual design; this work establishes usable structure and replaceable presentation, not a finished visual identity.

## Verification status

Repository code, the locked dependency's test documentation, and upstream documentation were inspected during the interview. No application implementation, compilation, or test execution was performed. Detailed module interfaces and framework mechanics remain implementation work, constrained by the agreed direction above.
