# Epic #71 execution ledger

- Parent: [Live updates: implementation and verification tracker](https://github.com/renodubois/hamlet/issues/71).
- Branch: `live-updates`.
- Immutable epic baseline: `f15a69306c9122f318b60ee79940fa96b4b86951`.
- Working tree at baseline: clean. User committed the previously untracked skill and plan before authorizing discovery; neither is implementation work for this run.
- Plan status: user approved the full eight-child plan, test seams, local commits/reviews, and tracker closure policy. No implementation yet.
- Discovery: recursively paginated native sub-issues and dependencies; full bodies, paginated comments, states/reasons, labels and assignees retrieved. Eight open, unassigned, `ready-for-agent` leaves. No nested containers, external blockers, cycles, or disagreement with the parent's explicit child list/textual dependencies.
- Scope: all eight leaves; parent is a container with integrated acceptance auditing, not an additional implementation task. Issue bodies remain authoritative; existing design context: `llm-docs/live-updates-plan.md`.

## Execution frontier

Sequential order follows native hierarchy among runnable issues: #63, #64, #65, #66, #67, #68, #69, #70. Each blocker must be verified on this branch and closed as completed before dependent implementation starts.

| Issue | Blockers | Status | Acceptance coverage / evidence | Tests / results | Review | Commits |
| --- | --- | --- | --- | --- | --- | --- |
| [#63 Shared wire types](https://github.com/renodubois/hamlet/issues/63) | None | Completed; tracker verified CLOSED/COMPLETED | Exact entity/event JSON, 4000-character Unicode/newline body, timestamp/ID/additive fields, unchanged HTTP/OpenAPI artifact, API validation | Protocol 4/5 tests (without/with OpenAPI); server 8 tests; desktop 159 tests; all fmt/clippy/build checks pass | Standards: 0; Spec: 0 actionable findings | `a411f6d` |
| [#64 Race-safe conversation state](https://github.com/renodubois/hamlet/issues/64) | None | Planned | Entity merging, replacing/older read races, bounded staging, recovery preservation | Not run | Pending | — |
| [#65 Authenticated bounded SSE](https://github.com/renodubois/hamlet/issues/65) | #63 | Planned | Hub, framing/readiness, authentication lifecycle, bounds, contract | Not run | Pending | — |
| [#66 Channel publication](https://github.com/renodubois/hamlet/issues/66) | #65 | Planned | Matching channel payloads, write failures, cancellation, safe publication | Not run | Pending | — |
| [#67 Message publication](https://github.com/renodubois/hamlet/issues/67) | #65 | Planned | Matching message payloads, pre-write author preparation, failures/cancellation | Not run | Pending | — |
| [#68 Desktop API stream](https://github.com/renodubois/hamlet/issues/68) | #66, #67 | Planned | Verified server gate; bound transport, incremental parsing/validation, deadlines, bounded delivery | Not run | Pending | — |
| [#69 Desktop live synchronization](https://github.com/renodubois/hamlet/issues/69) | #64, #68 | Planned | One session stream/recovery lifecycle, races, local work preservation, stale UI, polling removal | Not run | Pending | — |
| [#70 Integrated verification](https://github.com/renodubois/hamlet/issues/70) | #69 | Planned | Two-client recovery and races, compatibility, measured bounded fanout, full checks | Not run | Pending | — |
| [#71 Parent acceptance](https://github.com/renodubois/hamlet/issues/71) | All children | Planned | Integrated no-polling updates, authoritative recovery, visible failure and measured evidence | Not run | Original-baseline review pending | — |

## Approved test seams

1. Protocol public Serde/OpenAPI boundary (`protocol/tests/`), independent exact JSON fixtures and existing HTTP contract/validation checks; feature on/off checks.
2. Conversation owner-local transition/coordinator boundaries (`client/src/conversation/tests/`): entity/read/write identities, ordering, bounded staging/reset; preserve polling behavior until #69.
3. Server public HTTP routes and concrete hub subscription lifecycle: disposable databases, real loopback for transport/disconnect/cross-worker behavior, finite frame reads, controlled clocks and deterministic database barriers. Mutation results observed through HTTP and subscriptions, not private implementation assertions.
4. Authenticated desktop API boundary (`client/src/api/tests/`): production binding/decoding with controlled transport/time and real routes; finite deadline/frame/queue behavior, cleanup.
5. Conversation lifecycle and headless semantic view boundaries (`client/src/conversation/tests/`, `client/src/views/tests/`): injected API/time, real-route two-client scenarios, selection/session/write races, notices and history reset. No native desktop automation or real keyring.
6. Integrated real-route two-client harness and bounded fanout measurement in #70, recording client count/duration/payload sizes/latency/pressure/slow consumers without production capacity claims.

## Verification and delivery policy

- Test-first vertical slices at approved seams; focused tests and type/static checks during implementation.
- Each child gets its own implementation commit before parallel read-only Standards and Spec reviews; fixes are committed, checked and reviewed before closure.
- Protocol: format, lint/test with and without `openapi`; lockfile updates deliberate, no root workspace.
- Server: `cargo fmt --check`; migration formatting; `cargo clippy --locked --all-targets -- -D warnings`; `cargo test --locked`; contract artifact/inventory verification. Full suite on route/DTO changes.
- Client: `cargo fmt --check`; `cargo clippy --locked --all-targets -- -D warnings`; `cargo test --locked`; `cargo build --locked`.
- Before #68: complete server publication/cancellation contract and full server checks; existing desktop still builds.
- Final full affected-component checks and integrated parallel Standards/Spec review from original baseline required before parent closure.
- Verified issues close as implemented and verified on this branch, even while commits are local/unpushed. Tracker comments explicitly distinguish this from merge/deployment.
- No push, PR, merge, branch change, native automation, real keyring access, legacy-client edits, or human-owned documentation edits.
- Scope conflicts/cancellation machinery beyond the issue constraints require renewed approval. Failed/unavailable required checks block completion; native consent-dependent checks remain explicit manual limitations, not claimed passes.

## Resume checkpoint

#63 closed with evidence and parent progress comment; next frontier: #64. Parallel read-only Standards and Spec reviewers read the complete committed diff and passed. Initial reviewer attempts lacked command tools; supplied complete diff artifacts for the successful second reviews. All commits remain local/unpushed.

### #63 verification

Red tests first failed on absent message/event types, then absent channel variant/types; each slice passed after implementation. Added compatibility/validation characterization coverage. An initial OpenAPI test over-specified an explicit discriminator (not required by the contract); corrected it to assert both tagged `oneOf` payloads. No artifact regeneration was needed: existing server contract tests prove generated OpenAPI stayed unchanged.

All commands below passed from the repository root:

- `cargo check --manifest-path server/Cargo.toml` and `cargo check --manifest-path client/Cargo.toml` (deliberate lockfile updates add only the shared path package).
- `cargo fmt --manifest-path protocol/Cargo.toml --check`
- `cargo clippy --manifest-path protocol/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path protocol/Cargo.toml --locked` (4 tests)
- `cargo clippy --manifest-path protocol/Cargo.toml --locked --all-targets --features openapi -- -D warnings`
- `cargo test --manifest-path protocol/Cargo.toml --locked --features openapi` (5 tests)
- `cargo fmt --manifest-path server/Cargo.toml --check`
- `cargo fmt --manifest-path server/migration/Cargo.toml --check`
- `cargo clippy --manifest-path server/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path server/Cargo.toml --locked` (8 tests including contract artifact/inventory)
- `cargo fmt --manifest-path client/Cargo.toml --check`
- `cargo clippy --manifest-path client/Cargo.toml --locked --all-targets -- -D warnings`
- `cargo test --manifest-path client/Cargo.toml --locked` (159 tests)
- `cargo build --manifest-path client/Cargo.toml --locked`

Temporary diagnostic logs: `/tmp/hamlet-epic-71/63-*-checks.log` (not needed to resume; command results recorded above). No native automation/keyring access.
