#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"

SESSION_NAME="${DEFAULT_SESSION_NAME}-swarm"
LINES=20

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME      Tmux session to inspect
  --lines N           Number of pane lines to capture per window
  -h, --help          Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      SESSION_NAME="$2"
      shift 2
      ;;
    --lines)
      LINES="$2"
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

echo "== $SESSION_NAME windows =="
tmux list-windows -t "$SESSION_NAME" -F '#{window_index}: #{window_name} [#{window_active}] #{pane_current_command}'

echo
echo "== Active in-progress beads =="
(
  cd "$PROJECT_ROOT"
  br list --status in_progress || true
)

echo
while IFS=' ' read -r window_index window_name; do
  case "$window_name" in
    control|supervisor)
      continue
      ;;
  esac

  echo "== ${SESSION_NAME}:${window_name} =="
  tmux capture-pane -p -t "${SESSION_NAME}:${window_index}" | tail -n "$LINES"
  echo
done < <(tmux list-windows -t "$SESSION_NAME" -F '#{window_index} #{window_name}')
