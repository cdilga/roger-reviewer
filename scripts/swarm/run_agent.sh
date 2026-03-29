#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: $(basename "$0") <tool> <prompt-file>" >&2
  exit 1
fi

TOOL="$1"
PROMPT_FILE="$2"
SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)

if [[ ! -f "$PROMPT_FILE" ]]; then
  echo "Prompt file not found: $PROMPT_FILE" >&2
  exit 1
fi

cd "$PROJECT_ROOT"
PROMPT=$(cat "$PROMPT_FILE")

case "$TOOL" in
  codex)
    exec codex --dangerously-bypass-approvals-and-sandbox "$PROMPT"
    ;;
  claude)
    exec claude --dangerously-skip-permissions "$PROMPT"
    ;;
  gemini)
    exec gemini --yolo --prompt-interactive "$PROMPT"
    ;;
  *)
    echo "Unsupported tool: $TOOL" >&2
    exit 1
    ;;
esac
