#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"
UPSTREAM_NTM="${NTM_UPSTREAM_BIN:-${HOME}/.local/lib/acfs/bin/ntm}"

SESSION_NAME="${DEFAULT_SESSION_NAME}"
LINES=20

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME      Swarm session name
  --lines N           Lines of recent control-plane output to show
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

printf 'Main session: %s\n' "$SESSION_NAME"
if tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  "$UPSTREAM_NTM" status "$SESSION_NAME" || true
else
  printf '  not running\n'
fi

for suffix in control-plane controller health ft; do
  session="${SESSION_NAME}-${suffix}"
  printf '\n[%s]\n' "$session"
  if tmux has-session -t "$session" 2>/dev/null; then
    tmux capture-pane -p -t "${session}:0" | tail -n "$LINES"
  else
    printf 'not running\n'
  fi
done
