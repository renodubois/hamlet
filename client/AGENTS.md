# GPUI client instructions

Follow [ARCHITECTURE.md](ARCHITECTURE.md) for ownership and dependency rules, including its required [test layout](ARCHITECTURE.md#test-layout-required).

## Tests

- Put every in-crate test suite in the owning feature's `src/<feature>/tests/` directory. Name files by subject, without `_tests`; do not add inline suites or sibling test files beside production code.
- Declare suites from their owning production module behind `#[cfg(test)]`, using `#[path = "tests/<subject>.rs"]` where needed. Preserve module ownership/private access; do not widen visibility to move tests. Existing module names/Cargo test filters may remain unchanged.
- Keep single-suite helpers local, feature-shared helpers in `tests/support/`, and cross-feature fixtures in crate-private, test-only `src/test_support/`. Narrow owner-local test-only re-exports/injection hooks are allowed; duplicate test-only workflows are not.
- Top-level `tests/` is only for external integration-test crates against a library's public API. In-crate real-route scenarios stay with their feature.
- Check these rules when adding or reviewing tests. Preserve scenario coverage during reorganizations.

## Verification

From `client/`, run `cargo fmt --check`, `cargo clippy --locked --all-targets -- -D warnings`, `cargo test --locked`, and `cargo build --locked`.

Automated checks do not establish native acceptance. Do not launch desktop automation or access a real keyring without separate consent; follow the [native safety gate](VERIFY.md#native-safety-gate).
