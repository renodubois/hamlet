#!/usr/bin/env bash
# Local CI: run the same checks GitHub Actions would, without leaving your machine.
#
# Usage:
#   scripts/check.sh              # everything (server + client)
#   scripts/check.sh server       # just the Rust backend
#   scripts/check.sh client       # just the SolidJS frontend
#   scripts/check.sh --fix        # apply fmt fixes before running checks
#   scripts/check.sh server --fix # combinable
#
# Each step prints `===> <step>` before running and a one-line summary at the
# end. The script aborts on the first failure with a non-zero exit code, so
# it's safe to wire into a pre-push git hook.

set -eo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

target="all"
fix=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    server|client|all) target="$1" ;;
    --fix) fix=1 ;;
    -h|--help)
      sed -n '2,15p' "$0"
      exit 0
      ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

if [[ -t 1 ]]; then
  bold=$'\033[1m'; dim=$'\033[2m'; reset=$'\033[0m'
  green=$'\033[32m'; red=$'\033[31m'
else
  bold=""; dim=""; reset=""; green=""; red=""
fi

step() { printf '%s===> %s%s\n' "$bold" "$1" "$reset"; }
ok()   { printf '%s    ok%s\n' "$green" "$reset"; }
fail() { printf '%s    FAILED%s\n' "$red" "$reset"; }

run() {
  local label="$1"; shift
  step "$label"
  if "$@"; then ok; else fail; exit 1; fi
}

server_checks() {
  cd "$REPO_ROOT/server"
  if (( fix )); then
    run "server: cargo fmt" cargo fmt
  else
    run "server: cargo fmt --check" cargo fmt -- --check
  fi
  run "server: cargo clippy --all-targets -- -D warnings" \
    cargo clippy --all-targets -- -D warnings
  run "server: cargo test" cargo test
  if command -v cargo-audit >/dev/null 2>&1; then
    run "server: cargo audit" cargo audit
  else
    printf '%s    skipped: cargo-audit not installed (`cargo install cargo-audit`)%s\n' \
      "$dim" "$reset"
  fi
}

client_checks() {
  cd "$REPO_ROOT/client"
  if (( fix )); then
    run "client: npm run fmt" npm run fmt
  else
    run "client: npm run fmt:check" npm run fmt:check
  fi
  run "client: npm run lint" npm run lint
  run "client: npm run typecheck" npm run typecheck
  run "client: npm run test" npm run test
}

case "$target" in
  server) server_checks ;;
  client) client_checks ;;
  all)    server_checks; client_checks ;;
esac

printf '\n%sAll checks passed.%s\n' "$green$bold" "$reset"
