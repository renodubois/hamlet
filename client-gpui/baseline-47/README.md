# GPUI migration baseline — #47

Recorded 2026-09-29 against ticket-start HEAD `13e145ef48b1254b4a92ebbd6c502783f9e9cb5b` on `rewrite`.
Spec: [issue #47](https://github.com/renodubois/hamlet/issues/47), fetched with `gh issue view 47 --json number,title,body,comments,labels,url` and `gh issue view 47 --comments`: label `ready-for-agent`, no comments. Authority: [approved migration plan](../../docs/plans/client-gpui-rearchitecture.md), stage 0, and [target architecture](../ARCHITECTURE.md).

This is baseline evidence, not the ownership migration. Source references below refer to the ticket-start revision. No production source, dependencies, server routes, or existing tests changed. No new test seam was selected; the JSON fixtures are static compatibility examples, not new executable tests. Later tickets must preserve scenarios, not a fixed test count.

## Fresh automated evidence

From `client-gpui/`, sequentially, using the existing lockfile and warm `target/` cache:

| Command | UTC start → end | Result |
| --- | --- | --- |
| `cargo fmt --check` | 15:11:59 → 15:12:00 | exit 0 |
| `cargo clippy --locked --all-targets -- -D warnings` | 15:12:00 → 15:12:00 | exit 0 |
| `cargo test --locked` | 15:12:00 → 15:12:02 | exit 0; 97 passed, 0 failed, 0 ignored, 0 filtered out; 1.51s test execution |
| `cargo build --locked` | 15:12:02 → 15:12:02 | exit 0 |

Full command output, toolchain, dependency-file hashes, and starting status are in [checks.txt](checks.txt). This is a fresh invocation of each command and a fresh test execution, **not** a clean rebuild and not a reuse of #46's historical results. No dependencies were installed/upgraded to make it pass. Fresh results agree with the historical count but stand independently of it.

Environment: Linux x86_64, `7.2.7-arch1-1`; active `stable-x86_64-unknown-linux-gnu`; rustc `1.95.0 (59807616e 2026-04-14)`, Cargo `1.95.0 (f2d3ce0bd 2026-03-21)`. `DISPLAY`, `WAYLAND_DISPLAY`, `CARGO_TARGET_DIR`, and `RUSTFLAGS` unset. A session-bus address was present but was neither recorded nor used for provider operations. Native development library versions are in the evidence file.

Safety: only existing headless/controlled tests and their disposable loopback HTTP/SQLite fixtures ran. `persistence.rs:19–20,38–49,96–97,107–129,172–175` compile the real configuration path/provider construction only outside tests; `main.rs:132–135,232–235` uses default config and no provider in tests. Storage tests explicitly inject controlled stores and temporary paths. `cargo build` compiles but does not launch the desktop binary. No real credentials/configuration files, real keyring, desktop automation, or external server were accessed.

### Failures and limits

- **No automated baseline failures observed.** Nothing currently blocks subsequent format/lint/test/build checks; no failure was fixed or waived. Reproduce with the four exact commands above before attributing later failures to migration changes.
- This does not establish a cold-cache/offline build, non-Linux support, packaging, or native behavior. No clean rebuild was requested/performed.
- Native IME candidate/Enter, assistive technology and physical keyboard traversal, precise delayed-prepend native pixel anchoring, and locked/slow real-wallet behavior remain **unverified**, as in [VERIFY.md](../VERIFY.md). Separate consent and the relevant desktop/devices/provider are prerequisites for those later native checks. They do not block this evidence-only ticket or headless checks, but block claiming complete native acceptance.
- Existing tests use alternate `cfg(test)` send/history scheduling (`main.rs:528–562,876–920`) and direct root/private state access. Passing them does not establish the future shared executor or child-view lifetimes. See [SCENARIOS.md](SCENARIOS.md) for relocation risks; do not paper over them with public-field exports.

## Working-tree preservation

Starting `git status --short` (also captured in `checks.txt`):

```text
 M .gitignore
 M CONTEXT.md
 M client-gpui/README.md
?? client-gpui/ARCHITECTURE.md
?? docs/plans/client-gpui-prototype.md
?? docs/plans/client-gpui-rearchitecture.md
```

These six files are unrelated pre-existing work, read only for context where relevant and excluded from this ticket's staging/review diff. Their SHA-256 fingerprints were captured outside the repository and checked after authoring the baseline. All ticket additions live under `client-gpui/baseline-47/`. The pre-existing architecture/plan documents remain untracked by this ticket; their availability in a clean clone depends on the parent task publishing them separately. Do not stage them as #47 implementation.

## Semantic UI surface

Inventory from `src/main.rs:1130–1589`; all IDs are stable strings, not tree positions. Kit `Root` is mounted at `main.rs:1615` and provides selection/copy integration. Container IDs without a listed label are structural/test targets, not claims of native accessibility verification.

| ID(s) | Label / accessible name / tooltip and behavior |
| --- | --- |
| `server-url`, `username`, `password` | Visible and aria labels `Server URL`, `Username`, `Password`; password input masked. Server edits clear password and invalidate old work. |
| `login`, `signup` | `Log in` / `Create user`; pending `Signing in…` / `Creating user…`; duplicate submission disabled/inert. |
| `auth-mode` | `New user? Sign up` / `Have a user? Log in`; disabled during pending auth. |
| `auth-feedback` | Current auth feedback is both text and aria label. |
| `session-status`, `logout` | `Logged in as {username} at {server}`; `Log out` clears locally before cleanup/revocation. |
| `storage-status`, `retry-storage` | Storage feedback text/aria label; `Retry saved-login deletion` takes priority over `Retry saved-login restoration`. Available across screens when applicable, not while auth is pending. |
| `connection-status` | `Polling::status()` text/aria label: connected, paused, or failed channels/history/both; distinguishes unfocused pause from retrying reads. Exact strings in `polling.rs:108–133`. |
| `channels`, `channel-name` | Sidebar container; input visible/aria label `Channel name`. |
| `refresh-channels` | `Refresh channels` / `Refreshing channels…`; disabled during its read. |
| `create-channel` | `Create text channel` / `Creating channel…`; disabled while creating or channels are not ready. |
| `channel-feedback`, `channels-refresh-error` | Creation feedback and refresh error; error aria label omits the visible `Channel refresh: ` prefix. |
| `channel-{id}` | `# {name}`, bundled Hash icon; selection retained by channel identity, not list position. |
| `history-pane`, `history` | Focusable pane and variable-height list container; left click focuses pane. |
| `refresh-history` | `Refresh conversation` / `Refreshing conversation…`; disabled during catch-up or initial load. |
| `catchup-incomplete` | Error aria label; visible `Catch-up incomplete: {error}`. Existing contiguous history retained. |
| `retry-older`, `jump-latest` | `Retry older messages`; `Jump to latest` disabled unless known off-bottom. |
| `message-{id}`, `message-text-{id}`, `text-{id}` | Stable row, text wrapper, Kit SelectableText ID. Row aria label is plain message text; author/timestamp displayed separately. |
| `composer-panel`, `composer` | Composer container/click focus target; textarea aria label `Message to {name}`; read-only only for originating pending send. |
| `send-message`, `send-feedback` | Bundled Send icon; `Send message` / `Sending…`; tooltip **`Send message (Enter); Shift+Enter adds a line`**. Button disabled while sending; feedback is text/aria label. |

Also preserve explicit visible states: `Loading channels…`, `No text channels yet.`, `Loading conversation…`, `No messages in this channel yet.`, `Loading older messages…`, `Start of conversation.`, `Select a text channel to read its conversation.`, failure text, and saved-login verification feedback. These are not all separately ID'd.

Keyboard/selection baseline (`main.rs:150–178,491–512,1390–1405,1494–1566`): real textarea `submit_on_enter(true)` emits Enter without modifying text at a mid-line caret or stripping a trailing newline; Shift+Enter inserts newline. The subscription sends only when neither Shift nor secondary modifier is set. Composer click stops propagation to history focus; programmatic draft hydration uses `syncing_composer` to avoid wrong-channel writes. Per-channel drafts are authoritative in `Conversation` and disappear on session invalidation/restart. Mouse-drag plus Ctrl+C copies plain multiline SelectableText through Kit Root. Upward wheel near the first visible rows requests older pages. Stable-ID splices preserve reader position for prepend/middle insertion; initial load follows bottom, same-channel additions follow only when appropriate, and Jump explicitly resumes bottom-follow. Headless input/selection coverage is not native IME/accessibility proof.

## Transport and timing inventory

`http.rs:10–53`: remote HTTPS only; HTTP only for case-insensitive `localhost` or loopback IPs (IPv4/IPv6). Reject userinfo, query, fragment, non-root path, missing host and other schemes. Rustls TLS with normal certificate verification; no insecure override. Reusable reqwest client uses `no_proxy()`, explicit localhost resolution to `127.0.0.1`/`::1`, and `Policy::none()` redirects. Do not replace this with default proxy/redirect behavior. No credential/body logging is introduced.

Current unchanged routes under `/api/v1`: POST `auth/signup`, `auth/login`, `auth/logout`; GET `me`, `channels`, `channels/{id}/messages`; POST `channels`, `channels/{id}/messages`. Protected calls attach bearer auth. History requests use the server's default page size (no explicit `limit`) and optional opaque `before` query (`http.rs:421–454`). IDs/cursors are not synthesized from timestamps. Endpoint construction stays in the HTTP adapter; no server edit belongs in this baseline.

| Bound / schedule | Current location and observable semantics |
| --- | --- |
| **8 seconds** HTTP | `HttpAuth::new`, `http.rs:49`. Applies to actual HTTP operations, including login/signup/create/logout and `/me`; there is no extra 9-second outer wrapper for login/signup/create/logout. HTTP failures are typed; current-session 401 differs from temporary transport failure. |
| **9 seconds** channel read | `main.rs:774–787`, outer `tokio::time::timeout` in `dispatch_channels`, including controlled adapters. |
| **9 seconds** selected history page | `main.rs:869–920`, `load_history`; production Tokio timeout, headless GPUI timer/select. Initial/older/manual/focus/poll/post-send catch-up all use it. Each page gets its own bound, not one 9-second budget for the entire multi-page catch-up. Navigation cancels production task and advances serial; stale results are inert. |
| **9 seconds** send | `main.rs:520–563`; outer production Tokio timeout or headless GPUI timer/select. Timeout maps to unavailable/uncertain outcome, retains draft and never replays a write; cancellation does not prove non-delivery. |
| **10 seconds** storage workflows | `persistence.rs:21`; `bounded`, `main.rs:77–89`. Delete waits for its worker reply (`330`); save waits for **both remember and save replies together** (`389–392`). Queue submission is nonblocking `try_send` into capacity 16 (`persistence.rs:177,324–336`), with one dedicated blocking worker. Timing out the wait does not interrupt provider work or reorder deletion after a pending save. |
| **10 seconds total restoration** | `main.rs:432–481`: worker read is submitted, then an outer `bounded` covers waiting for its reply **plus** `/me` verification. `/me` has an inner 10-second adapter bound and the real HTTP 8-second bound, but the outer budget still caps their combined wait. Not 10 seconds for storage followed by a fresh 10/8 seconds. If storage consumes ~7 seconds, only ~3 remain. Known expiry can reject before read; expiry/identity/generation are checked again on completion. Unavailable/timeout is retryable, not authoritative deletion. |
| Expiry / polling | `main.rs:668–704` schedules expiry for the accepted generation; `709–723` ticks production polling every 1 second. Pure polling uses supplied monotonic time: history 3s, channels 15s, independent exponential read backoff to 60s (`polling.rs:4–6,71–89`). Pause unfocused; focus return/recovery request reconciliation without duplicating pending work. |

Storage feedback must distinguish saved, saved with old-cleanup warning, failed memory-only, and **unconfirmed** timed-out save/delete. Logout/expiry/rejection invalidate local state independently of server revocation and provider deletion; an already-invalid revocation response does not confirm a fresh revocation. FIFO, invalidation epoch, commit/rollback and durable pending-deletion protocol are described by source `persistence.rs:176–319` and covered by the mapped scenarios.

## Synthetic persistence compatibility

[compatibility.json](compatibility.json) contains only invented public identities/configuration and provider **keys**, never any credential values. It is a fixture envelope; each `configs[].value` is a complete example `session.json` object. Do not copy it to a user's configuration directory or use it with the real provider. These are source-derived worked examples; they have not been loaded via new production tests.

Compatibility facts (`persistence.rs:19–110`, `main.rs:132–146,236,432–481`):

- Config path: nonempty `XDG_CONFIG_HOME` wins, else `HOME/.config`, then `hamlet-gpui/session.json`; neither available means no path. No file at the real path was inspected.
- JSON fields: optional `server`, optional `saved` (`server`, `user: {id, username}`, `expires_at` integer epoch seconds), `pending_deletions` array of full selections, defaulting empty when omitted. No version field, password/token, drafts, or messages. Writer uses a sibling `.tmp` and rename; all writes are serialized by the worker.
- Last successful server preference can differ from the saved credential after a failed replacement. Startup prefill uses `config.server`; username/restore candidate are selected only on **exact string equality** with `saved.server`. Missing/invalid files return default config; valid loaded config requires a valid `server` and valid nonempty saved/pending public identities.
- Provider service is exactly `org.hamlet.gpui.rewrite.session.v1`. Account key is `UTF-8-byte-length(server):server:user.id`; username and expiry do not affect the key. Stored server strings are retained, not rewritten to `Url::to_string()`. Even a root trailing slash changes the key. Normalizing URL spelling would silently orphan credentials.
- Worker reads require full saved-selection equality. Stale deletion of the same key but a different newly saved selection is rejected; durable deletion intents are written before provider deletion and retained on failure/restart. Old-user cleanup must not remove a newer saved user. No memory-zeroization or exactly-once-delivery claim is made.

## Handoff

Stage 0 evidence gate is satisfied. Start #48 only under its own scope: preserve this snapshot, all unrelated work, the complete scenario inventory, and the distinctions between automated versus native evidence. Future ownership changes should replace private-field test setup with agreed feature/control interfaces, retain the real-route and controlled-worker coverage, and rerun all four commands. No migration shim or second implementation was added here.
