#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"
UPSTREAM_NTM="${NTM_UPSTREAM_BIN:-${HOME}/.local/lib/acfs/bin/ntm}"

SESSION_NAME="${DEFAULT_SESSION_NAME}"
MODE="assign"
START_FT=1
ASSIGN_SESSION=""
CONTROLLER_SESSION=""
FT_SESSION=""
HEALTH_SESSION=""

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME      Swarm session name
  --mode MODE         Control mode: assign (default) or nudge
  --no-ft             Do not start Frankenterm observation
  -h, --help          Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      SESSION_NAME="$2"
      shift 2
      ;;
    --mode)
      MODE="$2"
      shift 2
      ;;
    --no-ft)
      START_FT=0
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

case "$MODE" in
  nudge|assign)
    ;;
  *)
    echo "Invalid --mode '$MODE' (expected nudge or assign)." >&2
    exit 1
    ;;
esac

if [[ ! -x "$UPSTREAM_NTM" ]]; then
  echo "Upstream NTM binary not found at: $UPSTREAM_NTM" >&2
  exit 1
fi

if ! tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  echo "Tmux session '$SESSION_NAME' does not exist." >&2
  exit 1
fi

ASSIGN_SESSION="${SESSION_NAME}-control-plane"
CONTROLLER_SESSION="${SESSION_NAME}-controller"
FT_SESSION="${SESSION_NAME}-ft"
HEALTH_SESSION="${SESSION_NAME}-health"

if tmux has-session -t "$ASSIGN_SESSION" 2>/dev/null; then
  tmux kill-session -t "$ASSIGN_SESSION"
fi
if tmux has-session -t "$CONTROLLER_SESSION" 2>/dev/null; then
  tmux kill-session -t "$CONTROLLER_SESSION"
fi

case "$MODE" in
  nudge)
    tmux new-session -d -s "$CONTROLLER_SESSION" /bin/bash -lc \
      "cd \"$PROJECT_ROOT\" && exec \"$PROJECT_ROOT/scripts/swarm/supervise_swarm.sh\" --session \"$SESSION_NAME\""
    ;;
  assign)
    tmux new-session -d -s "$ASSIGN_SESSION" /bin/bash -lc \
      "cd \"$PROJECT_ROOT\" && exec \"$UPSTREAM_NTM\" assign \"$SESSION_NAME\" --watch --auto --strategy dependency --watch-interval 20s --delay 3s --reserve-files"
    tmux new-session -d -s "$CONTROLLER_SESSION" /bin/bash -lc \
      "cd \"$PROJECT_ROOT\" && exec \"$PROJECT_ROOT/scripts/swarm/supervise_swarm.sh\" --session \"$SESSION_NAME\""
    ;;
esac

if tmux has-session -t "$HEALTH_SESSION" 2>/dev/null; then
  tmux kill-session -t "$HEALTH_SESSION"
fi
tmux new-session -d -s "$HEALTH_SESSION" /bin/bash -lc \
  "cd \"$PROJECT_ROOT\" && while true; do \"$UPSTREAM_NTM\" health \"$SESSION_NAME\" --auto-restart-stuck --threshold 8m >/dev/null 2>&1 || true; sleep 30; done"

if (( START_FT == 1 )) && command -v ft >/dev/null 2>&1; then
  if tmux has-session -t "$FT_SESSION" 2>/dev/null; then
    tmux kill-session -t "$FT_SESSION"
  fi
  tmux new-session -d -s "$FT_SESSION" /bin/bash -lc "cd \"$PROJECT_ROOT\" && exec ft watch --foreground"
fi

echo "Started control plane for $SESSION_NAME"
echo "  mode: $MODE"
if [[ "$MODE" == "assign" ]]; then
  echo "  assign-watch: $ASSIGN_SESSION"
fi
echo "  controller: $CONTROLLER_SESSION"
echo "  health: $HEALTH_SESSION"
if (( START_FT == 1 )) && command -v ft >/dev/null 2>&1; then
  echo "  frankenterm: $FT_SESSION"
fi
