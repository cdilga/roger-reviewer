#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SESSION_NAME="${CI_FAILURE_WATCH_SESSION:-ci-failure-watch}"
WATCH_SCRIPT="${ROOT_DIR}/scripts/swarm/watch_ci_failures.sh"
RESTART=0
STATUS_ONLY=0

usage() {
  cat <<'EOF'
Usage:
  ensure_ci_failure_watch.sh [options]

Options:
  --session NAME   tmux session name to manage (default: ci-failure-watch)
  --restart        restart the watcher session if it already exists
  --status         report whether the watcher session is running and exit
  -h, --help       show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      SESSION_NAME="${2:-}"
      shift 2
      ;;
    --restart)
      RESTART=1
      shift
      ;;
    --status)
      STATUS_ONLY=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ ! -x "${WATCH_SCRIPT}" ]]; then
  echo "ci-failure-watch: missing watcher script at ${WATCH_SCRIPT}" >&2
  exit 1
fi

if tmux has-session -t "${SESSION_NAME}" 2>/dev/null; then
  if [[ "${STATUS_ONLY}" == "1" ]]; then
    echo "ci-failure-watch: running (${SESSION_NAME})"
    exit 0
  fi
  if [[ "${RESTART}" == "1" ]]; then
    tmux kill-session -t "${SESSION_NAME}" >/dev/null 2>&1 || true
  else
    echo "ci-failure-watch: already running (${SESSION_NAME})"
    exit 0
  fi
elif [[ "${STATUS_ONLY}" == "1" ]]; then
  echo "ci-failure-watch: not running (${SESSION_NAME})"
  exit 1
fi

tmux new-session \
  -d \
  -s "${SESSION_NAME}" \
  -n watch \
  "cd '${ROOT_DIR}' && exec '${WATCH_SCRIPT}'"

echo "ci-failure-watch: started (${SESSION_NAME})"
