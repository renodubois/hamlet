#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
base_name="$(basename "$repo_root")"
worktree_root=${HAMLET_WORKTREE_ROOT:-"$HOME/projects/${base_name}-wt"}
base_ref=${HAMLET_WORKTREE_BASE_REF:-HEAD}
copy_current=${HAMLET_WORKTREE_COPY_CURRENT:-0}
port_db=${HAMLET_WORKTREE_PORT_DB:-"${XDG_STATE_HOME:-$HOME/.local/state}/hamlet/worktree-ports.sqlite"}
branch=""
worktree_dir=""
reallocate_ports=0
mode="create"
release_target=""

usage() {
  cat <<'USAGE'
Create one Hamlet development worktree, assign it a branch, and reserve isolated ports.

Usage:
  scripts/create-worktree.sh [options] <branch>
  scripts/create-worktree.sh --list-port-allocations
  scripts/create-worktree.sh --release-port-allocation <worktree-dir>
  scripts/create-worktree.sh --prune-port-allocations

Options:
  --dir <dir>                    Worktree directory. Default: $HAMLET_WORKTREE_ROOT/<branch-slug>
  --base-ref <ref>               Ref to branch from when <branch> does not exist. Default: $HAMLET_WORKTREE_BASE_REF or HEAD
  --copy-current                 Rsync current working tree changes into the worktree after creation
  --reallocate-ports             Replace any existing port allocation for this worktree
  --port-db <path>               SQLite allocation DB. Default: $HAMLET_WORKTREE_PORT_DB or
                                  ${XDG_STATE_HOME:-$HOME/.local/state}/hamlet/worktree-ports.sqlite
  -h, --help                     Show this help

Environment:
  HAMLET_WORKTREE_ROOT                 Directory that contains worktrees (default: ~/projects/hamlet-wt)
  HAMLET_WORKTREE_BASE_REF             Default base ref for new branches (default: HEAD)
  HAMLET_WORKTREE_COPY_CURRENT         1 to copy current changes after creation
  HAMLET_WORKTREE_PORT_DB              Persistent SQLite port allocation DB
  HAMLET_WORKTREE_SERVER_PORT_START    First server port candidate (default: 3130)
  HAMLET_WORKTREE_SERVER_PORT_STEP     Server port step per slot (default: 100)
  HAMLET_WORKTREE_RENDERER_PORT_START  First renderer port candidate (default: 1432)
  HAMLET_WORKTREE_RENDERER_PORT_STEP   Renderer port step per slot (default: 10)
  HAMLET_WORKTREE_LIVEKIT_PORT_START   First LiveKit WS port candidate (default: 7890)
  HAMLET_WORKTREE_LIVEKIT_PORT_STEP    LiveKit port step per slot (default: 10)
  HAMLET_WORKTREE_LIVEKIT_TCP_OFFSET   LiveKit TCP offset from WS port (default: 1)
  HAMLET_WORKTREE_UDP_START            First LiveKit UDP range start (default: 50110)
  HAMLET_WORKTREE_UDP_STEP             UDP range step per slot (default: 110)
  HAMLET_WORKTREE_UDP_SIZE             UDP ports per slot (default: 101)
  HAMLET_WORKTREE_PORT_SLOT_LIMIT      Number of candidate slots to try (default: 100)
USAGE
}

require_command() {
  local command_name=$1
  if ! command -v "$command_name" >/dev/null 2>&1; then
    printf 'missing required command: %s\n' "$command_name" >&2
    exit 1
  fi
}

absolute_path() {
  python3 - "$1" <<'PY'
import os
import sys
print(os.path.abspath(os.path.expanduser(sys.argv[1])))
PY
}

