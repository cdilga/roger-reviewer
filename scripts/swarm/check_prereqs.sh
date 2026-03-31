#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
BR_RESOLVER="${SCRIPT_DIR}/resolve_br.sh"
PREFLIGHT_SCRIPT="${SCRIPT_DIR}/preflight_swarm.sh"
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"

SESSION_NAME="${DEFAULT_SESSION_NAME}"
CODEX_COUNT=4
CLAUDE_COUNT=4
GEMINI_COUNT=2
OPENCODE_COUNT=0
BR_BIN=""

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME      Tmux session name to validate
  --codex N           Number of Codex agents to expect
  --claude N          Number of Claude agents to expect
  --gemini N          Number of Gemini agents to expect
  --opencode N        Number of OpenCode agents to expect
  -h, --help          Show this help
EOF
}

require_command() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Missing required command: $cmd" >&2
    exit 1
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      SESSION_NAME="$2"
      shift 2
      ;;
    --codex)
      CODEX_COUNT="$2"
      shift 2
      ;;
    --claude)
      CLAUDE_COUNT="$2"
      shift 2
      ;;
    --gemini)
      GEMINI_COUNT="$2"
      shift 2
      ;;
    --opencode)
      OPENCODE_COUNT="$2"
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

require_command tmux
require_command bv
require_command curl
require_command am
if [[ ! -x "$PREFLIGHT_SCRIPT" ]]; then
  echo "Missing required script: $PREFLIGHT_SCRIPT" >&2
  exit 1
fi

echo "swarm preflight:"
"$PREFLIGHT_SCRIPT" \
  --session "$SESSION_NAME" \
  --codex "$CODEX_COUNT" \
  --claude "$CLAUDE_COUNT" \
  --gemini "$GEMINI_COUNT" \
  --opencode "$OPENCODE_COUNT"
echo

if [[ ! -x "$BR_RESOLVER" ]]; then
  echo "Missing required script: $BR_RESOLVER" >&2
  exit 1
fi
if ! BR_BIN="$("$BR_RESOLVER" --quiet --print-path)"; then
  echo "Unable to resolve pinned br path for this workspace." >&2
  exit 1
fi

if (( CODEX_COUNT > 0 )); then
  require_command codex
fi
if (( CLAUDE_COUNT > 0 )); then
  require_command claude
fi
if (( GEMINI_COUNT > 0 )); then
  require_command gemini
fi
if (( OPENCODE_COUNT > 0 )); then
  require_command opencode
fi

if ! curl -fsS http://127.0.0.1:8765/health/readiness >/dev/null; then
  echo "Agent Mail is not ready on http://127.0.0.1:8765." >&2
  echo "Start or inspect it with: tmux attach -t mcp-agent-mail" >&2
  exit 1
fi

echo "Project root: $PROJECT_ROOT"
echo "Swarm session: $SESSION_NAME"
echo "Planned mix: codex=$CODEX_COUNT claude=$CLAUDE_COUNT gemini=$GEMINI_COUNT opencode=$OPENCODE_COUNT"
echo "Pinned br path: $BR_BIN"
echo "Pinned br version: $("$BR_BIN" --version)"
echo

echo "Agent Mail readiness: OK"

if tmux has-session -t mcp-agent-mail 2>/dev/null; then
  echo "Agent Mail tmux session: mcp-agent-mail"
else
  echo "Agent Mail tmux session: not found (am still exists if you need to restart it)"
fi

echo
echo "br doctor:"
"$BR_BIN" doctor

echo
echo "br ready:"
"$BR_BIN" ready

echo
echo "bv --robot-next:"
bv --robot-next
