#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
GRAPH_FILE="${PROJECT_ROOT}/.beads/issues.jsonl"

LIMIT=12
STRICT=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Audit the next ready bead batch before a large swarm launch.

Options:
  --limit N          Number of ready issues to audit (default: 12)
  --strict           Treat warnings as launch-blocking
  -h, --help         Show this help
EOF
}

require_command() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Missing required command: $cmd" >&2
    exit 1
  fi
}

run_br_json() {
  local attempts=0
  local max_attempts=4
  local output
  local status

  while true; do
    if output=$(cd "$PROJECT_ROOT" && RUST_LOG=error br "$@" --json --no-auto-import --no-auto-flush 2>&1); then
      printf '%s\n' "$output"
      return 0
    fi
    status=$?
    if [[ "$output" == *"database is busy"* && "$attempts" -lt "$max_attempts" ]]; then
      attempts=$((attempts + 1))
      sleep 1
      continue
    fi
    printf '%s\n' "$output" >&2
    return "$status"
  done
}

print_queue_repair_playbook() {
  cat <<'EOF'
Queue repair playbook when useful work exists but `br ready` is thin or empty:
1. Run `br blocked` and `bv --robot-triage` to identify near-frontier blockers.
2. Inspect likely frontier beads with `br show <id>` and validate dependency edges.
3. If the next safe slice is obvious but missing, create/split a child bead with explicit acceptance criteria and validation contract.
4. Announce bead shaping in Agent Mail and run `br sync --flush-only`.
5. Re-run this audit before launching or re-launching a large swarm batch.
EOF
}

warn() {
  local message="$1"
  echo "WARN: $message"
  WARNINGS=$((WARNINGS + 1))
}

err() {
  local message="$1"
  echo "ERROR: $message"
  ERRORS=$((ERRORS + 1))
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --limit)
      LIMIT="$2"
      shift 2
      ;;
    --strict)
      STRICT=1
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

if ! [[ "$LIMIT" =~ ^[0-9]+$ ]] || [[ "$LIMIT" -le 0 ]]; then
  echo "--limit must be a positive integer" >&2
  exit 1
fi

require_command br
require_command jq

if [[ ! -f "$GRAPH_FILE" ]]; then
  echo "Missing bead graph file: $GRAPH_FILE" >&2
  exit 1
fi

READY_JSON=$(run_br_json ready)
OPEN_JSON=$(run_br_json list --status open)
GRAPH_JSON=$(jq -s '.' "$GRAPH_FILE")

READY_COUNT=$(jq 'length' <<<"$READY_JSON")
OPEN_COUNT=$(jq 'length' <<<"$OPEN_JSON")
BATCH_JSON=$(jq --argjson limit "$LIMIT" '.[0:$limit]' <<<"$READY_JSON")
BATCH_COUNT=$(jq 'length' <<<"$BATCH_JSON")

WARNINGS=0
ERRORS=0
declare -A DEP_STATUS_CACHE

echo "=== Pre-swarm bead-batch audit ==="
echo "Project root: $PROJECT_ROOT"
echo "Ready issues: $READY_COUNT"
echo "Open issues: $OPEN_COUNT"
echo "Audit batch limit: $LIMIT"
echo

if [[ "$READY_COUNT" -eq 0 ]]; then
  err "br ready returned zero issues."
  if [[ "$OPEN_COUNT" -gt 0 ]]; then
    echo "Open work still exists, so this is likely a graph/frontier hygiene gap."
  fi
  echo
  print_queue_repair_playbook
  exit 1
fi

echo "Auditing first $BATCH_COUNT ready issues..."
echo

