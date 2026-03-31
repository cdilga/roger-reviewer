#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"

SESSION_NAME="${DEFAULT_SESSION_NAME}"

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME      Swarm session name
  -h, --help          Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      SESSION_NAME="$2"
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

for suffix in control-plane controller health ft; do
  session="${SESSION_NAME}-${suffix}"
  if tmux has-session -t "$session" 2>/dev/null; then
    tmux kill-session -t "$session"
    echo "Stopped $session"
  fi
done
