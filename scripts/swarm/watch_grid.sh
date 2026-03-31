#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"

SOURCE_SESSION="${DEFAULT_SESSION_NAME}-swarm"
DASH_SESSION="${SOURCE_SESSION}-watch"
LINES=20
INTERVAL_SECONDS=2
ATTACH=1

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME            Source tmux swarm session to watch
  --lines N                 Number of lines to tail from each agent pane
  --interval-seconds N      Refresh interval for each dashboard pane
  --no-attach               Create/update the dashboard session without attaching
  -h, --help                Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      SOURCE_SESSION="$2"
      DASH_SESSION="${SOURCE_SESSION}-watch"
      shift 2
      ;;
    --lines)
      LINES="$2"
      shift 2
      ;;
    --interval-seconds)
      INTERVAL_SECONDS="$2"
      shift 2
      ;;
    --no-attach)
      ATTACH=0
      shift
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

if ! tmux has-session -t "$SOURCE_SESSION" 2>/dev/null; then
  echo "Tmux session '$SOURCE_SESSION' does not exist." >&2
  exit 1
fi

WINDOWS=()
while IFS= read -r window_line; do
  [[ -n "$window_line" ]] || continue
  WINDOWS+=("$window_line")
done < <(tmux list-windows -t "$SOURCE_SESSION" -F '#{window_index} #{window_name}' | awk '$2 != "control" { print }')

if [[ "${#WINDOWS[@]}" -eq 0 ]]; then
  echo "No non-control windows found in '$SOURCE_SESSION'." >&2
  exit 1
fi

pane_command() {
  local target="$1"
  local label="$2"
  cat <<EOF
while true; do
  clear
  printf '%s\n%s\n\n' '$label' "\$(date -Iseconds)"
  tmux capture-pane -p -t '$target' | tail -n '$LINES'
  sleep '$INTERVAL_SECONDS'
done
EOF
}

start_loop_in_pane() {
  local pane_target="$1"
  local source_target="$2"
  local label="$3"
  local cmd
  cmd=$(pane_command "$source_target" "$label")
  tmux send-keys -t "$pane_target" "$cmd" C-m
}

first_window="${WINDOWS[0]}"
first_index="${first_window%% *}"
first_name="${first_window#* }"

tmux kill-session -t "$DASH_SESSION" 2>/dev/null || true
tmux new-session -d -s "$DASH_SESSION" -n grid
start_loop_in_pane "${DASH_SESSION}:0.0" "${SOURCE_SESSION}:${first_index}" "$first_name"

for window in "${WINDOWS[@]:1}"; do
  window_index="${window%% *}"
  window_name="${window#* }"
  pane_id=$(tmux split-window -t "$DASH_SESSION:0" -P -F '#{pane_id}')
  start_loop_in_pane "$pane_id" "${SOURCE_SESSION}:${window_index}" "$window_name"
  tmux select-layout -t "$DASH_SESSION:0" tiled >/dev/null
done

tmux set-option -t "$DASH_SESSION" remain-on-exit on >/dev/null

if (( ATTACH == 1 )); then
  exec tmux attach -t "$DASH_SESSION"
fi

echo "Dashboard session ready: $DASH_SESSION"
