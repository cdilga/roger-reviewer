#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
BR_RESOLVER="${SCRIPT_DIR}/resolve_br.sh"
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"

SESSION_NAME="${DEFAULT_SESSION_NAME}"
CODEX_COUNT=4
CLAUDE_COUNT=4
GEMINI_COUNT=2
OPENCODE_COUNT=0
RETRY_COUNT=3
RETRY_DELAY_SECONDS=1
BR_BIN=""
LAST_BR_ERROR_OUTPUT=""

usage() {
  cat <<EOF_USAGE
Usage: $(basename "$0") [options]

Fail-loud swarm preflight for the vetted br path, queue health, and launch readiness.

Options:
  --session NAME           Tmux session name to validate for launch
  --codex N                Planned Codex worker count
  --claude N               Planned Claude worker count
  --gemini N               Planned Gemini worker count
  --opencode N             Planned OpenCode worker count
  --retries N              Retries for transient 'database is busy' (default: 3)
  --retry-delay-seconds N  Delay between retries (default: 1)
  -h, --help               Show this help
EOF_USAGE
}

require_command() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "PREFLIGHT_STATUS=fail"
    echo "PREFLIGHT_CLASS=operator_actionable"
    echo "PREFLIGHT_REASON=missing command: $cmd"
    exit 1
  fi
}

operator_fail() {
  local reason="$1"
  local detail="${2:-}"
  echo "PREFLIGHT_STATUS=fail"
  echo "PREFLIGHT_CLASS=operator_actionable"
  echo "PREFLIGHT_REASON=$reason"
  if [[ -n "$detail" ]]; then
    echo "PREFLIGHT_DETAIL_START"
    echo "$detail"
    echo "PREFLIGHT_DETAIL_END"
  fi
  exit 1
}

transient_fail() {
  local reason="$1"
  local detail="${2:-}"
  echo "PREFLIGHT_STATUS=retry"
  echo "PREFLIGHT_CLASS=transient_retry"
  echo "PREFLIGHT_REASON=$reason"
  if [[ -n "$detail" ]]; then
    echo "PREFLIGHT_DETAIL_START"
    echo "$detail"
    echo "PREFLIGHT_DETAIL_END"
  fi
  exit 75
}

is_transient_lock_output() {
  local output="${1:-}"
  [[ "$output" == *"database is busy"* ]] || \
    [[ "$output" == *"database is locked"* ]] || \
    [[ "$output" == *"SQLITE_BUSY"* ]] || \
    [[ "$output" == *"resource temporarily unavailable"* ]]
}

run_br_doctor_with_retry() {
  local attempts=0
  local output

  while true; do
    if output=$(cd "$PROJECT_ROOT" && "$BR_BIN" doctor --no-auto-import --no-auto-flush 2>&1); then
      printf '%s\n' "$output"
      return 0
    fi
    if is_transient_lock_output "$output"; then
      if (( attempts < RETRY_COUNT )); then
        attempts=$((attempts + 1))
        sleep "$RETRY_DELAY_SECONDS"
        continue
      fi
      LAST_BR_ERROR_OUTPUT="$output"
      return 75
    fi
    LAST_BR_ERROR_OUTPUT="$output"
    return 1
  done
}

run_br_json_with_retry() {
  local attempts=0
  local output

  while true; do
    if output=$(cd "$PROJECT_ROOT" && RUST_LOG=error "$BR_BIN" "$@" --json --no-auto-import --no-auto-flush 2>&1); then
      printf '%s\n' "$output"
      return 0
    fi
    if is_transient_lock_output "$output"; then
      if (( attempts < RETRY_COUNT )); then
        attempts=$((attempts + 1))
        sleep "$RETRY_DELAY_SECONDS"
        continue
      fi
      LAST_BR_ERROR_OUTPUT="$output"
      return 75
    fi
    LAST_BR_ERROR_OUTPUT="$output"
    return 1
  done
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
    --retries)
      RETRY_COUNT="$2"
      shift 2
      ;;
    --retry-delay-seconds)
      RETRY_DELAY_SECONDS="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      operator_fail "unknown argument: $1"
      ;;
  esac
done

for value in "$CODEX_COUNT" "$CLAUDE_COUNT" "$GEMINI_COUNT" "$OPENCODE_COUNT" "$RETRY_COUNT" "$RETRY_DELAY_SECONDS"; do
  if ! [[ "$value" =~ ^[0-9]+$ ]]; then
    operator_fail "numeric options must be integers"
  fi
