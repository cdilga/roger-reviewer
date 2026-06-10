#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')-testing"
PREFLIGHT_SCRIPT="${SCRIPT_DIR}/check_testing_swarm_prereqs.sh"
BROADCAST_SCRIPT="${SCRIPT_DIR}/broadcast_testing_lane_orders.sh"
IDENTITY_COMPAT_SCRIPT="${SCRIPT_DIR}/sync_agentmail_identity_compat.sh"

SESSION_NAME="${DEFAULT_SESSION_NAME}"
CODEX_COUNT=3
CLAUDE_COUNT=0
GEMINI_COUNT=1
OPENCODE_COUNT=0
BROWSER_OWNER="codex"
SPAWN_MODE="spawn_if_missing"
WAIT_SECONDS=3

spawn_args=()

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Spawn or reseed a dedicated Roger testing swarm.

Options:
  --session NAME           Tmux session name (default: ${SESSION_NAME})
  --codex N                Codex pane count (default: ${CODEX_COUNT})
  --claude N               Claude pane count
  --gemini N               Gemini pane count (default: ${GEMINI_COUNT})
  --opencode N             OpenCode pane count
  --browser-owner VALUE    Browser-owner selector for the broadcast script
  --spawn-mode MODE        One of: spawn_if_missing, respawn, reuse_existing
  --wait-seconds N         Delay after spawn before broadcasting prompts
  -h, --help               Show this help
EOF
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
    --browser-owner)
      BROWSER_OWNER="$2"
      shift 2
      ;;
    --spawn-mode)
      SPAWN_MODE="$2"
      shift 2
      ;;
    --wait-seconds)
      WAIT_SECONDS="$2"
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

case "$SPAWN_MODE" in
  spawn_if_missing|respawn|reuse_existing)
    ;;
  *)
    echo "Invalid --spawn-mode '$SPAWN_MODE'." >&2
    exit 1
    ;;
esac

if [[ ! -f "$PREFLIGHT_SCRIPT" ]]; then
  echo "Missing preflight script: $PREFLIGHT_SCRIPT" >&2
  exit 1
fi

if [[ ! -f "$BROADCAST_SCRIPT" ]]; then
  echo "Missing broadcast script: $BROADCAST_SCRIPT" >&2
  exit 1
fi

if [[ ! -f "$IDENTITY_COMPAT_SCRIPT" ]]; then
  echo "Missing identity compatibility script: $IDENTITY_COMPAT_SCRIPT" >&2
  exit 1
fi

if (( CODEX_COUNT > 0 )); then
  spawn_args+=(--cod="$CODEX_COUNT")
fi
if (( CLAUDE_COUNT > 0 )); then
  spawn_args+=(--cc="$CLAUDE_COUNT")
fi
if (( GEMINI_COUNT > 0 )); then
  spawn_args+=(--gmi="$GEMINI_COUNT")
fi
if (( OPENCODE_COUNT > 0 )); then
  spawn_args+=(--oco="$OPENCODE_COUNT")
fi

if [[ "${#spawn_args[@]}" -eq 0 ]]; then
  echo "Refusing to launch a testing swarm with zero agent panes." >&2
  exit 1
fi

bash "$PREFLIGHT_SCRIPT" \
  --session "$SESSION_NAME" \
  --codex "$CODEX_COUNT" \
  --claude "$CLAUDE_COUNT" \
  --gemini "$GEMINI_COUNT" \
  --opencode "$OPENCODE_COUNT"

if tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  case "$SPAWN_MODE" in
    spawn_if_missing)
      echo "Session '$SESSION_NAME' already exists; reusing it and rebroadcasting testing lane orders."
      ;;
    respawn)
      echo "Respawning existing session '$SESSION_NAME'."
      ntm kill "$SESSION_NAME" --force
      ntm spawn "$SESSION_NAME" "${spawn_args[@]}" --no-user --auto-restart
      sleep "$WAIT_SECONDS"
      ;;
    reuse_existing)
      echo "Reusing existing session '$SESSION_NAME' without respawn."
      ;;
  esac
else
  echo "Spawning testing swarm '$SESSION_NAME'."
  ntm spawn "$SESSION_NAME" "${spawn_args[@]}" --no-user --auto-restart
  sleep "$WAIT_SECONDS"
fi

bash "$IDENTITY_COMPAT_SCRIPT" --project "$PROJECT_ROOT" >/dev/null

bash "$BROADCAST_SCRIPT" \
  --session "$SESSION_NAME" \
  --browser-owner "$BROWSER_OWNER" \
  --delay-seconds 0

cat <<EOF
Testing swarm ready.

Suggested next commands:
  ntm status "$SESSION_NAME"
  ntm activity "$SESSION_NAME" --watch
  bash ${BROADCAST_SCRIPT} --session "$SESSION_NAME" --browser-owner "$BROWSER_OWNER"
EOF
