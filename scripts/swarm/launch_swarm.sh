#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
PREFLIGHT_SCRIPT="${PROJECT_ROOT}/scripts/swarm/preflight_swarm.sh"
PROMPT_BUILDER="${PROJECT_ROOT}/scripts/swarm/build_prompt.sh"
CONTROL_PLANE_UP="${PROJECT_ROOT}/scripts/swarm/control_plane_up.sh"
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"
UPSTREAM_NTM="${NTM_UPSTREAM_BIN:-${HOME}/.local/lib/acfs/bin/ntm}"

SESSION_NAME="${DEFAULT_SESSION_NAME}"
CODEX_COUNT=4
CLAUDE_COUNT=4
GEMINI_COUNT=2
OPENCODE_COUNT=0
DELAY_SECONDS=45
ATTACH=0
PREFLIGHT=1
CONTROL_PLANE=1
CONTROL_MODE="assign"
FT_WATCH="auto"
USER_PANE=1
PROMPT_FILE="${PROJECT_ROOT}/docs/swarm/overnight-marching-orders.md"
PROMPT_DIR="${TMPDIR:-/tmp}/roger-reviewer-swarm-prompts"
WORK_LANE="implementation"

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME          NTM/tmux session name to create
  --codex N               Number of Codex panes to spawn
  --claude N              Number of Claude panes to spawn
  --gemini N              Number of Gemini panes to spawn
  --opencode N            Number of OpenCode panes to spawn (not supported in native NTM mode)
  --prompt-file PATH      Base prompt file used to build per-pane marching orders
  --lane NAME             Worker lane: implementation (default) or maintenance
  --maintenance-lane      Shortcut for --lane maintenance
  --delay-seconds N       Delay between per-pane prompt sends
  --no-preflight          Skip swarm preflight checks
  --no-control-plane      Do not start the control-plane watcher sessions
  --no-supervisor         Backward-compatible alias for --no-control-plane
  --control-mode MODE     Control-plane mode: assign (default) or nudge
  --no-ft                 Do not start Frankenterm/WezTerm observation
  --no-user-pane          Spawn only agent panes (no reserved operator pane)
  --attach                Attach to the tiled NTM session after launch
  --no-attach             Do not attach after launch
  -h, --help              Show this help
EOF
}

require_command() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Missing required command: $cmd" >&2
    exit 1
  fi
}

count_for_type() {
  case "$1" in
    codex) printf '%s\n' "$CODEX_COUNT" ;;
    claude) printf '%s\n' "$CLAUDE_COUNT" ;;
    gemini) printf '%s\n' "$GEMINI_COUNT" ;;
    *) printf '0\n' ;;
  esac
}

agent_type_for_title() {
  case "$1" in
    *__cod_*) printf 'codex\n' ;;
    *__cc_*) printf 'claude\n' ;;
    *__gmi_*) printf 'gemini\n' ;;
    *) printf '\n' ;;
  esac
}

wait_for_agent_panes() {
  local expected="$1"
  local deadline now actual
  deadline=$(( $(date +%s) + 90 ))

  while true; do
    actual=$(tmux list-panes -t "$SESSION_NAME" -F '#{pane_title}' 2>/dev/null | \
      awk '/__(cod|cc|gmi)_/ { count += 1 } END { print count + 0 }')
    if [[ "$actual" -eq "$expected" ]]; then
      return 0
    fi

    now=$(date +%s)
    if (( now >= deadline )); then
      echo "Timed out waiting for $expected agent panes in $SESSION_NAME (saw $actual)." >&2
      exit 1
    fi
    sleep 2
  done
}

send_initial_prompts() {
  local pane_index pane_title agent_type prompt_out seeded_count
  local total_agents
  total_agents=$((CODEX_COUNT + CLAUDE_COUNT + GEMINI_COUNT))
  seeded_count=0
  mkdir -p "${PROMPT_DIR}/${SESSION_NAME}"

  while IFS='|' read -r pane_index pane_title; do
    [[ -n "$pane_index" ]] || continue
    agent_type=$(agent_type_for_title "$pane_title")
    [[ -n "$agent_type" ]] || continue

    prompt_out="${PROMPT_DIR}/${SESSION_NAME}/pane-${pane_index}.txt"
    "$PROMPT_BUILDER" "$PROMPT_FILE" "$prompt_out" "$WORK_LANE"

    "$UPSTREAM_NTM" send "$SESSION_NAME" --pane="$pane_index" --file "$prompt_out" --no-cass-check >/dev/null
    seeded_count=$((seeded_count + 1))
    echo "Seeded pane ${pane_index} (${agent_type})"
    sleep "$DELAY_SECONDS"
  done < <(tmux list-panes -t "$SESSION_NAME" -F '#{pane_index}|#{pane_title}' | sort -n -t'|' -k1,1)

  if (( seeded_count != total_agents )); then
    echo "Prompt seeding mismatch: expected $total_agents agents, seeded $seeded_count." >&2
    exit 1
  fi
}

