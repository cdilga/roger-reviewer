#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"

SESSION_NAME="${DEFAULT_SESSION_NAME}-swarm"
GRID_WINDOW_NAME="grid"
ATTACH=1

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME        Swarm session to reshape
  --window-name NAME    Name for the live grid window
  --no-attach           Reshape the session without attaching
  -h, --help            Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      SESSION_NAME="$2"
      shift 2
      ;;
    --window-name)
      GRID_WINDOW_NAME="$2"
      shift 2
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

if ! tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  echo "Tmux session '$SESSION_NAME' does not exist." >&2
  exit 1
fi

WINDOW_LINES=()
while IFS= read -r line; do
  [[ -n "$line" ]] || continue
  WINDOW_LINES+=("$line")
done < <(tmux list-windows -t "$SESSION_NAME" -F '#{window_index} #{window_name} #{window_panes}')

if [[ "${#WINDOW_LINES[@]}" -eq 0 ]]; then
  echo "No windows found in '$SESSION_NAME'." >&2
  exit 1
fi

CONTROL_INDEX=""
GRID_INDEX=""
WORKER_LINES=()

for line in "${WINDOW_LINES[@]}"; do
  window_index=${line%% *}
  rest=${line#* }
  window_name=${rest%% *}

  case "$window_name" in
    control)
      CONTROL_INDEX="$window_index"
      ;;
    supervisor)
      ;;
    "$GRID_WINDOW_NAME")
      GRID_INDEX="$window_index"
      ;;
    *)
      WORKER_LINES+=("$line")
      ;;
  esac
done

if [[ "${#WORKER_LINES[@]}" -eq 0 && -z "$GRID_INDEX" ]]; then
  echo "No worker windows found in '$SESSION_NAME'." >&2
  exit 1
fi

if [[ -z "$GRID_INDEX" ]]; then
  anchor_line="${WORKER_LINES[0]}"
  anchor_index=${anchor_line%% *}
  anchor_rest=${anchor_line#* }
  anchor_name=${anchor_rest%% *}
  anchor_target="${SESSION_NAME}:${anchor_index}.0"

  tmux select-pane -t "$anchor_target" -T "$anchor_name"
  tmux rename-window -t "${SESSION_NAME}:${anchor_index}" "$GRID_WINDOW_NAME"
  tmux setw -t "${SESSION_NAME}:${GRID_WINDOW_NAME}" window-size manual
  tmux resize-window -t "${SESSION_NAME}:${GRID_WINDOW_NAME}" -x 240 -y 70
  start_index=1
else
  tmux setw -t "${SESSION_NAME}:${GRID_WINDOW_NAME}" window-size manual
  tmux resize-window -t "${SESSION_NAME}:${GRID_WINDOW_NAME}" -x 240 -y 70
  start_index=0
fi

i=$start_index
while [[ $i -lt ${#WORKER_LINES[@]} ]]; do
  line="${WORKER_LINES[$i]}"
  source_index=${line%% *}
  source_rest=${line#* }
  source_name=${source_rest%% *}
  source_target="${SESSION_NAME}:${source_index}.0"

  tmux select-pane -t "$source_target" -T "$source_name"
  tmux join-pane -d -s "$source_target" -t "${SESSION_NAME}:${GRID_WINDOW_NAME}"
  i=$((i + 1))
done

tmux select-layout -t "${SESSION_NAME}:${GRID_WINDOW_NAME}" tiled >/dev/null
tmux setw -t "${SESSION_NAME}:${GRID_WINDOW_NAME}" window-size latest
tmux setw -t "${SESSION_NAME}:${GRID_WINDOW_NAME}" pane-border-status top
tmux setw -t "${SESSION_NAME}:${GRID_WINDOW_NAME}" pane-border-format '#{pane_title}'
tmux set-option -t "$SESSION_NAME" remain-on-exit on >/dev/null

if [[ -n "$CONTROL_INDEX" ]]; then
  tmux move-window -r -s "${SESSION_NAME}:${CONTROL_INDEX}" -t "${SESSION_NAME}:0" >/dev/null 2>&1 || true
fi

tmux select-window -t "${SESSION_NAME}:${GRID_WINDOW_NAME}"

if tmux has-session -t "${SESSION_NAME}-watch" 2>/dev/null; then
  tmux kill-session -t "${SESSION_NAME}-watch" >/dev/null 2>&1 || true
fi

if (( ATTACH == 1 )); then
  exec tmux attach -t "$SESSION_NAME"
fi

echo "Live grid ready in ${SESSION_NAME}:${GRID_WINDOW_NAME}"
