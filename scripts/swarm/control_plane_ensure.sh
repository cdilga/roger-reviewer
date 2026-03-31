#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"
CONTROL_PLANE_UP="${SCRIPT_DIR}/control_plane_up.sh"

SESSION_NAME="${DEFAULT_SESSION_NAME}"
MODE="assign"
CHECK_ONLY=0
WATCH=0
INTERVAL_SECONDS=20
START_FT=1
DRY_RUN=0
JSON=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME         Swarm session name
  --mode MODE            Control mode: assign (default) or nudge
  --check                Verify control-plane sessions without starting anything
  --watch                Re-check continuously and auto-heal missing sessions
  --interval-seconds N   Poll interval for --watch mode (default: 20)
  --no-ft                Disable Frankenterm when auto-starting control plane
  --dry-run              Print start command instead of executing it
  --json                 Emit machine-readable JSON output
  -h, --help             Show this help
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
    --check)
      CHECK_ONLY=1
      shift
      ;;
    --watch)
      WATCH=1
      shift
      ;;
    --interval-seconds)
      INTERVAL_SECONDS="$2"
      shift 2
      ;;
    --no-ft)
      START_FT=0
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --json)
      JSON=1
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
  assign|nudge)
    ;;
  *)
    echo "Invalid --mode '$MODE' (expected assign or nudge)." >&2
    exit 1
    ;;
esac

if [[ "$WATCH" == "1" && "$CHECK_ONLY" == "1" ]]; then
  echo "Cannot combine --watch with --check." >&2
  exit 1
fi

if [[ ! -x "$CONTROL_PLANE_UP" ]]; then
  echo "Missing required script: $CONTROL_PLANE_UP" >&2
  exit 1
fi

if ! command -v tmux >/dev/null 2>&1; then
  echo "Missing required command: tmux" >&2
  exit 1
fi

required_suffixes() {
  if [[ "$MODE" == "assign" ]]; then
    printf '%s\n' control-plane controller health
  else
    printf '%s\n' controller health
  fi
}

session_exists() {
  local session="$1"
  tmux has-session -t "$session" 2>/dev/null
}

controller_signal() {
  local controller_session="${SESSION_NAME}-controller"
  if ! session_exists "$controller_session"; then
    printf 'unavailable\n'
    return 0
  fi

  local capture
  capture="$(tmux capture-pane -p -t "${controller_session}:0" 2>/dev/null || true)"
  if printf '%s\n' "$capture" | rg -qi "nudged pane"; then
    printf 'reclaiming\n'
    return 0
  fi
  if printf '%s\n' "$capture" | rg -qi "exhausted-queue|genuinely exhausted|hold"; then
    printf 'exhausted\n'
    return 0
  fi
  printf 'idle-or-unknown\n'
}

report_state() {
  local status="$1"
  local missing_csv="$2"
  local signal="$3"

  if [[ "$JSON" == "1" ]]; then
    jq -n \
      --arg session "$SESSION_NAME" \
      --arg mode "$MODE" \
      --arg status "$status" \
      --arg missing "$missing_csv" \
      --arg signal "$signal" \
      '{
        session: $session,
        mode: $mode,
        status: $status,
        missing_sessions: (if $missing == "" then [] else ($missing | split(",")) end),
        controller_signal: $signal
      }'
    return 0
  fi

  printf 'session=%s mode=%s status=%s controller_signal=%s\n' \
    "$SESSION_NAME" "$MODE" "$status" "$signal"
  if [[ -n "$missing_csv" ]]; then
    printf 'missing_sessions=%s\n' "$missing_csv"
  fi
}

ensure_once() {
  if ! session_exists "$SESSION_NAME"; then
    if [[ "$JSON" == "1" ]]; then
      jq -n --arg session "$SESSION_NAME" '{session: $session, status: "main-session-missing"}'
    else
      echo "Main swarm session '$SESSION_NAME' does not exist."
    fi
    return 1
  fi

  local missing=()
  local suffix
  while IFS= read -r suffix; do
    [[ -n "$suffix" ]] || continue
    local cp_session="${SESSION_NAME}-${suffix}"
    if ! session_exists "$cp_session"; then
      missing+=("$cp_session")
    fi
  done < <(required_suffixes)

  local signal
  signal="$(controller_signal)"
  if [[ "${#missing[@]}" -eq 0 ]]; then
    report_state "healthy" "" "$signal"
    return 0
  fi

  local missing_csv
  missing_csv="$(IFS=,; echo "${missing[*]}")"
  if [[ "$CHECK_ONLY" == "1" ]]; then
    report_state "missing" "$missing_csv" "$signal"
    return 1
  fi

  local start_cmd=("$CONTROL_PLANE_UP" --session "$SESSION_NAME" --mode "$MODE")
  if [[ "$START_FT" != "1" ]]; then
    start_cmd+=(--no-ft)
  fi

  if [[ "$DRY_RUN" == "1" ]]; then
    if [[ "$JSON" == "1" ]]; then
      jq -n \
        --arg session "$SESSION_NAME" \
        --arg mode "$MODE" \
        --arg missing "$missing_csv" \
        --arg cmd "${start_cmd[*]}" \
        '{session: $session, mode: $mode, status: "would-start", missing_sessions: ($missing | split(",")), command: $cmd}'
    else
      printf 'status=would-start session=%s mode=%s missing_sessions=%s\n' "$SESSION_NAME" "$MODE" "$missing_csv"
      printf 'command=%s\n' "${start_cmd[*]}"
    fi
    return 0
  fi

  if [[ "$JSON" == "1" ]]; then
    "${start_cmd[@]}" >/dev/null
  else
    "${start_cmd[@]}"
  fi
  sleep 1

  local post_missing=()
  while IFS= read -r suffix; do
    [[ -n "$suffix" ]] || continue
    local cp_session="${SESSION_NAME}-${suffix}"
    if ! session_exists "$cp_session"; then
      post_missing+=("$cp_session")
    fi
  done < <(required_suffixes)

  signal="$(controller_signal)"
  if [[ "${#post_missing[@]}" -eq 0 ]]; then
    report_state "started" "" "$signal"
    return 0
  fi

  local post_missing_csv
  post_missing_csv="$(IFS=,; echo "${post_missing[*]}")"
  report_state "start-failed" "$post_missing_csv" "$signal"
  return 1
}

if [[ "$WATCH" == "1" ]]; then
  while true; do
    ensure_once || true
    sleep "$INTERVAL_SECONDS"
  done
fi

ensure_once