spawn_with_upstream_ntm() {
  local args=("$UPSTREAM_NTM" spawn "$SESSION_NAME" --no-cass-context --no-recovery --auto-restart)

  if (( USER_PANE == 0 )); then
    args+=(--no-user)
  fi
  if (( CODEX_COUNT > 0 )); then
    args+=("--cod=${CODEX_COUNT}")
  fi
  if (( CLAUDE_COUNT > 0 )); then
    args+=("--cc=${CLAUDE_COUNT}")
  fi
  if (( GEMINI_COUNT > 0 )); then
    args+=("--gmi=${GEMINI_COUNT}")
  fi

  "${args[@]}"
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
    --lane)
      WORK_LANE="$2"
      shift 2
      ;;
    --maintenance-lane)
      WORK_LANE="maintenance"
      shift
      ;;
    --delay-seconds)
      DELAY_SECONDS="$2"
      shift 2
      ;;
    --no-preflight)
      PREFLIGHT=0
      shift
      ;;
    --no-control-plane|--no-supervisor)
      CONTROL_PLANE=0
      shift
      ;;
    --control-mode)
      CONTROL_MODE="$2"
      shift 2
      ;;
    --no-ft)
      FT_WATCH=0
      shift
      ;;
    --no-user-pane)
      USER_PANE=0
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

case "$WORK_LANE" in
  implementation|maintenance)
    ;;
  *)
    echo "Invalid --lane value '$WORK_LANE' (expected implementation or maintenance)." >&2
    exit 1
    ;;
esac

case "$CONTROL_MODE" in
  nudge|assign)
    ;;
  *)
    echo "Invalid --control-mode '$CONTROL_MODE' (expected nudge or assign)." >&2
    exit 1
    ;;
esac

if (( OPENCODE_COUNT > 0 )); then
  echo "OpenCode panes are not supported by the native NTM launcher path yet." >&2
  echo "Use Codex/Claude/Gemini here, or keep OpenCode in a separate manual session." >&2
  exit 1
fi

require_command tmux
require_command curl
require_command am
if [[ ! -x "$UPSTREAM_NTM" ]]; then
  echo "Upstream NTM binary not found at: $UPSTREAM_NTM" >&2
  exit 1
fi
if [[ ! -x "$PROMPT_BUILDER" ]]; then
  echo "Missing required script: $PROMPT_BUILDER" >&2
  exit 1
fi
if [[ ! -x "$CONTROL_PLANE_UP" ]]; then
  echo "Missing required script: $CONTROL_PLANE_UP" >&2
  exit 1
fi
if (( PREFLIGHT == 1 )) && [[ ! -x "$PREFLIGHT_SCRIPT" ]]; then
  echo "Missing required script: $PREFLIGHT_SCRIPT" >&2
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

if (( PREFLIGHT == 1 )); then
  echo "Running swarm preflight..."
  if ! "$PREFLIGHT_SCRIPT" \
    --session "$SESSION_NAME" \
    --codex "$CODEX_COUNT" \
    --claude "$CLAUDE_COUNT" \
    --gemini "$GEMINI_COUNT" \
    --opencode "$OPENCODE_COUNT"; then
    status=$?
    if [[ "$status" -eq 75 ]]; then
      echo "Swarm preflight returned a transient retry condition. Re-run launch after lock contention clears." >&2
    fi
    exit "$status"
  fi
fi

spawn_with_upstream_ntm
wait_for_agent_panes $((CODEX_COUNT + CLAUDE_COUNT + GEMINI_COUNT))
send_initial_prompts

if (( CONTROL_PLANE == 1 )); then
  control_args=(--session "$SESSION_NAME" --mode "$CONTROL_MODE")
  if [[ "$FT_WATCH" == "0" ]]; then
    control_args+=(--no-ft)
  fi
  "$CONTROL_PLANE_UP" "${control_args[@]}"
fi

echo
echo "Swarm launched in tmux session: $SESSION_NAME"
echo "Project root: $PROJECT_ROOT"
echo "Worker lane: $WORK_LANE"
echo "Control mode: $CONTROL_MODE"
echo
echo "Useful commands:"
echo "  ${UPSTREAM_NTM} view $SESSION_NAME"
echo "  ${PROJECT_ROOT}/scripts/swarm/control_plane_status.sh --session $SESSION_NAME"
echo "  ${PROJECT_ROOT}/scripts/swarm/status.sh --session $SESSION_NAME"

if (( ATTACH == 1 )); then
  exec "$UPSTREAM_NTM" view "$SESSION_NAME"
fi
