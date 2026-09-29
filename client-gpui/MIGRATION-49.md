# Controlled execution checkpoint — #49

Scope: [issue #49](https://github.com/renodubois/hamlet/issues/49), starting HEAD
`d7a7f3f27904eaa0409d94bf7a9521b1e77cfba6`. This supersedes only the execution/time
entries in [MIGRATION-48.md](MIGRATION-48.md). Session, conversation, API binding,
storage mechanics and child-view ownership migrations remain for later tickets.
The pre-existing architecture/plan/README and immutable #47 baseline are untouched.

## Implementation

- `main.rs` explicitly supplies `runtime::Execution` alongside API/config/storage.
  `app_shell::open` runs the same restoration/deletion/poll startup in production
  and headless lifecycle tests. There is no test-only send/history dispatch or
  conditional omission of the poll timer in this startup path.
- `runtime.rs` provides future delivery, cancellable bounded work, timers,
  monotonic time and wall-clock seconds. It has no feature/state/provider policy.
  Production futures run on the existing Tokio runtime, not on the GPUI thread.
  The controlled adapter runs them on the headless background executor; both use
  the same deadline race and result channel, then the same GPUI completion code.
  GPUI's background timer/monotonic clock is shared; controlled wall time is an
  explicit epoch plus elapsed virtual time. No dependency changes are required.
- The temporary coordinator supplies time to existing transitions and polling.
  Generation, history serial, request and storage/deletion identities are retained.
  Navigation cancels controlled reads as well as production reads. Write timeouts
  retain uncertainty/drafts, never prove non-delivery, and never replay writes.
- The storage worker still uses nonblocking bounded queue submission and its own
  blocking provider thread. FIFO, invalidation, rollback, metadata and credentials
  are unchanged. Only the workflow deadline constant moves out of storage to its
  current caller (the temporary coordinator).

### Deadline ownership

| Policy | Owner and retained semantics |
| --- | --- |
| HTTP **8s** | Unchanged `api/mod.rs` reqwest transport. No new outer timeout for login/signup/create/logout. |
| Read/send **9s** | `app_shell.rs::READ_SEND_DEADLINE`: channel reads, each selected history page and sends. One common bounded path, including controlled adapters. |
| Secure store **10s** | `app_shell.rs::SECURE_STORE_DEADLINE`: deletion; remember plus save replies together; storage read plus `/me` verification together. |
| Restoration | **One outer 10s budget**, not a reset after storage. The redundant inner 10s `/me` bound is removed; real HTTP retains its own 8s bound. With a 7s provider delay only 3s remain for verification. Timeout is retryable, not authoritative deletion. |
| Polling/expiry | Supplied monotonic time; unchanged 3s/15s focus schedule and existing backoff. Expiry uses injected wall time and a generation guard. The existing initial focus reconciliation on the first tick remains. |

Timers are registered when work is submitted. At the deadline, the common bounded
path drops its future and reports timeout to its caller. This does not interrupt
blocking provider work or establish cancellation/non-delivery of a remote write.

## Behavioral verification

New `views/tests/execution.rs` and `execution_storage.rs` exercise the actual
`open` lifecycle via Kit `Root`, real controls/stable IDs, injected API outcomes
and fake providers with isolated files. They never navigate private child fields:

1. Delayed login remains pending, duplicate submit is inert, then workspace opens.
2. Send stays pending at 8s and becomes uncertain at 9s; copy through the real
   textarea proves the draft remains; late response is cancelled and no replay occurs.
3. Old-server login completion cannot replace a newer pending login.
4. Controlled wall-clock expiry removes the workspace and clears password display.
5. Actual automatic ticks exercise 3s/15s polling, native focus pause/return and
   no protected requests after logout (not manual calls to private `poll_at`).
6. Provider read consumes 7s; restoration remains pending at 9s, times out at 10s
   total, ignores late verification, and neither deletes nor automatically retries.
7. Blocked save and queued deletion each become unconfirmed at 10s, without
   blocking local logout; late provider work still rolls back and deletes in order.

All 97 pre-existing scenarios remain. Legacy fixtures now inject controlled
execution and pump it rather than sleep waiting for a Tokio dispatch. Their
pre-existing private assertions remain for later ownership migrations. The
loopback HTTP journey explicitly injects production execution and no longer has
a fixture-only blocking history bridge. Its HTTP adapter/route coverage is intact.

Tests involving the real dedicated worker or loopback I/O explicitly allow GPUI
scheduler parking/foreign wakeups. Provider gates determine completion ordering;
application deadlines still advance only through the controlled clock. Five-second
wall-clock safety guards wait for worker/HTTP progress, not application deadlines.
These are not claims of deterministic OS thread scheduling or real-wallet coverage.

### Fresh checks (2026-09-29 UTC)

From `client-gpui/`, with the existing lockfile/cache:

- Repeated `cargo check --locked --all-targets` and targeted view/execution tests.
- `cargo fmt --check` — passed.
- `cargo clippy --locked --all-targets -- -D warnings` — passed.
- `cargo test --locked` — **104 passed, 0 failed/ignored**.
- `cargo build --locked` — passed.
- Execution suite repeated with `SEED=1`, `2`, `3` — 7 passed each.
- `git diff --check` — passed.

The first tracer test failed because injected execution/construction did not yet
exist, then passed through the common lifecycle. Subsequent runs caught old
fixture scheduling/cancellation assumptions, incorrect initial UI-label/control
oracles, and the need to allow foreign wakeups for the real worker; all were fixed
before the successful runs. No production feature behavior was changed to satisfy
those fixture assumptions. Full command output is in the parent handoff artifacts
at `/tmp/hamlet-orchestrator/review-49/`.

## Preservation and handoff

The six starting unrelated files (`.gitignore`, `CONTEXT.md`, `client-gpui/README.md`,
`client-gpui/ARCHITECTURE.md`, and both client plan files) are excluded from staging;
starting status and SHA-256 hashes are captured and checked at handoff. No API,
server, dependency/lockfile, persisted-format or semantic-control-ID changes.
No desktop automation, real profile/provider/credentials, or issue mutations.
Native IME/accessibility/physical input, delayed native anchoring and locked/slow
real-wallet limitations remain unverified as recorded in `VERIFY.md`.

Parent owns the mandatory parallel Standards/Spec review and any resulting fixes.
Pending parent review is expected, not an implementation blocker. Do not advance
to #50 or close #49 from this checkpoint.
