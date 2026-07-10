#!/usr/bin/env bash
# Local CI: run the same checks GitHub Actions would, without leaving your machine.

set -eo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<'USAGE'
Local CI: run the same checks GitHub Actions would, without leaving your machine.

Usage:
  scripts/check.sh                         # everything (server + Electron client)
  scripts/check.sh server                  # just the Rust backend
  scripts/check.sh client                  # just the Electron/Solid client
  scripts/check.sh client electron         # explicit Electron client selector
  scripts/check.sh all --client electron   # server + Electron/Solid client
  scripts/check.sh --fix                   # apply fmt fixes before running checks
  scripts/check.sh --e2e                   # also run Playwright E2E tests

Client checks:
  electron -> client/: pnpm run fmt:check (or fmt with --fix), lint,
              typecheck, test

Optional checks:
  --e2e -> run Playwright E2E for client/ when client checks are selected

Each step prints `===> <step>` before running and a one-line summary at the end.
The script aborts on the first failure with a non-zero exit code, so it's safe
to wire into a pre-push git hook.
USAGE
}

die() {
  printf '%s\n' "$1" >&2
  printf 'Run scripts/check.sh --help for usage.\n' >&2
  exit 2
}

target="all"
target_seen=0
client_seen=0
fix=0
run_e2e=0

set_client_target() {
  local value="$1"
  if (( client_seen )); then
    die "multiple client selectors provided"
  fi
  case "$value" in
    electron)
      client_seen=1
      ;;
    *) die "invalid client selector: $value (expected electron)" ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    server|client|all)
      if (( target_seen )); then
        die "multiple targets provided"
      fi
      target="$1"
      target_seen=1
      ;;
    electron)
      set_client_target "$1"
      ;;
    --client)
      shift
      [[ $# -gt 0 ]] || die "--client requires a value: electron"
      set_client_target "$1"
      ;;
    --client=*)
      set_client_target "${1#--client=}"
      ;;
    --fix) fix=1 ;;
    --e2e) run_e2e=1 ;;
    -h|--help)
      usage
      exit 0
      ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done

if (( client_seen )); then
  if [[ "$target" == "server" ]]; then
    die "client selector cannot be used with server-only target"
  fi
  if (( ! target_seen )); then
    target="client"
  fi
fi

if [[ -t 1 ]]; then
  bold=$'\033[1m'; dim=$'\033[2m'; reset=$'\033[0m'
  green=$'\033[32m'; red=$'\033[31m'
else
  bold=""; dim=""; reset=""; green=""; red=""
fi

step() { printf '%s===> %s%s\n' "$bold" "$1" "$reset"; }
ok()   { printf '%s    ok%s\n' "$green" "$reset"; }
fail() { printf '%s    FAILED%s\n' "$red" "$reset"; }
skip() { printf '%s    skipped: %s%s\n' "$dim" "$1" "$reset"; }

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
    skip 'cargo-audit not installed (`cargo install cargo-audit`)'
  fi
}

client_checks() {
  if [[ ! -d "$REPO_ROOT/client" ]]; then
    die "client/ does not exist"
  fi

  cd "$REPO_ROOT/client"
  if (( fix )); then
    run "client: pnpm run fmt" pnpm run fmt
  else
    run "client: pnpm run fmt:check" pnpm run fmt:check
  fi
  run "client: pnpm run lint" pnpm run lint
  run "client: pnpm run typecheck" pnpm run typecheck
  run "client: pnpm run test" pnpm run test
}

client_e2e_checks() {
  if [[ ! -d "$REPO_ROOT/client" ]]; then
    die "client/ does not exist"
  fi

  cd "$REPO_ROOT/client"
  run "client: pnpm run test:e2e" pnpm run test:e2e
}

case "$target" in
  server) server_checks ;;
  client) client_checks ;;
  all)    server_checks; client_checks ;;
esac

if (( run_e2e )); then
  case "$target" in
    server) skip "client E2E not run for server-only target" ;;
    client|all) client_e2e_checks ;;
  esac
fi

printf '\n%sAll checks passed.%s\n' "$green$bold" "$reset"
