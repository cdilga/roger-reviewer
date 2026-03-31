#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"

SESSION_NAME="${DEFAULT_SESSION_NAME}-swarm"
CODEX_COUNT=4
CLAUDE_COUNT=4
GEMINI_COUNT=2
OPENCODE_COUNT=2
DELAY_SECONDS=45
ATTACH=0
SUPERVISOR=1
PROMPT_FILE="${PROJECT_ROOT}/docs/swarm/overnight-marching-orders.md"
PROMPT_DIR="${TMPDIR:-/tmp}/roger-reviewer-swarm-prompts"
WORKER_NAMES=(
  AmberFalcon
  CobaltHarbor
  IvoryOtter
  PinkPeak
  SilverBadger
  CopperBrook
  RedStone
  HazySpring
  CrimsonOtter
  JadeRaven
  SableCreek
  FrostyWillow
  LilacSummit
  TopazGrove
  AzureMeadow
  BronzeBadger
)
NEXT_WORKER_INDEX=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME      Tmux session name to create
  --codex N           Number of Codex windows to spawn
  --claude N          Number of Claude windows to spawn
  --gemini N          Number of Gemini windows to spawn
  --opencode N        Number of OpenCode windows to spawn
  --prompt-file PATH  Prompt file used for every worker session
  --delay-seconds N   Delay between spawned windows
  --no-supervisor     Do not start the tmux swarm supervisor
  --attach            Attach to the session after launch
  --no-attach         Do not attach after launch
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

spawn_windows() {
  local tool="$1"
  local count="$2"
  local i

  for (( i = 1; i <= count; i++ )); do
    local agent_name
    local worker_prompt_file
    local window_name

    if (( NEXT_WORKER_INDEX >= ${#WORKER_NAMES[@]} )); then
      echo "Not enough predefined worker names for requested swarm size." >&2
      exit 1
    fi

    agent_name="${WORKER_NAMES[$NEXT_WORKER_INDEX]}"
    NEXT_WORKER_INDEX=$((NEXT_WORKER_INDEX + 1))
    window_name=$(printf "%s-%02d" "$tool" "$i")
    worker_prompt_file="${PROMPT_DIR}/${SESSION_NAME}/${window_name}.txt"
    bash "${PROJECT_ROOT}/scripts/swarm/build_prompt.sh" "$PROMPT_FILE" "$agent_name" "$worker_prompt_file"
    tmux new-window -t "$SESSION_NAME:" -n "$window_name" "cd '$PROJECT_ROOT' && exec '$PROJECT_ROOT/scripts/swarm/run_agent.sh' '$tool' '$worker_prompt_file'"
    echo "Spawned $window_name as $agent_name"
    sleep "$DELAY_SECONDS"
  done
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
    --prompt-file)
      PROMPT_FILE="$2"
      shift 2
      ;;
    --delay-seconds)
      DELAY_SECONDS="$2"
      shift 2
      ;;
    --no-supervisor)
      SUPERVISOR=0
      shift
      ;;
    --attach)
      ATTACH=1
      shift
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

require_command tmux
require_command curl
require_command am
if [[ ! -f "${PROJECT_ROOT}/scripts/swarm/build_prompt.sh" ]]; then
  echo "Missing required script: ${PROJECT_ROOT}/scripts/swarm/build_prompt.sh" >&2
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

if tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  echo "Tmux session '$SESSION_NAME' already exists." >&2
  echo "Choose a new --session name or remove the existing session first." >&2
  exit 1
fi

if [[ ! -f "$PROMPT_FILE" ]]; then
  echo "Prompt file not found: $PROMPT_FILE" >&2
  exit 1
fi

if ! curl -fsS http://127.0.0.1:8765/health/readiness >/dev/null; then
  echo "Agent Mail is not ready on http://127.0.0.1:8765." >&2
  echo "Start or inspect it with: tmux attach -t mcp-agent-mail" >&2
  exit 1
fi

tmux new-session -d -s "$SESSION_NAME" -n control "cd '$PROJECT_ROOT' && exec ${SHELL:-/bin/zsh} -l"
tmux set-option -t "$SESSION_NAME" remain-on-exit on
tmux send-keys -t "$SESSION_NAME:control" "cd '$PROJECT_ROOT'" C-m
tmux send-keys -t "$SESSION_NAME:control" "printf 'Swarm control shell for %s\\n' '$SESSION_NAME'" C-m

spawn_windows codex "$CODEX_COUNT"
spawn_windows claude "$CLAUDE_COUNT"
spawn_windows gemini "$GEMINI_COUNT"
spawn_windows opencode "$OPENCODE_COUNT"

if (( SUPERVISOR == 1 )); then
  tmux new-window -t "$SESSION_NAME:" -n supervisor "cd '$PROJECT_ROOT' && exec '$PROJECT_ROOT/scripts/swarm/supervise_swarm.sh' --session '$SESSION_NAME'"
fi

echo
echo "Swarm launched in tmux session: $SESSION_NAME"
echo "Project root: $PROJECT_ROOT"
echo "Next step: wait for the CLIs to settle, then run:"
echo "  ${PROJECT_ROOT}/scripts/swarm/broadcast_marching_orders.sh --session $SESSION_NAME"
echo
echo "Useful commands:"
echo "  tmux attach -t $SESSION_NAME"
echo "  ${PROJECT_ROOT}/scripts/swarm/status.sh --session $SESSION_NAME"

if (( ATTACH == 1 )); then
  exec tmux attach -t "$SESSION_NAME"
fi
