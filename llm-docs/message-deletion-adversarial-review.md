# Message deletion: adversarial review and native evidence

Date: 2026-10-12. Reviewed implementation `a79b4b382f154c9e862928dc35cfbb89ad4cbe17` against its parent `a94c3272dcbac133d8374f84da0fd73901b72e5a`, including GitHub issues #85–#88 and [the agreed spec](message-deletion-spec.md). Corrections are uncommitted working-tree changes. The worktree was clean before review.

## Standards

Independent parallel review of the complete implementation diff found no substantiated standards breaches. Review of the corrections identified a redundant viewport-wide layout pass on unchanged frames and a test accessing a mounted view's private scroll state. Both were corrected: padding has a bounded, view-local geometry/content/anchor cache, zero-padding protection retires, and the unknown-measurement predicate test uses an independent `ListState`. Geometry and action journeys continue using real controls; the narrow view test hook observes only measurement counts.

Final outstanding Standards findings: **0**. The layout concern was a heuristic/performance finding, not a hard architecture violation.

## Spec: failures found and fixed

All four original feature defects below have failing-before/passing-after regressions.

| Defect | Correction |
| --- | --- |
| Width resize invalidates offscreen height hints; deletion treated unknown bottom status as permission to jump to latest. | Explicit reader-follow intent survives layout changes instead of inferring intent from unknown measurements. |
| A tall authored message's top can scroll outside the pane, leaving its hover/focus trash action unreachable. | Position the actual control during prepaint using the pane and owning row bounds; keep paint and hitboxes masked to its owner. Keyboard focus reveals vertically clipped controls when the pane can fit them. |
| Deleting a tall latest message leaves less than a viewport of surviving content below the reader; GPUI backfills preceding rows and moves the surviving anchor to bottom. | Measure only a viewport-worth of actual tail at current width and supply bounded presentation-only bottom padding. It is not a message placeholder or authoritative history. Appends consume space; deliberate downward scrolling or Jump to latest retires it. |
| Height-only shrink retains GPUI's old top offset; deletion misclassifies a formerly bottom-following reader as scrolled up. | Apply explicit follow intent before resized list layout, even without changed message IDs. |

Adversarial re-review caught two regressions in preliminary corrections before completion: pane-only tray clamping could overlap neighboring rows for an 8px sliver, and downward wheel input failed to resume following when a padded viewport contained all survivors and GPUI returned `None`. Owner hitbox/paint masks plus focus reveal address the former; a separately tested known-fitting predicate distinguishes the latter from unknown measurements.

Final independent Spec recheck: **0 concrete remaining findings**. No confirmed server ownership, soft-retention, event-publication, or client anti-resurrection defect was found.

## Native safety and environment

The user explicitly authorized the current desktop session. Read-only DMS preflight established an unlocked, active session; input operations rechecked the gate. Desktop: niri 26.04 (`8ed0da4`), X11 client windows through Xwayland/Xwayland-satellite. Input was OS-level XTEST pointer/keyboard injection, **not physical device input**.

- Fresh worktree-owned SQLite database and loopback endpoint `http://127.0.0.1:8082`; no inherited/shared database.
- Two fresh profiles, A/B, under `client/.env.deletion-review/`; no copied session metadata.
- Private D-Bus configured without activation directories. GNOME Keyring 51.1 ran with private HOME/config/data/cache/runtime and only its secrets component. The private login collection's `Locked` property was `false`, and bus ownership was checked before launching clients. No ordinary-wallet service was activated or accessed; no stored secret value was inspected.
- Disposable identities and synthetic messages only. HTTP credentials remained in process memory; no request/response traffic dumps. Evidence contains owned-window screenshots, synthetic UI text, test output and a pixel comparison—not login/password screens or bearer material. Screenshots used X11 window capture, not full desktop capture or the host clipboard.
- Owned client/server/provider/bus processes and private runtime resources were stopped/removed after evidence was preserved. The original terminal window was refocused. No system packages or desktop configuration were changed.

## Native outcomes

### Shared confirmation and two-client delivery

