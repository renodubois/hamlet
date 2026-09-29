# Storage mechanics checkpoint — #53

Scope: [issue #53](https://github.com/renodubois/hamlet/issues/53), pinned start
`3d65711d6e58f46f63cc6cad734b2b5d193382d9` on `rewrite`. Live body, comments (none)
and label (`ready-for-agent`) were fetched. This implements storage separation and
saved-login compatibility only, **not #54** or session/view ownership extraction.
The approved architecture/plan and prior migration/baseline records are unchanged.

## Ownership and compatibility

- `src/storage/mod.rs` retains the storage interface, public identity/outcomes and
  **one** bounded, nonblocking, dedicated worker. The complete `Outcome`, command
  and `Persistence` implementation is byte-identical to the pinned start. Save,
  preference, read, rollback, invalidation and deletion ordering has not changed.
- `src/storage/credentials.rs` owns `Store`, Secret Service operations and account
  key construction. Service remains `org.hamlet.gpui.rewrite.session.v1`; keys
  remain UTF-8 server byte length, original server spelling, and user ID. HTTP URL
  normalization does not rewrite durable identity. Real provider construction and
  operations remain excluded from test builds.
- `src/storage/preferences.rs` owns the token-free `Config`, loading/validation,
  environment-derived path and synchronous temporary-file/rename commit. Its write
  function is visible only to storage, called only by the same worker; there is no
  preference task, second writer or new queue. Supplied environment values let
  path fixtures run without changing process environment or accessing real files.
- `Selection` now imports canonical `api::User`, not its session re-export. Storage
  has no production session/view dependency. Existing callers retain the same
  interface; the old root `persistence` import alias is still only a namespace for
  this implementation, not another store or worker. Session/storage workflow
  decisions, their deadlines and legacy shell ownership remain for later tickets.
- `src/test_support/storage.rs` is the intentional crate-private, test-only home
  for the existing shared controlled provider. API, session and view suites no
  longer import `storage::tests`. The nine existing storage tests are mechanically
  relocated to `src/storage/tests.rs`, with scenario bodies unchanged apart from
  formatting. No existing scenario was removed or replaced.

## Behavioral evidence

Seven new scenarios exercise the approved storage/session interfaces with fake
providers, isolated files and controlled bound HTTP responses:

1. Execute #47's service, account-key and configuration-path worked examples.
2. Preserve distinct selected-server/saved-server identities, including a trailing
   slash; no stored URL normalization or metadata migration.
3. Load the original configuration omitting `pending_deletions`, read a credential
   seeded under its literal old key, renew it, reject an old-selection deletion,
   then delete it. Compare the whole resulting token-free metadata object.
4. Resume both selected-login/old-user and logged-out deletion fixtures. Failed
   deletion retains intent; a restarted worker can finish it without restoring the
   pending identity or removing the current credential.
5. Restore a pre-rearchitecture config/provider fixture through `AppSession` and
   bound `/me` verification; activation requires verification, and the restored
   context can save again and be deleted under the same key.
6. Fail metadata commit using an isolated filesystem obstacle after a provider
   write: same-account and cross-account replacements preserve the previous login;
   deletion cannot touch the provider without recording durable intent.
7. Hold provider deletion, fill all 16 queue slots with preferences, and reject
   overflow immediately. Preferences remain pending until provider work finishes,
   then commit FIFO without losing deletion metadata.

The existing controlled-time real-control scenarios still prove the shared 10s
restoration budget and unconfirmed save/delete deadlines: timing out the wait is
not provider cancellation, confirmed saving or confirmed deletion. Existing stale
save cleanup, failed replacement, old-user cleanup, retry and real-route saved-login
coverage all remain. No password/token metadata, plaintext fallback, durable
history/drafts, write replay, zeroization or exactly-once claim was introduced.

The first compatibility tracer failed on the missing shared test-support import,
then passed after relocating the provider. The remaining cases characterize
existing behavior; they needed no algorithm fix. They are not claimed as new
production behavior or as real-wallet verification.

## Checks and preservation

From `client-gpui/`, using the existing lockfile/cache, 2026-09-29 UTC:

- Repeated `cargo check --locked --all-targets`: passed.
- Targeted storage/timeout suite: **18 passed**, also with `SEED=1`, `2`, `3`.
- Targeted saved-login controls: **2 passed**; restored-fixture and existing session
  persistence scenarios: passed; mechanically relocated storage suite: **9 passed**.
- Final `cargo fmt --check`: passed (17:29:02).
- Final `cargo clippy --locked --all-targets -- -D warnings`: passed (17:29:02–04).
- Final `cargo test --locked`: **121 passed, 0 failed/ignored** (17:29:04–06).
- Final `cargo build --locked`: passed (17:29:06–08); compiled, not launched.
- `git diff --check`, worker byte-comparison, relocated scenario comparison and all
  six pre-existing file SHA-256 checks: passed.

Original modified `.gitignore`, `CONTEXT.md`, `client-gpui/README.md` and untracked
`client-gpui/ARCHITECTURE.md`, `docs/plans/client-gpui-prototype.md`,
`docs/plans/client-gpui-rearchitecture.md` are byte-preserved and excluded from
staging. No server, dependency/lockfile, API production, runtime, UI/control-ID,
baseline or prior migration-document change. No real credentials/profile/keyring,
desktop automation, issue comment/closure, push, branch change, stash or reset.

## Handoff

Evidence, live issue JSON, start pin, committed diff, commit list and handoff live
in `/tmp/hamlet-orchestrator/review-53/`. Parent owns mandatory parallel Standards
and Spec review **after the scoped commit**, then any issue closure. No nested
reviewers were launched; pending parent review is not an implementation blocker.

No substantive implementation blocker. Native IME, accessibility/physical input,
precise delayed native anchoring and locked/slow real-wallet limitations in
`VERIFY.md` remain unverified. No #54 work was performed.
