#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
BR_RESOLVER="${SCRIPT_DIR}/resolve_br.sh"
DEFAULT_SESSION_NAME="$(basename "$PROJECT_ROOT" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-*//; s/-*$//')"
UPSTREAM_NTM="${NTM_UPSTREAM_BIN:-${HOME}/.local/lib/acfs/bin/ntm}"

SESSION_NAME="${DEFAULT_SESSION_NAME}"
INTERVAL_SECONDS=15
IDLE_SECONDS=45
COOLDOWN_SECONDS=180
STATE_ROOT="${TMPDIR:-/tmp}/roger-reviewer-swarm-supervisor"
BR_BIN=""

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Options:
  --session NAME            Swarm session name to supervise
  --interval-seconds N      Polling interval between checks
  --idle-seconds N          How long a pane must sit idle before nudging
  --cooldown-seconds N      Minimum gap between nudges for the same pane
  -h, --help                Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --session)
      SESSION_NAME="$2"
      shift 2
      ;;
    --interval-seconds)
      INTERVAL_SECONDS="$2"
      shift 2
      ;;
    --idle-seconds)
      IDLE_SECONDS="$2"
      shift 2
      ;;
    --cooldown-seconds)
      COOLDOWN_SECONDS="$2"
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

if [[ ! -x "$UPSTREAM_NTM" ]]; then
  echo "Upstream NTM binary not found at: $UPSTREAM_NTM" >&2
  exit 1
fi
if [[ ! -x "$BR_RESOLVER" ]]; then
  echo "Missing required script: $BR_RESOLVER" >&2
  exit 1
fi
if ! tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  echo "Tmux session '$SESSION_NAME' does not exist." >&2
  exit 1
fi
if ! BR_BIN="$("$BR_RESOLVER" --quiet --print-path)"; then
  echo "Unable to resolve pinned br path for this workspace." >&2
  exit 1
fi

mkdir -p "${STATE_ROOT}/${SESSION_NAME}"

state_file() {
  local pane="$1"
  local suffix="$2"
  printf '%s/%s/pane-%s.%s\n' "$STATE_ROOT" "$SESSION_NAME" "$pane" "$suffix"
}

count_ready() {
  (
    cd "$PROJECT_ROOT"
    "$BR_BIN" ready --json 2>/dev/null || printf '[]\n'
  ) | jq 'if type == "array" then length else 0 end' 2>/dev/null || echo 0
}

count_status() {
  local status="$1"
  sqlite3 "${PROJECT_ROOT}/.beads/beads.db" \
    "select count(*) from issues where status='${status//\'/}';" 2>/dev/null || echo 0
}

agent_states_tsv() {
  "$UPSTREAM_NTM" health "$SESSION_NAME" --json 2>/dev/null | jq -r \
    '
      .agents[]
      | select(.process_status == "running")
      | select((.rate_limited // false) | not)
      | [.pane, (.activity // "unknown")] | @tsv
    '
}

continuation_prompt() {
  cat <<'EOF'
Continue autonomously from the current repo state. This swarm is persistent, so do not stop at the current prompt.

Immediately:
1. check Agent Mail and acknowledge anything pending
2. rerun `br ready`
3. if a ready bead exists, inspect it with `br show`, claim it, reserve files, and continue working
4. if `br` says `database is busy`, back off and retry before concluding queue state; if retries still fail, use `br ready --no-daemon`, `br show <id> --no-daemon`, and `br update <id> --status in_progress --no-daemon`
5. if `br ready` is still empty but useful open work remains, inspect `bv --robot-triage`, `br list --status open`, and the most adjacent blocked frontier; only create or split the next safe bead if the slice is obvious and you can attach a truthful validation contract

Do not ask the human for routine permission. Either claim real work, widen the graph safely, or post an explicit exhausted-queue report in Agent Mail and hold.
EOF
}

frontier_prompt() {
  cat <<'EOF'
The shared queue currently has open work but no ready leaf beads. Take one bounded frontier-widening pass now.

Do this in order:
1. re-check Agent Mail for coordination state
2. inspect `br list --status open`, `bv --robot-triage`, and the nearest blocked frontier
3. if the next safe slice is obvious, create or split the missing bead with a truthful validation contract and announce it in Agent Mail
4. if the graph is genuinely exhausted, send an explicit exhausted-queue note and hold

Do not do speculative product work without a bead. Either mint the missing safe bead or explain why the frontier is truly exhausted.
EOF
}

nudge_pane() {
  local pane="$1"
  local kind="$2"
  local now_ts="$3"
  local prompt_file
  prompt_file="$(mktemp "${TMPDIR:-/tmp}/roger-supervisor-prompt.XXXXXX")"
  if [[ "$kind" == "frontier" ]]; then
    frontier_prompt >"$prompt_file"
  else
    continuation_prompt >"$prompt_file"
  fi
  "$UPSTREAM_NTM" send "$SESSION_NAME" --pane="$pane" --file "$prompt_file" --no-cass-check >/dev/null
  rm -f "$prompt_file"
  printf '%s nudged pane %s (%s)\n' "$(date -Iseconds)" "$pane" "$kind"
  printf '%s\n' "$now_ts" >"$(state_file "$pane" last_nudge)"
}

while true; do
  if ! tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
    echo "Main swarm session '$SESSION_NAME' disappeared; supervisor exiting." >&2
    exit 0
  fi

  ready_count="$(count_ready)"
  open_count="$(count_status open)"
  in_progress_count="$(count_status in_progress)"

  if (( ready_count == 0 && open_count == 0 && in_progress_count == 0 )); then
    sleep "$INTERVAL_SECONDS"
    continue
  fi

  nudges_remaining="$ready_count"
  nudged_frontier=0
  now_ts="$(date +%s)"

  while IFS=$'\t' read -r pane activity; do
    [[ -n "$pane" ]] || continue
    idle_since_file="$(state_file "$pane" idle_since)"
    if [[ "$activity" != "idle" ]]; then
      rm -f "$idle_since_file"
      continue
    fi
    if [[ ! -f "$idle_since_file" ]]; then
      printf '%s\n' "$now_ts" >"$idle_since_file"
      continue
    fi
    idle_since_ts="$(cat "$idle_since_file")"
    if (( now_ts - idle_since_ts < IDLE_SECONDS )); then
      continue
    fi

    last_nudge_file="$(state_file "$pane" last_nudge)"
    last_nudge_ts=0
    if [[ -f "$last_nudge_file" ]]; then
      last_nudge_ts="$(cat "$last_nudge_file")"
    fi
    if (( now_ts - last_nudge_ts < COOLDOWN_SECONDS )); then
      continue
    fi

    if (( ready_count > 0 )); then
      if (( nudges_remaining <= 0 )); then
        break
      fi
      nudge_pane "$pane" "continue" "$now_ts"
      nudges_remaining=$((nudges_remaining - 1))
      sleep 2
      continue
    fi

    if (( open_count > 0 && in_progress_count == 0 && nudged_frontier == 0 )); then
      nudge_pane "$pane" "frontier" "$now_ts"
      nudged_frontier=1
      sleep 2
      break
    fi
  done < <(agent_states_tsv)

  sleep "$INTERVAL_SECONDS"
done
