#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
BR_RESOLVER="${SCRIPT_DIR}/resolve_br.sh"
CONTROL_PLANE_ENSURE="${SCRIPT_DIR}/control_plane_ensure.sh"
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"
UPSTREAM_NTM="${NTM_UPSTREAM_BIN:-${HOME}/.local/lib/acfs/bin/ntm}"

SESSION_NAME="${DEFAULT_SESSION_NAME}"
LINES=20
BR_BIN=""
CONTROL_MODE="assign"

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME      Swarm session to inspect
  --lines N           Number of pane lines to capture
  --control-mode MODE Control mode for ensure check: assign (default) or nudge
  -h, --help          Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      SESSION_NAME="$2"
      shift 2
      ;;
    --lines)
      LINES="$2"
      shift 2
      ;;
    --control-mode)
      CONTROL_MODE="$2"
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

case "$CONTROL_MODE" in
  assign|nudge)
    ;;
  *)
    echo "Invalid --control-mode '$CONTROL_MODE' (expected assign or nudge)." >&2
    exit 1
    ;;
esac

if ! tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  echo "Tmux session '$SESSION_NAME' does not exist." >&2
  exit 1
fi

if [[ -x "$BR_RESOLVER" ]]; then
  BR_BIN="$("$BR_RESOLVER" --quiet --print-path 2>/dev/null || true)"
fi

"$UPSTREAM_NTM" status "$SESSION_NAME" || true

echo
echo "== Control-plane health =="
if [[ -x "$CONTROL_PLANE_ENSURE" ]]; then
  "$CONTROL_PLANE_ENSURE" --session "$SESSION_NAME" --mode "$CONTROL_MODE" --check || true
else
  echo "control-plane ensure helper is unavailable at $CONTROL_PLANE_ENSURE"
fi
echo "Auto-heal command:"
echo "  ${PROJECT_ROOT}/scripts/swarm/control_plane_ensure.sh --session ${SESSION_NAME} --mode ${CONTROL_MODE} --watch"

echo
echo "== Active in-progress beads =="
if [[ -n "$BR_BIN" ]]; then
  (
    cd "$PROJECT_ROOT"
    "$BR_BIN" list --status in_progress || true
  )
else
  echo "Pinned br path is unavailable; run scripts/swarm/resolve_br.sh."
fi

echo
while IFS='|' read -r pane_index pane_title; do
  [[ -n "$pane_index" ]] || continue
  echo "== ${SESSION_NAME} pane ${pane_index} :: ${pane_title} =="
  tmux capture-pane -p -S "-${LINES}" -t "${SESSION_NAME}:0.${pane_index}"
  echo
done < <(tmux list-panes -t "$SESSION_NAME" -F '#{pane_index}|#{pane_title}')