slugify_branch() {
  local value=$1
  value=$(printf '%s' "$value" | tr '/[:space:]' '-' | tr -c 'A-Za-z0-9._-' '-')
  value=$(printf '%s' "$value" | sed -E 's/-+/-/g; s/^-//; s/-$//')
  if [[ -z "$value" ]]; then
    value="worktree"
  fi
  printf '%s' "$value"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dir)
      worktree_dir=${2:-}
      if [[ -z "$worktree_dir" ]]; then
        printf '%s\n' '--dir requires a value' >&2
        exit 2
      fi
      shift 2
      ;;
    --base-ref)
      base_ref=${2:-}
      if [[ -z "$base_ref" ]]; then
        printf '%s\n' '--base-ref requires a value' >&2
        exit 2
      fi
      shift 2
      ;;
    --copy-current)
      copy_current=1
      shift
      ;;
    --reallocate-ports)
      reallocate_ports=1
      shift
      ;;
    --port-db)
      port_db=${2:-}
      if [[ -z "$port_db" ]]; then
        printf '%s\n' '--port-db requires a value' >&2
        exit 2
      fi
      shift 2
      ;;
    --list-port-allocations)
      mode="list"
      shift
      ;;
    --release-port-allocation)
      mode="release"
      release_target=${2:-}
      if [[ -z "$release_target" ]]; then
        printf '%s\n' '--release-port-allocation requires a value' >&2
        exit 2
      fi
      shift 2
      ;;
    --prune-port-allocations)
      mode="prune"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    --*)
      printf 'unknown option: %s\n' "$1" >&2
      usage >&2
      exit 2
      ;;
    *)
      if [[ -n "$branch" ]]; then
        printf 'unexpected argument: %s\n' "$1" >&2
        usage >&2
        exit 2
      fi
      branch=$1
      shift
      ;;
  esac
done

