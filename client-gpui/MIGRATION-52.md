# Protected bound-client checkpoint — #52

Scope: [issue #52](https://github.com/renodubois/hamlet/issues/52), pinned start
`000d6666ba73b8ae556172a83dc21821ebc9380a` on `rewrite`. Issue body, label
(`ready-for-agent`) and comments (none) were fetched into the handoff artifacts.
This retires protected API compatibility only; **no #53 or later ownership
extraction** is included. The pre-existing architecture and approved plan are
unchanged.

## Implementation and ownership

- Channel discovery/creation, initial/older/catch-up history and message sends
  capture the accepted `AuthenticatedClient` through `AppSession::client_for`.
  Dispatch supplies operation arguments only. The generation check prevents
  an old descriptor from borrowing a newer context; captured clients remain
  immutable and bound to their original server/credential.
- `ReadRequest`, `CreateRequest` and `SendRequest` no longer carry a URL or token.
  Session generations, channel identity, request serials, history task serials,
  cursor checks and pending-state checks still gate completions. Generations
  change on logout, server changes and subsequent authentication; old success
  or rejection cannot populate or invalidate a newer session.
- Startup/session construction now accepts `HttpTransport` directly. Removed
  `api/legacy.rs`, `api/legacy_fixture.rs`, `AuthApi`, `HttpAuth`, test `Login`,
  `Session::token`, and the `api as http` alias. Storage's three URL-validation
  references simply use the canonical module name; storage mechanics are unchanged.
- One network implementation remains in `api/`. Newest history uses
  `history_page(channel, None)`, retaining its cursor; the unused vector-only
  `history` convenience was removed, including from contract tests. Only session
  persistence accesses credential material in production.
- No channel ordering/creation-selection, history merging, tied-timestamp order,
  server-ID deduplication, draft, send-uncertainty or polling algorithm changed.
  HTTP 8s, read/send 9s and combined restoration/storage 10s budgets remain intact.
  No write replay, implicit reauthentication, route or transport-policy change.

Session/conversation task ownership and the temporary monolithic shell remain for
later tickets. Existing state/type re-exports and private-view assertions are not
claims that the later coordinator/view/storage extractions are complete.

## Behavioral coverage

All **113 starting scenarios remain**, plus one new real-control lifecycle test:
`protected_contexts_survive_logout_changed_server_and_late_old_results` exercises
both delayed channel success and authoritative rejection after logout, changed
server and subsequent login. It observes the newer workspace/history and asserts
origin/bearer isolation for protected reads and captured old-client revocation.
It uses Kit Root, semantic IDs, controlled responses/time, not private view fields.

All old controlled protected fixtures now receive already-bound requests and
return wire responses through the same API decoding as real HTTP. Their prior
synthetic `"same"` timestamps were replaced with valid tied RFC3339 timestamps;
controlled failures now supply the actual error envelopes. Existing validation,
conflict, malformed-success, ambiguous-write, pagination and race assertions remain.

The existing Bob/Alice real rewrite-route polling journey now uses production
`HttpTransport` directly, real login/logout controls and the actual lifecycle poll
timer. It waits for semantic message/channel controls rather than an API forwarding
fixture's completion side channel. A five-second wall-clock guard waits only for
loopback/thread progress; polling still advances the GPUI clock. No private root
fields, alternate transport wrapper or manually invoked `poll_at` remain in that
journey. Existing bound-client tests retain controlled and two-origin loopback
coverage for clones, credentials, every operation and opaque cursor encoding.

TDD: the new lifecycle tracer first failed to compile because startup still
required `Arc<dyn AuthApi>`, then passed with direct transport construction and
bound dispatch. Its input sequence was corrected to allow server-change password
cleanup to settle before entering the next password. Targeted fixture migrations
caught invalid timestamps/error envelopes and an HTTP completion scheduling
assumption, all resolved in fixtures without changing product behavior.

## Verification

From `client-gpui/`, using existing dependencies/lockfile/cache, on 2026-09-29 UTC:

- Repeated `cargo check --locked --all-targets`: passed; production-only check also ran.
- Targeted API: **23 passed**; views: **39 passed**; conversation/polling: **31 passed**.
- Final real-route polling journey and new binding lifecycle test, each with
  `SEED=1`, `2`, `3`: passed.
- `cargo fmt --check`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo test --locked`: **114 passed, 0 failed/ignored**.
- `cargo build --locked`: passed; compiled only, no desktop launch.
- `git diff --check` and all six preservation SHA-256 checks: passed.

Issue JSON, pinned start, hashes, check logs, commit list, three-dot review diff
and handoff are under `/tmp/hamlet-orchestrator/review-52/`. Mandatory parallel
Standards/Spec review is **pending parent** after this scoped commit, not an
implementation blocker.

## Preservation and limits

Byte-preserved and excluded from staging: original modified `.gitignore`,
`CONTEXT.md`, `client-gpui/README.md`, and untracked `client-gpui/ARCHITECTURE.md`,
`docs/plans/client-gpui-prototype.md`, `docs/plans/client-gpui-rearchitecture.md`.
No server, dependency/lockfile, runtime, storage-format, baseline, layout or semantic
control-ID changes. No real credentials/keyring, desktop automation, issue mutation,
push, branch switch, stash or reset. Tests use synthetic data, fake providers,
isolated files/databases and loopback HTTP only.

No substantive implementation blocker. Native IME/accessibility/physical-input,
precise delayed native anchoring and locked/slow real-wallet limitations recorded
in `VERIFY.md` remain unverified. Do not close #52 or begin #53 from this checkpoint.
