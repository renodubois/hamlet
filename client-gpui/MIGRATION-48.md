# Module scaffolding checkpoint — #48

Scope: [issue #48](https://github.com/renodubois/hamlet/issues/48), ticket-start HEAD
`1fd21f426afa0d45a5d9af9c0572115d23a1a708`. This is the mechanical module/test-placement
checkpoint, **not completed view or workflow ownership extraction**. The target
[architecture](ARCHITECTURE.md) and [plan](../docs/plans/client-gpui-rearchitecture.md)
remain the authority for subsequent tickets. Those pre-existing documents and the
README are intentionally not changed/staged here.

## Current editing locations

| Responsibility | Current home |
| --- | --- |
| GPUI/Kit initialization, dependency construction, window and Kit `Root` creation | `src/main.rs` |
| Temporary `Hamlet` root: all feature workflows, rendering, inputs, list anchoring, subscriptions and task/result handling | `src/views/app_shell.rs` |
| Shared colors and bundled Hash/Send icon mapping | `src/theme.rs` |
| Existing lazy Tokio runtime and channel-returning bounded future bridge | `src/runtime.rs` |
| Existing pure authentication transitions and legacy API types/trait | `src/session/state.rs` |
| Existing pure channels/history/draft/send transitions and legacy server data | `src/conversation/state.rs` |
| Existing supplied-time focus/backoff schedule | `src/conversation/polling.rs` |
| The sole existing HTTP adapter, transport policy, decoding and route tests | `src/api/mod.rs` (mechanical move of `http.rs`) |
| The sole existing ordered credential/configuration worker and controlled provider tests | `src/storage/mod.rs` (mechanical move of `persistence.rs`) |

There is still one binary crate, no library or parallel client implementation.
`session/mod.rs`, `conversation/mod.rs` and `views/mod.rs` establish module homes;
there are deliberately no empty child-view/coordinator implementations claiming
ownership that has not migrated. `app_shell::open` contains the old root-start
sequence (resume deletions, restore, production poll timer). Dependency construction
now supplies the original config/provider to the same root initializer.

## Compatibility and legacy-owner removal inventory

| Temporary element | Purpose / removal gate |
| --- | --- |
| Crate-local `api as http` and `storage as persistence` imports in `main.rs` | Keep existing qualified imports working after file-only adapter moves. Remove each alias when its consumers switch to the canonical API/storage interface in the bound-client/storage tickets. They do not define alternate implementations. |
| `session/mod.rs` and `conversation/mod.rs` state re-exports | Preserve existing callers while pure implementations gain their target homes. Replace wildcard compatibility exports with the intentional owned interfaces when their coordinators migrate; do not retain a second state owner. |
| `Hamlet`, `Deletion`, all workflows and view fields in `views/app_shell.rs` | Explicit temporary monolith. Move authentication/expiry and saved-login decisions to session coordination (including cleanup surviving screen changes); requests/polling to conversation coordination; inputs/list/selection/rendering to owned child views. Remove `Hamlet` only when the shell composes those owners and old-session completions remain inert. |
| `AppSession` still stores editable form copies, API handle and public transition fields in `session/state.rs` | Remove form/password duplication and raw API coupling at session/login extraction, retaining validation, generation and restore semantics through the owned session interface. This move alone does not certify target state purity/dependency direction. |
| `AuthApi`, `AuthError`, `Login`, `User` still in session state; `Channel`, `Message`, `Page` still in conversation state; `HttpAuth` still uses them | Resolve in the bound-client ticket: canonical API types/errors and bound server/authenticated contexts, unchanged transport/routes, no per-operation URL/token plumbing. Existing trait defaults and secret-bearing descriptors are not redesigned here. |
| Conversation transitions still accept `AppSession`, request descriptors still carry server/token/generation | Remove coupling during bound-client/conversation coordination work while preserving stale-result/rejection and history/send algorithms. |
| `api/mod.rs` and `storage/mod.rs` are unsplit legacy implementations | Split API endpoint/wire/client responsibilities and storage credential/preferences mechanics in their owning tickets. Preserve one HTTP transport implementation and one serialized worker protocol, including durable deletion intents and rollback. |
| `runtime::runtime` lazy singleton, root-local dispatch/expiry/poll timers, separate `cfg(test)` send/history scheduling | Runtime relocation is not executor unification. Replace legacy scheduling at the approved controlled execution/time seam when coordinators own task lifetimes; production and tests must then use the same workflow path. Preserve 8s HTTP, 9s selected reads/send, and 10s storage/combined restoration semantics, not one universal timeout. |
| `views/tests/legacy_fixture.rs`, descendant private-field assertions and test-only `Hamlet::new` | Keep existing suite setup without widening production fields/methods for tests. The constructor supplies the same default config/no provider as before; only `open` is the production startup surface. Replace fixture/private access as owned interfaces and child views land, using controlled requests/time and real Kit controls/stable IDs. |
| Shared controlled storage provider remains in `storage::tests` (also reached via `persistence::tests`) | Move to intentional crate-private test support when storage ownership is split; retain isolated files and worker-ordering coverage. Never substitute real Secret Service in automated tests. |
| Original `HistoryProbe` feasibility tests in `views/tests/history.rs`; fixture-only Tokio bridge in `journeys.rs` | Retain their existing evidence/limitations, not new ownership seams. Remove only after equivalent production history/control and real HTTP journey coverage is documented at the migrated owners. |

`runtime::bounded` now receives its deadline from the caller rather than importing
storage policy. Every existing caller still passes `persistence::DEADLINE` (10s),
including the **single outer restoration budget covering storage reply plus `/me`**.
No scheduling algorithm, timeout value, cancellation policy, HTTP route, provider
identity, persisted format, dependency, semantic ID, label or layout is changed.

## Scenario relocation

The immutable [#47 scenario inventory](baseline-47/SCENARIOS.md), compatibility
fixtures and baseline checks remain untouched. Existing tests are relocated, not
rewritten or replaced; no new test seam is introduced in this mechanical ticket.
Pure/adapter suites stay beside their moved implementations:

| Previous namespace | Current namespace | Scenarios |
| --- | --- | ---: |
| `session::tests` | `session::state::tests` | 8 |
| `conversation::tests` | `conversation::state::tests` | 27 |
| `polling::tests` | `conversation::polling::tests` | 4 |
| `http::tests` | `api::tests` | 19 |
| `persistence::tests` | `storage::tests` | 9 |

The 30 former `tests::<name>` GPUI scenarios live under
`views::app_shell::tests::<group>::<name>`. Their names and assertions are retained:

| File under `src/views/tests/` | Scenarios | Coverage |
| --- | ---: | --- |
| `authentication.rs` | 5 | Validation, pending login/signup, recoverable signup errors and keyboard focus |
| `channels.rs` | 5 | Creation, late creation after logout, refresh selection, disappearing-channel fallback |
| `history.rs` | 6 | Existing feasibility probes, real wheel/pagination, middle insertion, refresh/jump, plain selection/copy |
| `composer.rs` | 4 | Real textarea/Enter/Shift+Enter, per-channel drafts, uncertainty and stalled-send timeout |
| `polling.rs` | 3 | Multi-page catch-up/navigation, focus/read/logout race, outage/recovery with retained draft |
| `saved_login.rs` | 2 | Signup/save/verified restore/logout, failed deletion warning and retry via controlled worker |
| `journeys.rs` | 5 | Login/logout, selected-conversation navigation, expiry/composer cleanup, send/poll confirmation, Bob activity through real rewrite routes |

`app_shell.rs` attaches the fixture with `#[path = "tests/legacy_fixture.rs"]` so
all suites remain descendants of the legacy root. That physical placement is
intentional: no production internals are exported just to preserve private tests.
Group-specific fixtures stay with their suite; shared `TestAuth`, `SendAuth` and
`PagedAuth` remain in the explicit legacy fixture.

## Verification and preservation

All commands below run from `client-gpui/`, using the existing lockfile/cache:

- During relocation: `cargo fmt`, `cargo check --locked --all-targets`; targeted
  `cargo test --locked views::app_shell::tests::authentication` (5), `...::history`
  (6), and `...::saved_login` (2).
- Final gate on 2026-09-29 (UTC): `cargo fmt --check` passed (15:34:32–33),
  `cargo clippy --locked --all-targets -- -D warnings` passed (15:34:33–34),
  `cargo test --locked` passed (15:34:34–36; 97 passed, 0 failed/ignored), and
  `cargo build --locked` passed (15:34:36–39).
- The full run's scenario names match all 97 baseline names one-for-one after
  namespace relocation. Session state, conversation state, pure polling and
  storage files are byte-identical moves; the HTTP file only changes its polling
  test import. No baseline/dependency/server files changed.

The first extraction check caught an extra closing test-module delimiter; it was
removed before the successful typecheck/targeted runs. No behavior fix was needed.
Unrelated starting edits in `.gitignore`, `CONTEXT.md`, `client-gpui/README.md`,
`client-gpui/ARCHITECTURE.md`, `docs/plans/client-gpui-prototype.md` and
`docs/plans/client-gpui-rearchitecture.md` are excluded from staging. Starting
status, SHA-256 fingerprints and command logs were captured outside the repo in
`/tmp/hamlet-48-evidence/`; hashes are rechecked before handoff.

No desktop automation, real profile/provider/credentials, dependency upgrade or
server source change. The existing controlled tests use isolated files and
loopback HTTP fixtures. Native IME, accessibility/physical keyboard, delayed
native pixel anchoring and locked/slow real-wallet limitations in [VERIFY.md](VERIFY.md)
remain unverified; historical native evidence is not claimed as a fresh run.
