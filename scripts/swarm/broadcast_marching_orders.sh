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

detect_agent_type() {
  local pane_title="${1:-}"
  local pane_command="${2:-}"
  local normalized_command

  normalized_command="$(printf '%s' "$pane_command" | tr '[:upper:]' '[:lower:]')"

  case "$normalized_command" in
    codex*)
      echo "codex"
      return 0
      ;;
    claude*)
      echo "claude"
      return 0
      ;;
    gemini*)
      echo "gemini"
      return 0
      ;;
    opencode*)
      echo "opencode"
      return 0
      ;;
  esac

  case "$pane_title" in
    *__cod_*)
      echo "codex"
      return 0
      ;;
    *__cc_*)
      echo "claude"
      return 0
      ;;
    *__gmi_*)
      echo "gemini"
      return 0
      ;;
    *__oco_*|*__ope_*)
      echo "opencode"
      return 0
      ;;
  esac

  return 1
}

list_target_panes_via_ntm() {
  ntm status "$SESSION_NAME" --json 2>/dev/null | python3 -c '
import json
import sys

supported = {"codex", "claude", "gemini", "opencode"}

try:
    payload = json.load(sys.stdin)
except json.JSONDecodeError:
    sys.exit(1)

rows = []
for pane in payload.get("panes", []):
    pane_type = str(pane.get("type", "")).strip().lower()
    if pane_type not in supported:
        continue
    index = pane.get("index")
    if index is None:
        continue
    rows.append(
        (
            int(index),
            str(pane.get("title", "")),
            str(pane.get("command", "")),
            pane_type,
        )
    )

for index, title, command, pane_type in sorted(rows, key=lambda row: row[0]):
    print(f"{index}|{title}|{command}|{pane_type}")
'
}

list_target_panes_via_tmux() {
  while IFS='|' read -r pane_index pane_title pane_command; do
    [[ -n "$pane_index" ]] || continue
    if pane_type=$(detect_agent_type "$pane_title" "$pane_command"); then
      printf '%s|%s|%s|%s\n' "$pane_index" "$pane_title" "$pane_command" "$pane_type"
    fi
  done < <(tmux list-panes -t "$SESSION_NAME" -F '#{pane_index}|#{pane_title}|#{pane_current_command}')
}

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

pane_rows=""
if ! pane_rows="$(list_target_panes_via_ntm)"; then
  pane_rows=""
fi
if [[ -z "$pane_rows" ]]; then
  pane_rows="$(list_target_panes_via_tmux | sort -n -t'|' -k1,1)"
fi

matched_panes=0
while IFS='|' read -r pane_index pane_title pane_command pane_type; do
  [[ -n "${pane_index:-}" ]] || continue
  matched_panes=$((matched_panes + 1))

  prompt_out="${PROMPT_DIR}/${SESSION_NAME}/pane-${pane_index}.txt"
  if [[ -n "$WORK_LANE" ]]; then
    "$PROMPT_BUILDER" "$PROMPT_FILE" "$prompt_out" "$WORK_LANE"
  else
    "$PROMPT_BUILDER" "$PROMPT_FILE" "$prompt_out"
  fi

  ntm send "$SESSION_NAME" --pane="$pane_index" --file "$prompt_out" --no-cass-check >/dev/null
  pane_label="${pane_title:-$pane_command}"
  echo "Broadcast built marching orders to ${SESSION_NAME} pane ${pane_index} (${pane_type}: ${pane_label})"
  sleep "$SEND_DELAY_SECONDS"
done <<<"$pane_rows"

if [[ "$matched_panes" -eq 0 ]]; then
  echo "No supported agent panes detected in session '$SESSION_NAME'." >&2
  exit 1
fi
