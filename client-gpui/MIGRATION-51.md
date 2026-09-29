# Bound authentication checkpoint — #51

Scope: [issue #51](https://github.com/renodubois/hamlet/issues/51), pinned start
`b0bd214e0115e912087fa42bcef78a6b7c2c980f` on `rewrite`. Fetched body, labels
(`ready-for-agent`) and comments (none) are recorded in the handoff artifacts.
This implements authentication/restoration/revocation binding only, **not #52**
or the later session/view/storage ownership extractions. The pre-existing target
architecture and approved plan remain unchanged.

## Authority and behavior

- `Session` retains the accepted `AuthenticatedClient`, user, expiry and original
  server spelling. It no longer owns a separate token. Login/signup callers bind
  a `ServerClient` and activate its returned `Authentication`; a subsequent login
  creates a different immutable context. Validation, pending-submit protection,
  signup uncertainty, recoverable inputs and password cleanup remain unchanged.
- `AppSession::verify_saved` privately binds the stored credential and verifies
  `/me`. `finish_restore` requires expected user ID **and username**, current
  generation/server, compatible client origin, and unexpired saved expiry before
  activation. Timeout/unavailable remains retryable; authoritative rejection,
  identity mismatch, missing/empty credential and expiry request cleanup.
- Startup prefill and credential account keys still use the original stored
  server string, not the API's normalized URL. `Session::save` is the restricted
  session/storage operation: it passes credential material directly to the same
  ordered worker. The new crate-private API `credential_for_session` accessor
  has only session production callers; views do not use it. Metadata stays
  password/token-free, with unchanged file/provider formats and ordering.
- Logout captures the accepted old client before removing active access. Local
  invalidation, storage deletion and bound revocation remain independent. An
  already-invalid revocation remains distinct from a newly confirmed revocation;
  old-generation results cannot change newer authentication or feedback.
- HTTP transport policy, eight-second HTTP bounds, nine-second protected workflow
  bounds and the **single ten-second restoration budget covering storage + /me**
  are unchanged. No new universal timeout, replay or implicit reauthentication.

## Temporary compatibility: retire in #52 or the owning later ticket

`AuthApi::server` is a temporary factory on the existing facade, sharing the same
`HttpTransport` used by its protected forwarding methods. It stores no credential
or competing active session. Production has no legacy login/signup/current-user/
logout methods or `Login` result: those now exist only in test compatibility.

Protected channel/history/create/send descriptors and dispatch remain unchanged.
Their existing `Session::token()` reads the **accepted bound client's** credential;
there is no parallel mutable token owner. Their existing server/generation/request
checks remain intact. #52 must migrate these consumers to `active_client`, remove
raw descriptor plumbing and retire `AuthApi`/`HttpAuth` and their aliases. Keep the
restricted session-persistence access, not a view/conversation credential getter.

Authentication-focused fixtures now implement the bound request adapter, including
real-control pending/validation/signup errors, delayed login, expiry, saved-login
verification and revocation. `api/legacy_fixture.rs` is a **test-only** adapter for
still-unmigrated protected-operation fixtures: it converts their old auth outcomes
into controlled wire responses after normal bound request construction. It does
not implement a production workflow or network stack. Remove it with those fixtures
in #52. Mixed protected real-route fixtures still have test-only facade helpers;
auth-only route/security tests and the save/verified-restore journey use bound calls.
Pure legacy conversation fixtures use a test-only `Login::bind` conversion; their
algorithms and assertions are unchanged.

The shell still coordinates tasks/storage lifecycle and owns form fields; this is
not a claim that the later session coordinator or login view extraction is complete.
Existing private-field-heavy tests remain for their owning tickets. New tests use
session/storage interfaces or real Kit controls/stable IDs through `Root`.

## Verification

All 108 starting scenarios remain, with five additional scenarios:

1. A private verification candidate exposes no active client; completion after
   logout cannot activate it.
2. Old handles/revocation keep their original origin and credential after newer
   authentication; late rejection/revocation cannot invalidate the new context.
3. Unavailable restoration retries remain distinct from rejection, ID/username
   mismatch, expiry during verification and missing-credential cleanup.
4. Saving through the session seam preserves uppercase/trailing-slash stored
   identity, token-free metadata and the existing provider key; a fresh session
   restores through bound verification using the isolated saved credential.
5. Real controls show no workspace for a candidate; changed-server login can
   complete while old deletion/verification are pending, and late old verification
   cannot replace the newer user/workspace.

TDD: the first session candidate tracer failed on the missing bound session seam,
then passed after implementation. Further characterization/lifetime tests passed
through the same implementation. Targeted authentication (5), execution (initially
7, finally 8), saved-login (2), API (19) and session tests passed; typechecking ran
four times. An initial unused test import and a preflight Clippy type-complexity
warning were corrected, not waived.

Final checks from `client-gpui/` on 2026-09-29 UTC, existing lockfile/cache:

- `cargo fmt --check`: passed.
- `cargo clippy --locked --all-targets -- -D warnings`: passed.
- `cargo test --locked`: **113 passed, 0 failed/ignored**.
- `cargo build --locked`: passed (compiled only; no desktop launch).
- `git diff --check` and all six pre-existing SHA-256 checks: passed.

Logs, issue JSON, pinned start, preservation fingerprints, commit list, three-dot
review diff and handoff: `/tmp/hamlet-orchestrator/review-51/`.
Mandatory parallel Standards/Spec review is **pending parent**, after this scoped
commit; unavailable nested reviewers are not an implementation blocker.

## Preservation and limits

Byte-preserved and excluded from staging: original modified `.gitignore`,
`CONTEXT.md`, `client-gpui/README.md`, and untracked `client-gpui/ARCHITECTURE.md`,
`docs/plans/client-gpui-prototype.md`, `docs/plans/client-gpui-rearchitecture.md`.
No server, dependency/lockfile, runtime, storage-mechanics, baseline, UI layout or
semantic-control-ID changes. No issue comment/closure, push, branch switch,
stash/reset, real credentials/keyring or desktop automation. Tests used synthetic
credentials, controlled providers, isolated files and loopback HTTP only.

No substantive implementation blocker. Native IME/accessibility/physical input,
precise delayed native anchoring and locked/slow real-wallet limitations in
`VERIFY.md` remain unverified. Do not close #51 or start #52 from this checkpoint.
