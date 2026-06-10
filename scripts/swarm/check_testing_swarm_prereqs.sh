#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')-testing"

SESSION_NAME="${DEFAULT_SESSION_NAME}"
CODEX_COUNT=3
CLAUDE_COUNT=0
GEMINI_COUNT=1
OPENCODE_COUNT=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Validate prerequisites for a dedicated Roger testing swarm.

Options:
  --session NAME      Tmux session name to validate
  --codex N           Number of Codex agents to expect (default: ${CODEX_COUNT})
  --claude N          Number of Claude agents to expect
  --gemini N          Number of Gemini agents to expect (default: ${GEMINI_COUNT})
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

agent_mail_tmux_session_name() {
  if tmux has-session -t agent-mail 2>/dev/null; then
    printf 'agent-mail\n'
    return 0
  fi
  if tmux has-session -t mcp-agent-mail 2>/dev/null; then
    printf 'mcp-agent-mail\n'
    return 0
  fi
  return 1
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
require_command ntm
require_command git
require_command curl
require_command am
require_command rr

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

AGENT_MAIL_SESSION="$(agent_mail_tmux_session_name || true)"

if ! curl -fsS http://127.0.0.1:8765/health/readiness >/dev/null; then
  echo "Agent Mail is not ready on http://127.0.0.1:8765." >&2
  echo "Start or inspect it with: tmux attach -t ${AGENT_MAIL_SESSION:-agent-mail}" >&2
  exit 1
fi

echo "Project root: $PROJECT_ROOT"
echo "Testing swarm session: $SESSION_NAME"
echo "Planned mix: codex=$CODEX_COUNT claude=$CLAUDE_COUNT gemini=$GEMINI_COUNT opencode=$OPENCODE_COUNT"
echo "rr path: $(command -v rr)"
echo "rr version: $(rr --version 2>/dev/null || echo unavailable)"
echo "Agent Mail readiness: OK"
if command -v rch >/dev/null 2>&1; then
  echo "rch: $(rch --version 2>/dev/null || echo installed)"
else
  echo "rch: not installed"
fi
