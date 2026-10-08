## Parent

Part of renodubois/hamlet#77. Complete shared deletion behavior using the established live rename and local soft-deletion flows, respecting ADR-0001 and existing best-effort delivery limits.

## What to build

Other connected users see a deleted channel disappear, lose its local conversation state, and move to a remaining channel when necessary. Asynchronous rename/message/history results cannot bring a channel back after this workspace knows it is deleted.

## Acceptance criteria

- [ ] Successful deletions publish an event identifying the deleted channel through the existing authenticated best-effort stream. Failed, missing, repeated, and last-active-channel-rejected deletions publish no success event.
- [ ] Extend shared protocol/event decoding and existing workspace delivery transitions; do not put stream/HTTP policy in views or introduce a general synchronization framework.
- [ ] A received deletion removes the channel and discards its cached history, draft, and channel-specific operation state. If selected, use the same first-remaining-locally-known fallback as local deletion; if none are known, clear selection without reconciliation. Removing an unselected channel preserves selection.
- [ ] Local HTTP deletion confirmation and its live event work in either order and tolerate repeated delivery without duplicate cleanup effects or navigation/history requests. The live event is not confirmation of the originating dialog request; only its confirmed successful HTTP response closes it automatically.
- [ ] Known-deleted channel state cannot be resurrected by late rename confirmations/events, channel creation deliveries, history completions, send confirmations, or message deliveries. Ensure local and remote deletion exercise the same rule and previous-session results remain isolated.
- [ ] Preserve inline dialog feedback if an in-flight rename/delete becomes missing because another user deleted its target. Do not replace an uncertain request outcome with automatic success solely because an event arrived, retry mutations, or roll back a delivered deletion.
- [ ] Preserve existing best-effort reconnect behavior. Missed deletion notifications may leave stale channels/cached messages until workspace reopening, while the server blocks deleted access; do not add replay, polling, catch-up reads, or refresh controls.
- [ ] Prove two-user deletion through the existing real HTTP/SSE workspace harness with response/stream gates: selected/unselected channel, both event/response orders, replacement navigation, no locally known replacement, and no additional reconciliation reads.
- [ ] Extend existing controlled stream/coordinator and headless real-control tests for late deliveries, duplicate events, visible fallback/cleanup, dialog confirmation/error behavior, and uncertainty without mutation replay. Use existing operation publication tests only where route gates cannot demonstrate publication outcomes.
- [ ] Verify failed mutations generate no phantom shared changes, restart/reopening excludes deleted channels, and disconnected clients retain the explicitly accepted missed-event limitation.
- [ ] Run the documented automated checks across affected components; do not launch native desktop automation or access a real credential store.

## Blocked by

- renodubois/hamlet#79 — Deliver live channel renames. Required to exercise live rename/deletion interactions and prevent late renames resurrecting deleted state.
- renodubois/hamlet#80 — Soft-delete channels through the sidebar. Provides deletion persistence, access rules, local cleanup, and fallback behavior to propagate live.
