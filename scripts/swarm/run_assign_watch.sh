#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd -P)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"

SESSION_NAME="${DEFAULT_SESSION_NAME}"
STRATEGY="dependency"
WATCH_INTERVAL="10s"
DELAY="2s"
STOP_WHEN_DONE=0
CHECK_ONLY=0
EXTRA_ARGS=()

usage() {
  cat <<EOF
Usage: $(basename "$0") [options] [-- <extra ntm assign flags>]

Run the canonical continuous NTM assignment loop for this repo.

This is the missing control-plane companion to plain \`ntm send\`: it keeps
watching for idle agents and newly unblocked beads, then reassigns work.

Defaults:
  session        ${SESSION_NAME}
  strategy       ${STRATEGY}
  watch-interval ${WATCH_INTERVAL}
  delay          ${DELAY}

Examples:
  $(basename "$0")
  $(basename "$0") --session roger-reviewer --watch-interval 5s
  $(basename "$0") --check
  $(basename "$0") --stop-when-done
  $(basename "$0") -- --cod-only --limit 4

Notes:
  - Use \`--check\` to observe repo-scoped \`ntm status\` + \`ntm activity\`
    without mutating assignments.
  - This does not restart crashed panes by itself. Pair it with
    \`ntm spawn ... --auto-restart\` for new swarms, or use
    \`ntm respawn <session>\` if a live session has dead panes.
  - \`--dry-run\` is intentionally rejected here because the current external
    NTM build can still mutate assignments during the initial pass.
  - Any flags after \`--\` are forwarded directly to \`ntm assign\`, except
    \`--dry-run\`.
EOF
}

json_get_field() {
  local field="$1"
  python3 -c 'import json,sys; payload=json.load(sys.stdin); value=payload.get(sys.argv[1], ""); print("" if value is None else value)' "$field"
}

status_json() {
  ntm status "$SESSION_NAME" --json
}

activity_json() {
  ntm activity "$SESSION_NAME" --json
}

require_repo_scoped_session() {
  local status_payload="$1"
  local session_working_directory

  session_working_directory="$(printf '%s' "$status_payload" | json_get_field working_directory)"
  if [[ -z "$session_working_directory" ]]; then
    echo "Unable to determine working_directory from 'ntm status ${SESSION_NAME} --json'." >&2
    exit 1
  fi

  if [[ "$session_working_directory" != "$PROJECT_ROOT" ]]; then
    echo "Refusing to run assign-watch for session '$SESSION_NAME': expected working_directory '$PROJECT_ROOT' but ntm reported '$session_working_directory'." >&2
    echo "Use '--check' to inspect the session safely, or target a session that resolves to this repo." >&2
    exit 1
  fi
}

reject_unsafe_args() {
  local arg
  for arg in "${EXTRA_ARGS[@]}"; do
    case "$arg" in
      --dry-run)
        echo "Refusing to forward '--dry-run' to 'ntm assign': the current external NTM build can still mutate assignments." >&2
        echo "Use '$(basename "$0") --check' for safe observation, or omit '--dry-run' for a real watch loop." >&2
        exit 1
        ;;
    esac
  done
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      SESSION_NAME="$2"
      shift 2
      ;;
    --strategy)
      STRATEGY="$2"
      shift 2
      ;;
    --watch-interval)
      WATCH_INTERVAL="$2"
      shift 2
      ;;
    --delay)
      DELAY="$2"
      shift 2
      ;;
    --check)
      CHECK_ONLY=1
      shift
      ;;
    --stop-when-done)
      STOP_WHEN_DONE=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      EXTRA_ARGS+=("$@")
      break
      ;;
    *)
      EXTRA_ARGS+=("$1")
      shift
      ;;
  esac
done

if ! command -v ntm >/dev/null 2>&1; then
  echo "ntm is not installed or not on PATH." >&2
  exit 1
fi

if ! tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  echo "Tmux session '$SESSION_NAME' does not exist." >&2
  exit 1
fi

status_payload="$(status_json)"
require_repo_scoped_session "$status_payload"

if [[ "$CHECK_ONLY" -eq 1 ]]; then
  activity_payload="$(activity_json)"
  printf 'Session scope check passed for %s\n' "$SESSION_NAME"
  printf 'Project root: %s\n' "$PROJECT_ROOT"
  printf 'Status JSON:\n%s\n' "$status_payload"
  printf 'Activity JSON:\n%s\n' "$activity_payload"
  exit 0
fi

reject_unsafe_args

cmd=(
  ntm assign "$SESSION_NAME"
  --watch
  --auto
  --strategy "$STRATEGY"
  --watch-interval "$WATCH_INTERVAL"
  --delay "$DELAY"
)

if [[ "$STOP_WHEN_DONE" -eq 1 ]]; then
  cmd+=(--stop-when-done)
fi

cmd+=("${EXTRA_ARGS[@]}")

printf 'Starting continuous assignment loop:'
printf ' %q' "${cmd[@]}"
printf '\n'

exec "${cmd[@]}"
