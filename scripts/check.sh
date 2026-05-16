#!/usr/bin/env bash
# Local CI: run the same checks GitHub Actions would, without leaving your machine.

set -eo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<'USAGE'
Local CI: run the same checks GitHub Actions would, without leaving your machine.

Usage:
  scripts/check.sh                    # everything (server + both clients)
  scripts/check.sh server             # just the Rust backend
  scripts/check.sh client             # both clients
  scripts/check.sh client tauri       # just the Tauri/Solid client
  scripts/check.sh client iced        # just the native Iced client
  scripts/check.sh all --client iced  # server + native Iced client
  scripts/check.sh --fix              # apply fmt fixes before running checks
  scripts/check.sh client iced --fix  # combinable

Client checks:
  tauri -> client/: npm run fmt:check (or fmt with --fix), lint, typecheck, test
  iced  -> client-iced/: cargo fmt -- --check (or fmt with --fix),
           cargo check --all-targets, cargo clippy --all-targets -- -D warnings,
           cargo test

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
client_target="all"
client_seen=0
fix=0

set_client_target() {
  local value="$1"
  if (( client_seen )); then
    die "multiple client selectors provided"
  fi
  case "$value" in
    tauri|iced)
      client_target="$value"
      client_seen=1
      ;;
    *) die "invalid client selector: $value (expected tauri or iced)" ;;
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
    tauri|iced)
      set_client_target "$1"
      ;;
    --client)
      shift
      [[ $# -gt 0 ]] || die "--client requires a value: tauri or iced"
      set_client_target "$1"
      ;;
    --client=*)
      set_client_target "${1#--client=}"
      ;;
    --fix) fix=1 ;;
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

client_tauri_checks() {
  cd "$REPO_ROOT/client"
  if (( fix )); then
    run "client-tauri: npm run fmt" npm run fmt
  else
    run "client-tauri: npm run fmt:check" npm run fmt:check
  fi
  run "client-tauri: npm run lint" npm run lint
  run "client-tauri: npm run typecheck" npm run typecheck
  run "client-tauri: npm run test" npm run test
}

client_iced_checks() {
  cd "$REPO_ROOT/client-iced"
  if (( fix )); then
    run "client-iced: cargo fmt" cargo fmt
  else
    run "client-iced: cargo fmt --check" cargo fmt -- --check
  fi
  run "client-iced: cargo check --all-targets" cargo check --all-targets
  run "client-iced: cargo clippy --all-targets -- -D warnings" \
    cargo clippy --all-targets -- -D warnings
  run "client-iced: cargo test" cargo test
}

client_checks() {
  case "$client_target" in
    all)   client_tauri_checks; client_iced_checks ;;
    tauri) client_tauri_checks ;;
    iced)  client_iced_checks ;;
  esac
}

case "$target" in
  server) server_checks ;;
  client) client_checks ;;
  all)    server_checks; client_checks ;;
esac

printf '\n%sAll checks passed.%s\n' "$green$bold" "$reset"