if [[ $# -gt 0 ]]; then
  if [[ -n "$branch" ]]; then
    printf 'unexpected argument: %s\n' "$1" >&2
    usage >&2
    exit 2
  fi
  branch=$1
  shift
fi
if [[ $# -gt 0 ]]; then
  printf 'unexpected argument: %s\n' "$1" >&2
  usage >&2
  exit 2
fi

require_command python3

manage_allocations() {
  local action=$1
  local target=${2:-}
  python3 - "$port_db" "$action" "$target" <<'PY'
import os
import sqlite3
import sys
import time

path, action, target = sys.argv[1:4]
os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
conn = sqlite3.connect(path, timeout=30)
conn.execute("PRAGMA busy_timeout = 30000")
conn.execute(
    "CREATE TABLE IF NOT EXISTS allocations ("
    "owner TEXT PRIMARY KEY, "
    "branch TEXT NOT NULL, "
    "server_port INTEGER NOT NULL, "
    "renderer_port INTEGER NOT NULL, "
    "livekit_port INTEGER NOT NULL, "
    "livekit_tcp_port INTEGER NOT NULL, "
    "udp_start INTEGER NOT NULL, "
    "udp_end INTEGER NOT NULL, "
    "created_at INTEGER NOT NULL, "
    "updated_at INTEGER NOT NULL"
    ")"
)
conn.commit()

if action == "list":
    rows = conn.execute(
        "SELECT owner, branch, server_port, renderer_port, livekit_port, livekit_tcp_port, "
        "udp_start, udp_end, updated_at FROM allocations ORDER BY owner"
    ).fetchall()
    if not rows:
        print(f"no port allocations in {path}")
    else:
        print(f"port allocations in {path}:")
        for row in rows:
            owner, branch, server, renderer, livekit, livekit_tcp, udp_start, udp_end, updated_at = row
            print(
                f"{owner}\n"
                f"  branch={branch}\n"
                f"  server={server} renderer={renderer} "
                f"livekit={livekit}/{livekit_tcp} udp={udp_start}-{udp_end} "
                f"updated_at={time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(updated_at))}"
            )
    raise SystemExit(0)

if action == "release":
    owner = os.path.abspath(os.path.expanduser(target))
    cur = conn.execute("DELETE FROM allocations WHERE owner = ?", (owner,))
    conn.commit()
    if cur.rowcount:
        print(f"released port allocation for {owner}")
    else:
        print(f"no port allocation found for {owner}")
    raise SystemExit(0)

if action == "prune":
    rows = conn.execute("SELECT owner FROM allocations ORDER BY owner").fetchall()
    removed = []
    for (owner,) in rows:
        if not os.path.exists(owner):
            conn.execute("DELETE FROM allocations WHERE owner = ?", (owner,))
            removed.append(owner)
    conn.commit()
    if removed:
        for owner in removed:
            print(f"pruned missing worktree allocation: {owner}")
    else:
        print("no missing worktree allocations to prune")
    raise SystemExit(0)

print(f"unknown allocation action: {action}", file=sys.stderr)
raise SystemExit(2)
PY
}

case "$mode" in
  list)
    manage_allocations list
    exit 0
    ;;
  release)
    require_command python3
    release_target=$(absolute_path "$release_target")
    manage_allocations release "$release_target"
    exit 0
    ;;
  prune)
    manage_allocations prune
    exit 0
    ;;
esac

if [[ -z "$branch" ]]; then
  printf 'missing required <branch>\n' >&2
  usage >&2
  exit 2
fi

require_command git

if [[ -z "$worktree_dir" ]]; then
  worktree_dir="$worktree_root/$(slugify_branch "$branch")"
fi
worktree_dir=$(absolute_path "$worktree_dir")
port_db=$(absolute_path "$port_db")

mkdir -p "$(dirname "$worktree_dir")"

create_or_update_worktree() {
  if [[ -e "$worktree_dir/.git" ]]; then
    if ! git -C "$worktree_dir" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
      printf 'path exists but is not a git worktree: %s\n' "$worktree_dir" >&2
      exit 1
    fi
    if git -C "$repo_root" show-ref --verify --quiet "refs/heads/$branch"; then
      current_branch=$(git -C "$worktree_dir" branch --show-current || true)
      if [[ "$current_branch" != "$branch" ]]; then
        git -C "$worktree_dir" checkout "$branch"
      fi
    else
      git -C "$worktree_dir" checkout -b "$branch" "$base_ref"
    fi
    return
  fi

  if [[ -e "$worktree_dir" ]]; then
    printf 'path exists but is not a git worktree: %s\n' "$worktree_dir" >&2
    exit 1
  fi

  if git -C "$repo_root" show-ref --verify --quiet "refs/heads/$branch"; then
    git -C "$repo_root" worktree add "$worktree_dir" "$branch"
  else
    git -C "$repo_root" worktree add -b "$branch" "$worktree_dir" "$base_ref"
  fi
}

link_shared_dependencies() {
  local dir=$1
  if [[ -d "$repo_root/client/node_modules" && ! -e "$dir/client/node_modules" ]]; then
    ln -s "$repo_root/client/node_modules" "$dir/client/node_modules"
  fi
}

copy_current_tree() {
  local dir=$1
  rsync -a --delete \
    --exclude .git \
    --exclude client/node_modules \
    --exclude client/dist \
    --exclude client/dist-electron \
    --exclude client/release \
    --exclude client/test-results \
    --exclude client/playwright-report \
    --exclude server/target \
    --exclude server/uploads \
    "$repo_root/" "$dir/"
}

allocate_ports() {
  local owner=$1 allocation_branch=$2 reallocate=$3
  python3 - "$port_db" "$owner" "$allocation_branch" "$reallocate" <<'PY'
import os
import socket
import sqlite3
import sys
import time

path, owner, branch, reallocate = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4] == "1"

def env_int(name, default):
    value = os.environ.get(name)
    if value is None or value == "":
        return default
    try:
        parsed = int(value)
    except ValueError:
        print(f"{name} must be an integer", file=sys.stderr)
        raise SystemExit(2)
    return parsed

server_start = env_int("HAMLET_WORKTREE_SERVER_PORT_START", 3130)
server_step = env_int("HAMLET_WORKTREE_SERVER_PORT_STEP", 100)
renderer_start = env_int("HAMLET_WORKTREE_RENDERER_PORT_START", 1432)
renderer_step = env_int("HAMLET_WORKTREE_RENDERER_PORT_STEP", 10)
livekit_start = env_int("HAMLET_WORKTREE_LIVEKIT_PORT_START", 7890)
livekit_step = env_int("HAMLET_WORKTREE_LIVEKIT_PORT_STEP", 10)
livekit_tcp_offset = env_int("HAMLET_WORKTREE_LIVEKIT_TCP_OFFSET", 1)
udp_start_base = env_int("HAMLET_WORKTREE_UDP_START", 50110)
udp_step = env_int("HAMLET_WORKTREE_UDP_STEP", 110)
udp_size = env_int("HAMLET_WORKTREE_UDP_SIZE", 101)
slot_limit = env_int("HAMLET_WORKTREE_PORT_SLOT_LIMIT", 100)

if udp_size < 1 or slot_limit < 1:
    print("HAMLET_WORKTREE_UDP_SIZE and HAMLET_WORKTREE_PORT_SLOT_LIMIT must be positive", file=sys.stderr)
    raise SystemExit(2)

os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
conn = sqlite3.connect(path, timeout=30, isolation_level=None)
conn.execute("PRAGMA busy_timeout = 30000")
conn.execute(
    "CREATE TABLE IF NOT EXISTS allocations ("
    "owner TEXT PRIMARY KEY, "
    "branch TEXT NOT NULL, "
    "server_port INTEGER NOT NULL, "
    "renderer_port INTEGER NOT NULL, "
    "livekit_port INTEGER NOT NULL, "
    "livekit_tcp_port INTEGER NOT NULL, "
    "udp_start INTEGER NOT NULL, "
    "udp_end INTEGER NOT NULL, "
    "created_at INTEGER NOT NULL, "
    "updated_at INTEGER NOT NULL"
    ")"
)
conn.execute("BEGIN IMMEDIATE")
now = int(time.time())

existing = conn.execute(
    "SELECT server_port, renderer_port, livekit_port, livekit_tcp_port, udp_start, udp_end "
    "FROM allocations WHERE owner = ?",
    (owner,),
).fetchone()
if existing and not reallocate:
    conn.execute("UPDATE allocations SET branch = ?, updated_at = ? WHERE owner = ?", (branch, now, owner))
    conn.commit()
    print("\t".join(str(value) for value in existing))
    raise SystemExit(0)

if existing and reallocate:
    conn.execute("DELETE FROM allocations WHERE owner = ?", (owner,))

rows = conn.execute(
    "SELECT server_port, renderer_port, livekit_port, livekit_tcp_port, udp_start, udp_end "
    "FROM allocations"
).fetchall()

def valid_port(port):
    return 1 <= port <= 65535

def tcp_free(port):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 0)
        try:
            sock.bind(("0.0.0.0", port))
        except OSError:
            return False
    return True