1. GUI signup/login, sending and observing synthetic messages worked. A nonauthor's hover exposes no trash action: [screenshot](evidence/message-deletion-adversarial/B-other-author-no-trash.png).
2. Moving the pointer across a visible row onto its trash control opened confirmation without deleting. Initial Enter cancelled; SQLite verified the row remained undeleted. Context-menu Down/Enter opened the same confirmation.
3. Deliberate GUI confirmation through **both** tray and context menu removed the target, and the second client received live removal. Retained SQLite rows preserved text/author/channel/creation fields and gained `deleted_at`.
4. Repeated the shared confirmation paths on the final build. A's context confirmation was open for row 074; B, signed in as the same author, confirmed its tray deletion. A's target dialog closed without a submission from A. See [B confirmation](evidence/message-deletion-adversarial/B-latest-confirm.png), [B removal](evidence/message-deletion-adversarial/B-latest-deleted.png), and [A dialog closed](evidence/message-deletion-adversarial/A-latest-other-window-closed.png).
5. On the final build, A then confirmed row 072 through its context menu; B observed its removal. See [context confirmation](evidence/message-deletion-adversarial/A-latest-context-delete-ready.png), [A result](evidence/message-deletion-adversarial/A-latest-context-deleted.png), and [B result](evidence/message-deletion-adversarial/B-latest-context-observed.png).

### Native viewport and pagination

- Tall-row clipping was exercised with a 44-line message whose original top was above the pane. The final visible trash control remained inside the pane and opened the bounded shared confirmation: [hover](evidence/message-deletion-adversarial/A-latest-clipped-hover.png), [confirmation](evidence/message-deletion-adversarial/A-latest-clipped-confirm.png).
- Native testing discovered the large-tail shrink defect. In the earlier failing build, row 071 visibly moved after deletion: [before](evidence/message-deletion-adversarial/A-resized-before-delete.png), [after](evidence/message-deletion-adversarial/A-resized-after-delete.png).
- Final build: scrolled reader, width resize, then tall latest-message deletion with insufficient surviving tail and **no intervening wheel input**. Surviving text crop `(243, 60, 580, 210)` was byte-for-byte pixel-identical before/after, and Jump to latest remained available. [Before](evidence/message-deletion-adversarial/A-latest-tail-before.png), [after](evidence/message-deletion-adversarial/A-latest-tail-after.png), [comparison record](evidence/message-deletion-adversarial/pixel-proof.json). Jump to latest subsequently returned to the real bottom.
- Final build: height-only shrink at bottom, then deletion without wheel input, retained bottom-follow: [before](evidence/message-deletion-adversarial/A-final-height-bottom-before.png), [after](evidence/message-deletion-adversarial/A-final-height-bottom-after.png).
- Deleting the loaded newest rows left **Load older messages**, not false exhaustion or automatic visible refill: [empty loaded history](evidence/message-deletion-adversarial/A-empty-loaded-history-older-control.png). Clicking it successfully retrieved older rows through the existing cursor, whose referenced row had been deleted: [older history](evidence/message-deletion-adversarial/A-older-after-deleted-cursor.png). Network request-count/no-backfill guarantees are established by controlled tests, not visual inference.

### Independent real-HTTP adversarial checks

On the isolated server, assertions established: unauthenticated deletion 401; nonauthor deletion of active and already-deleted messages 403; malformed message ID 400; wrong/missing channel 404; eight concurrent author deletions all 204; deliberate repeat 204; first deletion timestamp unchanged; text/identity/channel/creation fields retained; authoritative history excludes the target. The automated real-route/SSE suites additionally establish single-event publication, silence on rejection/repeat, races and cursor continuity.

## Automated final checks

From each component directory: `cargo fmt --check`, strict all-target `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, and `cargo build --locked`. Protocol lint/test/build also used `--all-features`.

- **Client: 254 passed.** Added regressions cover unknown/known fitting measurements, width/height resize intent, tall-tail deficits, appends/cache reuse, deliberate wheel/jump retirement, clipped tall trays and top/bottom slivers.
- **Server: 51 passed; 1 pre-existing opt-in fanout test ignored.**
- **Protocol: 17 passed.**
- `git diff --check` passed.

Logs, failing-before regression output, screenshots and `SHA256SUMS` are under [evidence/message-deletion-adversarial/](evidence/message-deletion-adversarial/).

## Remaining verification boundary

These native runs establish the stated deletion journeys on this Linux desktop; they are not universal native acceptance. Physical input hardware, assistive technology, IME, selection/copy across applications, slow/locked-wallet drills, native HTTP fault injection, packaging and non-Linux platforms remain unverified in this review. Controlled automated coverage remains the evidence for pending/uncertain HTTP outcomes, stale session/race handling and exact request counts.

When less than a full 32px tray fits in a row's visible intersection, hover does not scroll or overlap another row: the control clips at the pane edge. Keyboard focus reveals it when the pane and row can fit it; an exceptionally short pane can still clip it. Best-effort missed deletions remain an agreed limitation, not a newly introduced recovery mechanism.
