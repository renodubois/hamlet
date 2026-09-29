# Bound API expansion checkpoint — #50

Scope: [issue #50](https://github.com/renodubois/hamlet/issues/50), starting HEAD
`198008cdc3ddb0f993011d2f3cdea052bb5ba776` on `rewrite`. The issue has no comments
and carries `ready-for-agent`. This checkpoint expands the API behind existing
callers; it does **not** migrate session/conversation coordination or implement
#51. It supersedes the API ownership entries in [MIGRATION-48.md](MIGRATION-48.md).
[Execution ownership and deadlines](MIGRATION-49.md) remain unchanged.

## Canonical ownership and interface

- `src/api/client.rs`: `HttpTransport` owns the reusable reqwest pool/policy;
  `transport.server(origin)` validates an origin once and returns `ServerClient`.
  `AuthenticatedClient` shares an immutable server/credential context through
  `Arc`. Clones retain that context; new authentication never mutates old handles.
- `src/api/auth.rs`: login/signup return `Authentication { client, user,
  expires_at }`. Current-user verification and logout/revocation take no URL or
  credential arguments. `restore_candidate(token)` binds a private candidate;
  the session owner must verify identity/expiry/generation before publishing it.
  Binding alone does not establish a verified or locally active session.
- `src/api/channels.rs` and `messages.rs`: listing, creation, newest history,
  opaque-cursor history pages and sending accept operation arguments only.
- `src/api/types.rs` and `error.rs`: canonical `User`, `Channel`, `Message`, `Page`
  and `ApiError`. API production code has no session/conversation dependency.
  `wire.rs` contains private wire DTOs. The generic error fallback is now
  operation-neutral (in particular, conflict no longer means username conflict).
  Existing feature-specific authentication/write uncertainty wording remains.
- A private request-adapter seam receives **already bound** reqwest requests.
  The real adapter executes them once. Controlled adapters supply responses
  through the same public bound operations, URL/header/body construction and
  decoding, not through a second endpoint implementation. Test support exports
  the adapter only under `cfg(test)`; application callers do not see transport
  details. Missing controlled responses return a typed failure.

Remote HTTPS/loopback-only HTTP validation, normal certificate verification,
`no_proxy`, explicit IPv4/IPv6 localhost resolution, no redirects, eight-second
HTTP deadlines and all rewrite routes are retained. Response bodies remain lazy:
status-only outcomes do not wait for a body, while JSON reads retain reqwest's
request/body deadline. Error decoding distinctions and single-attempt uncertain
writes are unchanged. The 9s read/send and combined 10s restoration/storage
workflow bounds remain with their existing owner. No stored server string or
credential key is normalized/reformatted by this change.

## Temporary compatibility and retirement

`src/api/legacy.rs` holds `AuthApi`, `HttpAuth` and `Login`. The production
`HttpAuth` is **only a forwarding facade**: it binds a server/candidate and calls
the bound operation. It has no HTTP dispatch, endpoint paths or decoding.
Existing legacy fixture implementations remain with the unmigrated workflows;
new API tests use bound clients. The old send default now returns `Unavailable`
rather than panicking when a fixture omits an implementation.

Retire this facade **after both authentication and protected caller migrations**:

1. Session coordination retains an authenticated client, uses private verified
   restoration candidates and captures the old client for revocation. Replace
   the legacy `Login` credential handoff with narrowly scoped session/storage
   coordination, not an accessor for views or conversation.
2. Conversation coordination consumes that client and removes URL/token fields
   from request descriptors while preserving generation/channel/request IDs.
3. Remove `AuthApi`/`HttpAuth`/`Login`, their fixtures/import aliases, and API data
   re-exports from session/conversation once their last consumers migrate.

The bound interface exports **no credential accessor or public conversion**.
Only the private facade conversion produces the legacy `Login` consumed by the
current session/storage workflow in the temporary shell. Existing raw-token
plumbing remains solely for those unmigrated callers; it is not a new interface
for future consumers. `AuthenticatedClient` and legacy `Login` have redacted
Debug; secret-bearing legacy read/create/send descriptors now have no Debug.
No credential/body logging, memory-zeroization or exactly-once claim was added.

## Verification

All **104** starting scenarios remain. The 19 original API tests were relocated
to `src/api/tests.rs`; three shortened-timeout fixtures now inject their client
through the forwarding facade. Existing real rewrite-route, redirect, deadline,
malformed-response, write-uncertainty and stored-login tests remain operational.
Four new scenarios in `src/api/binding_tests.rs` cover:

- Login's public identity/expiry and bound origin, with no bearer on authentication.
- Controlled two-origin/two-credential isolation across clones, newer login/signup
  and revocation; old handles keep their original credential and redacted Debug.
- All bound operations over two real loopback origins, with exact methods,
  origins, bearer headers, login/create/send bodies and opaque cursor encoding.
- Controlled adapters cannot bypass origin/channel validation; stored candidates
  use their bound bearer and omitted outcomes return failures without replay.

TDD evidence: the login tracer failed on the missing `HttpTransport` interface,
then passed; the controlled isolation tracer failed on the missing adapter seam,
then passed. The subsequent transport/validation characterization tests passed
against the relocated implementation without a production behavior fix. New
assertions are at the approved bound-client seam, not private client/view fields.

From `client-gpui/`, with the existing lockfile/cache:

- `cargo fmt` during implementation; three `cargo check --locked --all-targets`
  runs passed (an initial unused compatibility import warning was removed).
- Targeted API runs: 20, then 2 bound-only, then **23 API tests passed**.
- `cargo test --locked views::app_shell::tests::execution`: **7 passed**.
- `cargo test --locked views::app_shell::tests::saved_login`: **2 passed**.
- Final `cargo fmt --check`: passed.
- Final `cargo clippy --locked --all-targets -- -D warnings`: passed.
- Final `cargo test --locked`: **108 passed, 0 failed/ignored**.
- Final `cargo build --locked`: passed.
- `git diff --check` and all six pre-existing file SHA-256 checks: passed.

Full logs, fetched issue body/comments/labels, pinned start and preservation
fingerprints are in `/tmp/hamlet-orchestrator/review-50/`. The parent owns mandatory
parallel Standards/Spec review after this scoped commit; review is pending,
not an implementation blocker.

## Preservation and limits

The original modified `.gitignore`, `CONTEXT.md`, `client-gpui/README.md` and
untracked `client-gpui/ARCHITECTURE.md`, `docs/plans/client-gpui-prototype.md`,
`docs/plans/client-gpui-rearchitecture.md` are byte-preserved and excluded from
staging. Baseline evidence, server/dependency files, storage/configuration format,
UI controls/layout, workflow scheduling and runtime code are untouched.

No desktop automation, real credentials/profile/keyring, issue mutation, push or
branch change. Tests use synthetic data, isolated storage and loopback servers.
Native IME/accessibility/physical-input, delayed native anchoring and locked/slow
real-wallet limitations in [VERIFY.md](VERIFY.md) remain unverified. No substantive
implementation blocker was encountered; do not close #50 or advance to #51 here.
