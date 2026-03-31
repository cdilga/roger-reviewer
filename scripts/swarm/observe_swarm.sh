#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"

SESSION_NAME="${DEFAULT_SESSION_NAME}-swarm"
MAX_LINES=20
FORMAT="text"
INCLUDE_TEXT=0
PANE_FILTER=""

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME       Tmux swarm session to inspect
  --lines N            Number of captured lines to inspect per pane
  --json               Emit JSON instead of a text table
  --include-text       Include captured pane text in JSON output
  --pane NAME|ID       Restrict output to a specific window name or pane id
  -h, --help           Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      SESSION_NAME="$2"
      shift 2
      ;;
    --lines)
      MAX_LINES="$2"
      shift 2
      ;;
    --json)
      FORMAT="json"
      shift
      ;;
    --include-text)
      INCLUDE_TEXT=1
      shift
      ;;
    --pane)
      PANE_FILTER="$2"
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

if ! command -v tmux >/dev/null 2>&1; then
  echo "Missing required command: tmux" >&2
  exit 1
fi

if ! tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  echo "Tmux session '$SESSION_NAME' does not exist." >&2
  exit 1
fi

ready_count() {
  if command -v br >/dev/null 2>&1 && [[ -f "${PROJECT_ROOT}/.beads/beads.db" ]]; then
    (
      cd "$PROJECT_ROOT"
      br ready 2>/dev/null || true
    ) | awk '/^[0-9]+\./ { count += 1 } END { print count + 0 }'
  else
    echo 0
  fi
}

worker_panes() {
  tmux list-panes -t "$SESSION_NAME" -F '#{window_name}|#{pane_id}|#{pane_dead}|#{pane_current_command}' | \
    while IFS='|' read -r window_name pane_id pane_dead pane_command; do
      case "$window_name" in
        control|supervisor)
          continue
          ;;
      esac
      if [[ -n "$PANE_FILTER" && "$PANE_FILTER" != "$window_name" && "$PANE_FILTER" != "$pane_id" ]]; then
        continue
      fi
      printf '%s|%s|%s|%s\n' "$window_name" "$pane_id" "$pane_dead" "$pane_command"
    done
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

pane_state() {
  local pane_dead="$1"
  local working="$2"
  local idle_prompt="$3"

  if [[ "$pane_dead" != "0" ]]; then
    printf 'dead\n'
  elif [[ "$working" == "true" ]]; then
    printf 'working\n'
  elif [[ "$idle_prompt" == "true" ]]; then
    printf 'idle\n'
  else
    printf 'active\n'
  fi
}

if [[ "$FORMAT" == "json" ]]; then
  tmpfile=$(mktemp)
  while IFS='|' read -r window_name pane_id pane_dead pane_command; do
    [[ -n "$pane_id" ]] || continue
    capture=$(pane_capture "$pane_id")
    working=false
    idle_prompt=false
    if pane_is_working "$capture"; then
      working=true
    fi
    if pane_is_idle_prompt "$capture"; then
      idle_prompt=true
    fi
    state=$(pane_state "$pane_dead" "$working" "$idle_prompt")
    if (( INCLUDE_TEXT == 0 )); then
      capture=""
    fi
    jq -n \
      --arg window_name "$window_name" \
      --arg pane_id "$pane_id" \
      --arg pane_command "$pane_command" \
      --arg state "$state" \
      --arg text "$capture" \
      --argjson pane_dead "$pane_dead" \
      --argjson working "$working" \
      --argjson idle_prompt "$idle_prompt" \
      '{
        window_name: $window_name,
        pane_id: $pane_id,
        pane_command: $pane_command,
        pane_dead: ($pane_dead != 0),
        state: $state,
        working: $working,
        idle_prompt: $idle_prompt,
        text: $text
      }' >>"$tmpfile"
  done < <(worker_panes)

  jq -s \
    --arg session "$SESSION_NAME" \
    --arg observed_at "$(date -Iseconds)" \
    --argjson ready_count "$(ready_count)" \
    '{
      session: $session,
      observed_at: $observed_at,
      ready_count: $ready_count,
      panes: .
    }' "$tmpfile"
  rm -f "$tmpfile"
  exit 0
fi

printf 'Session: %s\n' "$SESSION_NAME"
printf 'Observed: %s\n' "$(date -Iseconds)"
printf 'Ready beads: %s\n\n' "$(ready_count)"
printf '%-14s %-14s %-10s %-10s %s\n' "WINDOW" "PANE" "STATE" "COMMAND" "LAST LINE"

while IFS='|' read -r window_name pane_id pane_dead pane_command; do
  [[ -n "$pane_id" ]] || continue
  capture=$(pane_capture "$pane_id")
  working=false
  idle_prompt=false
  if pane_is_working "$capture"; then
    working=true
  fi
  if pane_is_idle_prompt "$capture"; then
    idle_prompt=true
  fi
  state=$(pane_state "$pane_dead" "$working" "$idle_prompt")
  last_line=$(printf '%s\n' "$capture" | tail -n 1)
  printf '%-14s %-14s %-10s %-10s %s\n' "$window_name" "$pane_id" "$state" "$pane_command" "$last_line"
done < <(worker_panes)
