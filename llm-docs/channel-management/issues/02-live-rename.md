## Parent

Part of renodubois/hamlet#77. This slice extends the working sidebar rename flow with best-effort shared updates.

## What to build

Connected users see other users’ channel renames immediately through the existing authenticated live stream, without losing their selected conversation or causing extra reads.

## Acceptance criteria

- [ ] Successful renames publish a shared-protocol event carrying the updated channel through the existing best-effort hub. Failed operations publish no success event; unchanged-name no-ops need not publish.
- [ ] Extend the existing API event decoding and workspace delivery/merge behavior, not the views’ transport responsibilities.
- [ ] Received renames update names and sidebar order by channel identity, preserving selected channel, messages, draft, and history/reading state; do not add duplicates or perform follow-up reads.
- [ ] Handle a single rename’s HTTP confirmation and live event in either arrival order without duplicate state or requests. A live event can update the channel but cannot confirm or close the originating request’s dialog.
- [ ] Preserve last-successful-rename server semantics without introducing revisions, stale-edit rejection, or a global event ordering system.
- [ ] Previous-session or obsolete deliveries cannot modify a newer authenticated workspace.
- [ ] Keep existing fixed reconnect and best-effort limitations: missed renames need not be repaired; no replay, polling, catch-up reads, or automatic write retries.
- [ ] Prove two-user rename propagation with the existing workspace-to-real-server HTTP/SSE harness and response/stream gates; cover both response/event orders, selected and unselected channels, failed mutations, and absence of extra reads. Use existing headless sidebar and controlled stream fixtures for deterministic visible-state checks where useful.
- [ ] Run applicable documented automated checks without native desktop or real credential-store access.

## Blocked by

- renodubois/hamlet#78 — Rename channels through the sidebar.