done

require_command tmux
require_command curl
require_command am
require_command bv
require_command jq

if (( CODEX_COUNT > 0 )); then
  require_command codex
fi
if (( CLAUDE_COUNT > 0 )); then
  require_command claude
fi
if (( GEMINI_COUNT > 0 )); then
  require_command gemini
fi
if (( OPENCODE_COUNT > 0 )); then
  require_command opencode
fi

if [[ ! -x "$BR_RESOLVER" ]]; then
  operator_fail "missing required script: $BR_RESOLVER"
fi
if ! BR_BIN="$($BR_RESOLVER --quiet --print-path)"; then
  operator_fail "unable to resolve vetted br path"
fi

if ! curl -fsS http://127.0.0.1:8765/health/readiness >/dev/null; then
  operator_fail "agent mail readiness check failed" "Start or inspect Agent Mail with: tmux attach -t mcp-agent-mail"
fi

if tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  operator_fail "tmux session already exists: $SESSION_NAME" "Pick a different --session or remove the existing session before launch."
fi

if ! doctor_output="$(run_br_doctor_with_retry)"; then
  status=$?
  if [[ "$status" -eq 75 ]]; then
    transient_fail "br doctor hit transient sqlite lock after retries" "$LAST_BR_ERROR_OUTPUT"
  fi
  operator_fail "br doctor failed" "$LAST_BR_ERROR_OUTPUT"
fi
doctor_errors="$(grep '^ERROR ' <<<"$doctor_output" || true)"
if [[ -n "$doctor_errors" ]]; then
  fatal_doctor_errors="$(grep -Ev '^ERROR db\.recoverable_anomalies: blocked_issues_cache is marked stale and needs rebuild$' <<<"$doctor_errors" || true)"
  if [[ -n "$fatal_doctor_errors" ]]; then
    operator_fail "br doctor reported workspace health errors" "$doctor_output"
  fi
  echo "WARN: br doctor reported stale blocked cache metadata; treating as recoverable advisory for launch preflight."
fi
if grep -q '^WARN db.recovery_artifacts:' <<<"$doctor_output"; then
  echo "WARN: br doctor reported preserved recovery artifacts; advisory only."
fi
if grep -q '^WARN db.sidecars:' <<<"$doctor_output"; then
  echo "WARN: br doctor reported sqlite sidecar warnings; advisory only unless accompanied by fatal ERROR lines."
fi

if ! ready_json="$(run_br_json_with_retry ready)"; then
  status=$?
  if [[ "$status" -eq 75 ]]; then
    transient_fail "br ready hit database busy after retries" "$LAST_BR_ERROR_OUTPUT"
  fi
  operator_fail "br ready failed" "$LAST_BR_ERROR_OUTPUT"
fi

if ! open_json="$(run_br_json_with_retry list --status open)"; then
  status=$?
  if [[ "$status" -eq 75 ]]; then
    transient_fail "br list --status open hit database busy after retries" "$LAST_BR_ERROR_OUTPUT"
  fi
  operator_fail "br list --status open failed" "$LAST_BR_ERROR_OUTPUT"
fi

ready_count="$(jq 'length' <<<"$ready_json")"
open_count="$(jq 'length' <<<"$open_json")"
planned_workers=$((CODEX_COUNT + CLAUDE_COUNT + GEMINI_COUNT + OPENCODE_COUNT))

if [[ "$ready_count" -eq 0 ]]; then
  if [[ "$open_count" -gt 0 ]]; then
    operator_fail \
      "no ready beads while open work still exists" \
      "Run ./scripts/swarm/audit_bead_batch.sh --limit 20 --strict and follow the queue-repair playbook before launch."
  fi
  operator_fail "no open or ready beads available for launch"
fi

echo "=== Swarm preflight ==="
echo "Project root: $PROJECT_ROOT"
echo "Session: $SESSION_NAME"
echo "Resolved br path: $BR_BIN"
echo "Resolved br version: $($BR_BIN --version)"
echo "Open issues: $open_count"
echo "Ready issues: $ready_count"
echo "Planned workers: $planned_workers"

if [[ "$ready_count" -lt "$planned_workers" ]]; then
  echo "WARN: ready queue ($ready_count) is smaller than planned workers ($planned_workers); some agents may idle."
fi

echo "PREFLIGHT_STATUS=pass"
echo "PREFLIGHT_CLASS=ready"
echo "PREFLIGHT_REASON=launch preflight checks passed"