def udp_free(port):
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 0)
        try:
            sock.bind(("0.0.0.0", port))
        except OSError:
            return False
    return True

def number_reserved(port):
    for server, renderer, livekit, livekit_tcp, udp_start, udp_end in rows:
        if port in (server, renderer, livekit, livekit_tcp):
            return True
        if udp_start <= port <= udp_end:
            return True
    return False

def range_reserved(start, end):
    for server, renderer, livekit, livekit_tcp, udp_start, udp_end in rows:
        if any(start <= port <= end for port in (server, renderer, livekit, livekit_tcp)):
            return True
        if start <= udp_end and udp_start <= end:
            return True
    return False

for slot in range(slot_limit):
    server_port = server_start + slot * server_step
    renderer_port = renderer_start + slot * renderer_step
    livekit_port = livekit_start + slot * livekit_step
    livekit_tcp_port = livekit_port + livekit_tcp_offset
    udp_start = udp_start_base + slot * udp_step
    udp_end = udp_start + udp_size - 1
    tcp_ports = (server_port, renderer_port, livekit_port, livekit_tcp_port)

    if not all(valid_port(port) for port in tcp_ports) or not (valid_port(udp_start) and valid_port(udp_end)):
        continue
    if any(number_reserved(port) for port in tcp_ports) or range_reserved(udp_start, udp_end):
        continue
    if not all(tcp_free(port) for port in tcp_ports):
        continue
    if not all(udp_free(port) for port in range(udp_start, udp_end + 1)):
        continue

    conn.execute(
        "INSERT INTO allocations "
        "(owner, branch, server_port, renderer_port, livekit_port, livekit_tcp_port, udp_start, udp_end, created_at, updated_at) "
        "VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        (owner, branch, server_port, renderer_port, livekit_port, livekit_tcp_port, udp_start, udp_end, now, now),
    )
    conn.commit()
    print("\t".join(str(value) for value in (server_port, renderer_port, livekit_port, livekit_tcp_port, udp_start, udp_end)))
    raise SystemExit(0)

conn.rollback()
print(
    f"could not find a free Hamlet port slot after checking {slot_limit} candidates; "
    f"try --list-port-allocations, --prune-port-allocations, or adjust HAMLET_WORKTREE_* port settings",
    file=sys.stderr,
)
raise SystemExit(1)
PY
}

