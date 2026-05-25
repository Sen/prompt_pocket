#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_BIN="$ROOT_DIR/target/debug/prompt-pocket"
POLL_INTERVAL="${DEV_POLL_INTERVAL:-1}"
APP_PID=""
WATCH_PATHS=(
  "Cargo.toml"
  "Cargo.lock"
  ".env"
  "src"
  "templates"
  "static"
)

cd "$ROOT_DIR"

snapshot() {
  for path in "${WATCH_PATHS[@]}"; do
    if [[ -f "$path" ]]; then
      printf '%s\n' "$path"
    elif [[ -d "$path" ]]; then
      find "$path" -type f
    fi
  done | LC_ALL=C sort | while IFS= read -r file; do
    if [[ -f "$file" ]]; then
      cksum "$file"
    fi
  done
}

stop_app() {
  if [[ -n "$APP_PID" ]] && kill -0 "$APP_PID" 2>/dev/null; then
    echo "[dev] Stopping Prompt Pocket..."
    kill "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
  fi

  APP_PID=""
}

start_app() {
  echo "[dev] Building Prompt Pocket..."

  if ! cargo build; then
    echo "[dev] Build failed. Fix the error and save a watched file to retry."
    return
  fi

  echo "[dev] Starting Prompt Pocket..."
  "$APP_BIN" "$@" &
  APP_PID="$!"
}

cleanup() {
  stop_app
}

trap cleanup EXIT INT TERM

echo "[dev] Watching src/, templates/, static/, Cargo files, and .env."
echo "[dev] Set DEV_POLL_INTERVAL to change the ${POLL_INTERVAL}s polling interval."

last_snapshot="$(snapshot)"
start_app "$@"

while true; do
  sleep "$POLL_INTERVAL"

  if [[ -n "$APP_PID" ]] && ! kill -0 "$APP_PID" 2>/dev/null; then
    wait "$APP_PID" 2>/dev/null || true
    APP_PID=""
    echo "[dev] Prompt Pocket is not running. Save a watched file to rebuild."
  fi

  next_snapshot="$(snapshot)"

  if [[ "$next_snapshot" != "$last_snapshot" ]]; then
    last_snapshot="$next_snapshot"
    echo "[dev] Change detected. Rebuilding..."
    stop_app
    start_app "$@"
  fi
done
