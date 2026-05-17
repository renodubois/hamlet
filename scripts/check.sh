#!/usr/bin/env bash
# Local CI: run the same checks GitHub Actions would, without leaving your machine.

set -eo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<'USAGE'
Local CI: run the same checks GitHub Actions would, without leaving your machine.

Usage:
  scripts/check.sh                         # everything (server + all available clients)
  scripts/check.sh server                  # just the Rust backend
  scripts/check.sh client                  # all available clients
  scripts/check.sh client tauri            # just the Tauri/Solid client
  scripts/check.sh client electron         # just the Electron/Solid client
  scripts/check.sh client iced             # just the native Iced client
  scripts/check.sh all --client iced       # server + native Iced client
  scripts/check.sh all --client electron   # server + Electron/Solid client
  scripts/check.sh --fix                   # apply fmt fixes before running checks
  scripts/check.sh --e2e                   # also run Playwright E2E tests
  scripts/check.sh client iced --fix       # combinable

Client checks:
  tauri    -> client/: npm run fmt:check (or fmt with --fix), lint, typecheck, test
  electron -> client-electron/: npm run fmt:check (or fmt with --fix), lint,
              typecheck, test
  iced     -> client-iced/: cargo fmt -- --check (or fmt with --fix),
              cargo check --all-targets, cargo clippy --all-targets -- -D warnings,
              cargo test

Optional checks:
  --e2e -> run Playwright E2E for selected web clients:
           client/: npm run test:e2e; client-electron/: npm run test:e2e
           when selected or when client-electron/ exists under the all selector

When checking all clients, client-electron/ is skipped until that directory exists.
Selecting `electron` explicitly requires client-electron/ to be present.

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
run_e2e=0

set_client_target() {
  local value="$1"
  if (( client_seen )); then
    die "multiple client selectors provided"
  fi
  case "$value" in
    tauri|electron|iced)
      client_target="$value"
      client_seen=1
      ;;
    *) die "invalid client selector: $value (expected tauri, electron, or iced)" ;;
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
    tauri|electron|iced)
      set_client_target "$1"
      ;;
    --client)
      shift
      [[ $# -gt 0 ]] || die "--client requires a value: tauri, electron, or iced"
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

client_electron_checks() {
  if [[ ! -d "$REPO_ROOT/client-electron" ]]; then
    die "client-electron/ does not exist yet"
  fi

  cd "$REPO_ROOT/client-electron"
  if (( fix )); then
    run "client-electron: npm run fmt" npm run fmt
  else
    run "client-electron: npm run fmt:check" npm run fmt:check
  fi
  run "client-electron: npm run lint" npm run lint
  run "client-electron: npm run typecheck" npm run typecheck
  run "client-electron: npm run test" npm run test
}

maybe_client_electron_checks() {
  if [[ -d "$REPO_ROOT/client-electron" ]]; then
    client_electron_checks
  else
    skip "client-electron/ not found"
  fi
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
    all)      client_tauri_checks; maybe_client_electron_checks; client_iced_checks ;;
    tauri)    client_tauri_checks ;;
    electron) client_electron_checks ;;
    iced)     client_iced_checks ;;
  esac
}

client_tauri_e2e_checks() {
  cd "$REPO_ROOT/client"
  run "client-tauri: npm run test:e2e" npm run test:e2e
}

client_electron_e2e_checks() {
  if [[ ! -d "$REPO_ROOT/client-electron" ]]; then
    die "client-electron/ does not exist yet"
  fi

  cd "$REPO_ROOT/client-electron"
  run "client-electron: npm run test:e2e" npm run test:e2e
}

maybe_client_electron_e2e_checks() {
  if [[ -d "$REPO_ROOT/client-electron" ]]; then
    client_electron_e2e_checks
  else
    skip "client-electron/ E2E not run because client-electron/ was not found"
  fi
}

client_e2e_checks() {
  case "$client_target" in
    all)      client_tauri_e2e_checks; maybe_client_electron_e2e_checks ;;
    tauri)    client_tauri_e2e_checks ;;
    electron) client_electron_e2e_checks ;;
    iced)     skip "client-iced has no separate Playwright E2E step; cargo test covers native smoke tests" ;;
  esac
}

case "$target" in
  server) server_checks ;;
  client) client_checks ;;
  all)    server_checks; client_checks ;;
esac

if (( run_e2e )); then
  client_e2e_checks
fi

printf '\n%sAll checks passed.%s\n' "$green$bold" "$reset"
