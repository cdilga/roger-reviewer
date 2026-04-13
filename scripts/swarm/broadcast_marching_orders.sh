#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"
PROMPT_BUILDER="${PROJECT_ROOT}/scripts/swarm/build_prompt.sh"

SESSION_NAME="${DEFAULT_SESSION_NAME}"
PROMPT_FILE="${PROJECT_ROOT}/docs/swarm/overnight-marching-orders.md"
PROMPT_DIR="${TMPDIR:-/tmp}/roger-reviewer-swarm-prompts"
SEND_DELAY_SECONDS=2
WORK_LANE=""

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME        Tmux session to target
  --prompt-file PATH    File containing the marching orders
  --lane NAME           Prompt lane override: implementation or maintenance
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
    --lane)
      WORK_LANE="$2"
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

if [[ ! -x "$PROMPT_BUILDER" ]]; then
  echo "Prompt builder not found or not executable: $PROMPT_BUILDER" >&2
  exit 1
fi

if [[ -n "$WORK_LANE" ]]; then
  case "$WORK_LANE" in
    implementation|maintenance)
      ;;
    *)
      echo "Invalid --lane value '$WORK_LANE' (expected implementation or maintenance)." >&2
      exit 1
      ;;
  esac
fi

mkdir -p "${PROMPT_DIR}/${SESSION_NAME}"

while IFS='|' read -r pane_index pane_title; do
  [[ -n "$pane_index" ]] || continue
  case "$pane_title" in
    *__cod_*|*__cc_*|*__gmi_*)
      ;;
    *)
      continue
      ;;
  esac

  prompt_out="${PROMPT_DIR}/${SESSION_NAME}/pane-${pane_index}.txt"
  if [[ -n "$WORK_LANE" ]]; then
    "$PROMPT_BUILDER" "$PROMPT_FILE" "$prompt_out" "$WORK_LANE"
  else
    "$PROMPT_BUILDER" "$PROMPT_FILE" "$prompt_out"
  fi

  ntm send "$SESSION_NAME" --pane="$pane_index" --file "$prompt_out" --no-cass-check >/dev/null
  echo "Broadcast built marching orders to ${SESSION_NAME} pane ${pane_index} (${pane_title})"
  sleep "$SEND_DELAY_SECONDS"
done < <(tmux list-panes -t "$SESSION_NAME" -F '#{pane_index}|#{pane_title}' | sort -n -t'|' -k1,1)
