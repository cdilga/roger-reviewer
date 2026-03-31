#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"

SESSION_NAME="${DEFAULT_SESSION_NAME}-swarm"
INTERVAL_SECONDS=15
IDLE_SECONDS=30
COOLDOWN_SECONDS=120
MAX_LINES=40
STATE_ROOT="${TMPDIR:-/tmp}/roger-reviewer-swarm-supervisor"

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME            Tmux swarm session to supervise
  --interval-seconds N      Polling interval between checks
  --idle-seconds N          How long a pane must sit idle before nudging
  --cooldown-seconds N      Minimum gap between nudges for the same pane
  --lines N                 Number of captured lines to inspect per pane
  -h, --help                Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      SESSION_NAME="$2"
      shift 2
      ;;
    --interval-seconds)
      INTERVAL_SECONDS="$2"
      shift 2
      ;;
    --idle-seconds)
      IDLE_SECONDS="$2"
      shift 2
      ;;
    --cooldown-seconds)
      COOLDOWN_SECONDS="$2"
      shift 2
      ;;
    --lines)
      MAX_LINES="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if ! tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  echo "Tmux session '$SESSION_NAME' does not exist." >&2
  exit 1
fi

mkdir -p "${STATE_ROOT}/${SESSION_NAME}"

continuation_prompt() {
  cat <<'EOF'
Continue autonomously from the current repo state. Because this is a persistent tmux swarm session, do not stop after a single checkpoint.

Immediately:
1. check Agent Mail and acknowledge anything pending
2. run `br ready`
3. inspect the best unblocked bead with `br show <id>`
4. claim it, reserve files, and keep working

If `br` says `database is busy`, wait briefly and retry before treating the queue as empty.
If `br ready` is empty but the next safe slice is obvious, create or split the bead needed to continue safely, include the validation contract, and announce it in Agent Mail instead of idling silently.
EOF
}

worker_panes() {
  tmux list-panes -a -t "$SESSION_NAME" -F '#{window_name} #{pane_id} #{pane_dead}' | while read -r window_name pane_id pane_dead; do
    case "$window_name" in
      control|supervisor)
        continue
        ;;
    esac
    printf '%s %s %s\n' "$window_name" "$pane_id" "$pane_dead"
  done
}

ready_count() {
  (
    cd "$PROJECT_ROOT"
    br ready 2>/dev/null || true
  ) | awk '/^[0-9]+\./ { count += 1 } END { print count + 0 }'
}

pane_capture() {
  local pane_id="$1"
  tmux capture-pane -pJ -t "$pane_id" | tail -n "$MAX_LINES"
}

pane_is_idle_prompt() {
  local capture="$1"
  printf '%s\n' "$capture" | tail -n 8 | grep -Eq '^[[:space:]]*(›|❯|>) '
}

pane_is_working() {
  local capture="$1"
  printf '%s\n' "$capture" | tail -n 20 | grep -Eq 'Working \(|thinking|Thinking|tool call|Esc to interrupt'
}

state_file() {
  local pane_id="$1"
  local suffix="$2"
  local safe_pane
  safe_pane=$(printf '%s' "$pane_id" | tr -cd '[:alnum:]')
  printf '%s/%s/%s.%s\n' "$STATE_ROOT" "$SESSION_NAME" "$safe_pane" "$suffix"
}

mark_active() {
  local pane_id="$1"
  rm -f "$(state_file "$pane_id" idle_hash)" "$(state_file "$pane_id" idle_since)" >/dev/null 2>&1 || true
}

nudge_pane() {
  local pane_id="$1"
  local pane_name="$2"
  local now_ts="$3"
  tmux set-buffer -- "$(continuation_prompt)"
  tmux paste-buffer -t "$pane_id"
  tmux send-keys -t "$pane_id" C-m
  printf '%s nudged %s (%s)\n' "$(date -Iseconds)" "$pane_name" "$pane_id"
  printf '%s\n' "$now_ts" >"$(state_file "$pane_id" last_nudge)"
}

while true; do
  ready=$(ready_count)
  if [[ "$ready" -eq 0 ]]; then
    sleep "$INTERVAL_SECONDS"
    continue
  fi

  nudged_this_cycle=0

  while read -r pane_name pane_id pane_dead; do
    [[ -n "$pane_id" ]] || continue
    [[ "$pane_dead" == "0" ]] || continue
    [[ "$nudged_this_cycle" -lt "$ready" ]] || break

    capture=$(pane_capture "$pane_id")

    if pane_is_working "$capture"; then
      mark_active "$pane_id"
      continue
    fi

    if ! pane_is_idle_prompt "$capture"; then
      mark_active "$pane_id"
      continue
    fi

    now_ts=$(date +%s)
    idle_hash=$(printf '%s' "$capture" | shasum | awk '{print $1}')
    hash_file=$(state_file "$pane_id" idle_hash)
    idle_since_file=$(state_file "$pane_id" idle_since)
    last_nudge_file=$(state_file "$pane_id" last_nudge)

    previous_hash=""
    idle_since_ts="$now_ts"
    last_nudge_ts=0

    if [[ -f "$hash_file" ]]; then
      previous_hash=$(cat "$hash_file")
    fi
    if [[ -f "$idle_since_file" ]]; then
      idle_since_ts=$(cat "$idle_since_file")
    fi
    if [[ -f "$last_nudge_file" ]]; then
      last_nudge_ts=$(cat "$last_nudge_file")
    fi

    if [[ "$idle_hash" != "$previous_hash" ]]; then
      printf '%s\n' "$idle_hash" >"$hash_file"
      printf '%s\n' "$now_ts" >"$idle_since_file"
      continue
    fi

    if (( now_ts - idle_since_ts < IDLE_SECONDS )); then
      continue
    fi

    if (( now_ts - last_nudge_ts < COOLDOWN_SECONDS )); then
      continue
    fi

    nudge_pane "$pane_id" "$pane_name" "$now_ts"
    nudged_this_cycle=$((nudged_this_cycle + 1))
    sleep 2
  done < <(worker_panes)

  sleep "$INTERVAL_SECONDS"
done