while IFS= read -r issue_id; do
  [[ -n "$issue_id" ]] || continue

  ISSUE_JSON=$(run_br_json show "$issue_id")
  ISSUE_TYPE=$(jq -r '.[0].issue_type // "unknown"' <<<"$ISSUE_JSON")
  ISSUE_TITLE=$(jq -r '.[0].title // ""' <<<"$ISSUE_JSON")
  ACCEPTANCE=$(jq -r '.[0].acceptance_criteria // ""' <<<"$ISSUE_JSON")

  echo "- $issue_id [$ISSUE_TYPE] $ISSUE_TITLE"

  if [[ -z "${ACCEPTANCE//[[:space:]]/}" ]]; then
    warn "$issue_id has no acceptance criteria."
  elif [[ "${#ACCEPTANCE}" -lt 48 ]]; then
    warn "$issue_id acceptance criteria is short; verify close conditions are explicit."
  elif grep -Eiq '\b(todo|tbd|later)\b' <<<"$ACCEPTANCE"; then
    warn "$issue_id acceptance criteria looks provisional (contains TODO/TBD/later)."
  fi

  while IFS= read -r dep_id; do
    [[ -n "$dep_id" ]] || continue

    dep_status="${DEP_STATUS_CACHE[$dep_id]:-}"
    if [[ -z "$dep_status" ]]; then
      if ! DEP_JSON=$(run_br_json show "$dep_id"); then
        err "$issue_id has an unreadable blocking dependency: $dep_id"
        continue
      fi
      dep_status=$(jq -r '.[0].status // "unknown"' <<<"$DEP_JSON")
      DEP_STATUS_CACHE["$dep_id"]="$dep_status"
    fi

    if [[ "$dep_status" != "closed" ]]; then
      err "$issue_id is ready but blocking dependency $dep_id is '$dep_status'."
    fi
  done < <(jq -r '.[0].dependencies[]? | select(.type == "blocks") | .depends_on_id' <<<"$ISSUE_JSON")

  if [[ "$ISSUE_TYPE" != "task" ]]; then
    open_children_count=$(jq --arg parent "$issue_id" '[.[] | select(.status == "open") | select(any(.dependencies[]?; .type == "parent-child" and .depends_on_id == $parent))] | length' <<<"$GRAPH_JSON")
    ready_children_count=$(jq --arg parent "$issue_id" '[.[] | select(any(.dependencies[]?; .type == "parent-child" and .depends_on_id == $parent))] | length' <<<"$BATCH_JSON")

    if [[ "$open_children_count" -eq 0 ]]; then
      warn "$issue_id is a ready $ISSUE_TYPE with no open child beads (possible missing leaf)."
    elif [[ "$ready_children_count" -eq 0 ]]; then
      warn "$issue_id is ready but none of its $open_children_count open children are in this ready batch."
    fi
  fi
done < <(jq -r '.[].id' <<<"$BATCH_JSON")

LEAF_COUNT=$(jq '[.[] | select(.issue_type == "task")] | length' <<<"$BATCH_JSON")
NON_LEAF_COUNT=$((BATCH_COUNT - LEAF_COUNT))

if [[ "$LEAF_COUNT" -eq 0 ]]; then
  warn "No task-level leaves in the audited ready batch."
fi

echo
echo "Summary:"
echo "  batch_size=$BATCH_COUNT"
echo "  leaf_tasks=$LEAF_COUNT"
echo "  non_leaf_items=$NON_LEAF_COUNT"
echo "  warnings=$WARNINGS"
echo "  errors=$ERRORS"

if [[ "$ERRORS" -gt 0 ]]; then
  echo
  echo "Audit failed: dependency sanity errors detected."
  print_queue_repair_playbook
  exit 1
fi

if [[ "$STRICT" -eq 1 && "$WARNINGS" -gt 0 ]]; then
  echo
  echo "Audit completed with warnings and strict mode is enabled."
  print_queue_repair_playbook
  exit 2
fi

if [[ "$WARNINGS" -gt 0 ]]; then
  echo
  echo "Audit completed with warnings; run queue repair before launching a large swarm."
  print_queue_repair_playbook
  exit 0
fi

echo
echo "Audit passed with no warnings."