write_env_files() {
  local dir=$1 server_port=$2 renderer_port=$3 livekit_port=$4 livekit_tcp=$5 udp_start=$6 udp_end=$7
  local server_url="http://127.0.0.1:${server_port}"
  local renderer_url="http://127.0.0.1:${renderer_port}"
  local livekit_url="ws://127.0.0.1:${livekit_port}"

  cat > "$dir/.hamlet-worktree.env" <<EOF
# Source this file before running cargo/pnpm commands in this worktree:
#   source .hamlet-worktree.env
export HAMLET_SERVER_PORT=${server_port}
export HAMLET_SERVER_URL=${server_url}
export HAMLET_BIND_ADDR=127.0.0.1:${server_port}
export HAMLET_RENDERER_HOST=127.0.0.1
export HAMLET_RENDERER_PORT=${renderer_port}
export HAMLET_RENDERER_URL=${renderer_url}
export VITE_HAMLET_DEFAULT_SERVER_URL=${server_url}
export LIVEKIT_URL=${livekit_url}
export LIVEKIT_API_KEY=devkey
export LIVEKIT_API_SECRET=devsecretdevsecretdevsecretdevsecret
export HAMLET_LIVEKIT_PORT=${livekit_port}
export HAMLET_LIVEKIT_TCP_PORT=${livekit_tcp}
export HAMLET_LIVEKIT_UDP_START=${udp_start}
export HAMLET_LIVEKIT_UDP_END=${udp_end}
export HAMLET_LIVEKIT_CONFIG=./livekit.local.yaml
export HAMLET_VOICE_COMPOSE_PROJECT=hamlet_voice_wt${server_port}
EOF

  cat > "$dir/client/.env.local" <<EOF
HAMLET_RENDERER_HOST=127.0.0.1
HAMLET_RENDERER_PORT=${renderer_port}
VITE_HAMLET_DEFAULT_SERVER_URL=${server_url}
EOF

  cat > "$dir/server/.env" <<EOF
HAMLET_SERVER_PORT=${server_port}
HAMLET_BIND_ADDR=127.0.0.1:${server_port}
LIVEKIT_URL=${livekit_url}
LIVEKIT_API_KEY=devkey
LIVEKIT_API_SECRET=devsecretdevsecretdevsecretdevsecret
HAMLET_LIVEKIT_PORT=${livekit_port}
HAMLET_LIVEKIT_TCP_PORT=${livekit_tcp}
HAMLET_LIVEKIT_UDP_START=${udp_start}
HAMLET_LIVEKIT_UDP_END=${udp_end}
HAMLET_LIVEKIT_CONFIG=./livekit.local.yaml
EOF

  (
    cd "$dir/server"
    HAMLET_SERVER_PORT="$server_port" \
      HAMLET_LIVEKIT_PORT="$livekit_port" \
      HAMLET_LIVEKIT_TCP_PORT="$livekit_tcp" \
      HAMLET_LIVEKIT_UDP_START="$udp_start" \
      HAMLET_LIVEKIT_UDP_END="$udp_end" \
      LIVEKIT_API_KEY=devkey \
      LIVEKIT_API_SECRET=devsecretdevsecretdevsecretdevsecret \
      ./scripts/write-livekit-config.sh ./livekit.local.yaml >/dev/null
  )
}

create_or_update_worktree

if (( copy_current )); then
  copy_current_tree "$worktree_dir"
fi

link_shared_dependencies "$worktree_dir"

ports_tsv=$(allocate_ports "$worktree_dir" "$branch" "$reallocate_ports")
IFS=$'\t' read -r server_port renderer_port livekit_port livekit_tcp udp_start udp_end <<< "$ports_tsv"

write_env_files "$worktree_dir" "$server_port" "$renderer_port" "$livekit_port" "$livekit_tcp" "$udp_start" "$udp_end"

cat <<NEXT
ready: $worktree_dir ($branch)
ports: server $server_port, renderer $renderer_port, LiveKit $livekit_port/$livekit_tcp, UDP $udp_start-$udp_end
port allocation db: $port_db

Use this worktree like this:
  cd $worktree_dir
  source .hamlet-worktree.env
  cd server && cargo run              # or: docker compose up
  cd ../client && pnpm run electron:dev
NEXT
