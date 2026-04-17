#!/usr/bin/env bash
# Launches two Hamlet client instances with isolated WebView data directories
# so they can be logged in as different users at the same time.
#
# Instance A runs via `npm run tauri dev` (Vite + first build + window).
# Instance B runs the debug binary directly once the first build finishes,
# reusing the same Vite dev server on port 1420.
set -euo pipefail

cd "$(dirname "$0")/.."

DATA_A="${TMPDIR:-/tmp}/hamlet-client-a"
DATA_B="${TMPDIR:-/tmp}/hamlet-client-b"
BIN="src-tauri/target/debug/client"

mkdir -p "$DATA_A" "$DATA_B"

HAMLET_DATA_DIR="$DATA_A" npm run tauri dev &
TAURI_PID=$!

cleanup() {
    kill "$TAURI_PID" "${CLIENT_B_PID:-}" 2>/dev/null || true
}
trap cleanup INT TERM EXIT

echo "Waiting for Vite on :1420..."
until curl -sf http://localhost:1420 >/dev/null 2>&1; do
    kill -0 "$TAURI_PID" 2>/dev/null || { echo "tauri dev exited early" >&2; exit 1; }
    sleep 1
done

echo "Waiting for $BIN to finish building..."
until [ -x "$BIN" ]; do
    kill -0 "$TAURI_PID" 2>/dev/null || { echo "tauri dev exited early" >&2; exit 1; }
    sleep 1
done
sleep 2

HAMLET_DATA_DIR="$DATA_B" "$BIN" &
CLIENT_B_PID=$!

echo "Client A pid=$TAURI_PID data=$DATA_A"
echo "Client B pid=$CLIENT_B_PID data=$DATA_B"

wait
