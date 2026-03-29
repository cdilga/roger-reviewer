#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 3 || $# -gt 4 ]]; then
  echo "Usage: $(basename "$0") <tool> <agent-name> <prompt-file> [window-name]" >&2
  exit 1
fi

TOOL="$1"
AGENT_NAME="$2"
PROMPT_FILE="$3"
WINDOW_NAME="${4:-$TOOL}"

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
STATE_ROOT="${TMPDIR:-/tmp}/roger-reviewer-swarm/${WINDOW_NAME}"
STOP_FILE="${STATE_ROOT}/STOP"
HEARTBEAT_FILE="${STATE_ROOT}/heartbeat"
LAST_STATUS_FILE="${STATE_ROOT}/last-status"
LAST_OUTPUT_FILE="${STATE_ROOT}/last-output.txt"
SLEEP_SECONDS="${SWARM_SLEEP_SECONDS:-20}"

if [[ ! -f "$PROMPT_FILE" ]]; then
  echo "Prompt file not found: $PROMPT_FILE" >&2
  exit 1
fi

mkdir -p "$STATE_ROOT"
cd "$PROJECT_ROOT"

BASE_PROMPT=$(cat "$PROMPT_FILE")

build_prompt() {
  local cycle="$1"
  cat <<EOF
${BASE_PROMPT}

Persistent swarm identity rules:
- Your Agent Mail identity for this swarm is exactly \`${AGENT_NAME}\`. Reuse that exact name. Do not invent a new identity.
- Register or refresh that exact Agent Mail name at the start of the cycle before doing other coordination work.
- Do not take a bead because a launcher hinted at one. Self-select from \`br ready\` and \`bv\`.
- This invocation is cycle ${cycle}. Work autonomously until you reach a durable checkpoint, then stop cleanly.
- Before stopping, make sure bead status, file reservations, and Agent Mail reflect reality. If you changed bead state or found a blocker, send a short Agent Mail update.
- When this cycle ends, exit instead of waiting at an interactive prompt. The launcher will invoke you again.
EOF
}

run_cycle() {
  local prompt="$1"

  case "$TOOL" in
    codex)
      codex exec \
        --dangerously-bypass-approvals-and-sandbox \
        --output-last-message "$LAST_OUTPUT_FILE" \
        "$prompt"
      ;;
    claude)
      claude \
        --dangerously-skip-permissions \
        -p \
        "$prompt" \
        >"$LAST_OUTPUT_FILE"
      cat "$LAST_OUTPUT_FILE"
      ;;
    gemini)
      gemini \
        --yolo \
        -p \
        "$prompt" \
        >"$LAST_OUTPUT_FILE"
      cat "$LAST_OUTPUT_FILE"
      ;;
    opencode)
      opencode run "$prompt" >"$LAST_OUTPUT_FILE"
      cat "$LAST_OUTPUT_FILE"
      ;;
    *)
      echo "Unsupported tool: $TOOL" >&2
      return 1
      ;;
  esac
}

cycle=0

while [[ ! -f "$STOP_FILE" ]]; do
  cycle=$((cycle + 1))
  date -Iseconds >"$HEARTBEAT_FILE"
  prompt=$(build_prompt "$cycle")

  printf '\n[%s] %s cycle %d starting for %s\n\n' "$(date -Iseconds)" "$TOOL" "$cycle" "$AGENT_NAME"

  if run_cycle "$prompt"; then
    printf 'ok %s\n' "$(date -Iseconds)" >"$LAST_STATUS_FILE"
  else
    status=$?
    printf 'error %d %s\n' "$status" "$(date -Iseconds)" >"$LAST_STATUS_FILE"
    printf '\n[%s] %s cycle %d failed with exit %d\n' "$(date -Iseconds)" "$TOOL" "$cycle" "$status" >&2
  fi

  date -Iseconds >"$HEARTBEAT_FILE"
  [[ -f "$STOP_FILE" ]] && break
  sleep "$SLEEP_SECONDS"
done

printf '[%s] %s loop stopped for %s\n' "$(date -Iseconds)" "$TOOL" "$AGENT_NAME"
