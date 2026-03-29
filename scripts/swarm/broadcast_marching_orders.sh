#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"

SESSION_NAME="${DEFAULT_SESSION_NAME}-swarm"
PROMPT_FILE="${PROJECT_ROOT}/docs/swarm/overnight-marching-orders.md"
SEND_DELAY_SECONDS=2

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME        Tmux session to target
  --prompt-file PATH    File containing the marching orders
  --delay-seconds N     Delay between window broadcasts
  -h, --help            Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      SESSION_NAME="$2"
      shift 2
      ;;
    --prompt-file)
      PROMPT_FILE="$2"
      shift 2
      ;;
    --delay-seconds)
      SEND_DELAY_SECONDS="$2"
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

if [[ ! -f "$PROMPT_FILE" ]]; then
  echo "Prompt file not found: $PROMPT_FILE" >&2
  exit 1
fi

tmux load-buffer "$PROMPT_FILE"

while IFS=' ' read -r window_index window_name; do
  case "$window_name" in
    control)
      continue
      ;;
  esac

  tmux paste-buffer -t "${SESSION_NAME}:${window_index}"
  tmux send-keys -t "${SESSION_NAME}:${window_index}" Enter
  echo "Broadcast marching orders to ${SESSION_NAME}:${window_name}"
  sleep "$SEND_DELAY_SECONDS"
done < <(tmux list-windows -t "$SESSION_NAME" -F '#{window_index} #{window_name}')
