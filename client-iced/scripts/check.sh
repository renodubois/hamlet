#!/usr/bin/env bash
# Run the native Iced client checks in the same order maintainers should use
# before handing off an alpha build.

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

if [[ "${1:-}" == "--fix" ]]; then
  cargo fmt
else
  cargo fmt -- --check
fi

cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test
