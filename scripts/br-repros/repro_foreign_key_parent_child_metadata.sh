#!/usr/bin/env bash
set -euo pipefail

BR_BIN="${BR_BIN:-br}"
ITERATIONS="${ITERATIONS:-200}"
READ_PATH_STRESS="${READ_PATH_STRESS:-1}"

if [[ ! -x "$BR_BIN" ]]; then
  if ! command -v "$BR_BIN" >/dev/null 2>&1; then
    echo "Missing executable BR_BIN: $BR_BIN" >&2
    exit 2
  fi
  BR_BIN="$(command -v "$BR_BIN")"
fi

if ! [[ "$ITERATIONS" =~ ^[0-9]+$ ]] || [[ "$ITERATIONS" -lt 1 ]]; then
  echo "ITERATIONS must be a positive integer (got: $ITERATIONS)" >&2
  exit 2
fi
if ! [[ "$READ_PATH_STRESS" =~ ^[01]$ ]]; then
  echo "READ_PATH_STRESS must be 0 or 1 (got: $READ_PATH_STRESS)" >&2
  exit 2
fi

tmp=$(mktemp -d /tmp/br-repro-fk-parent-child.XXXXXX)
cd "$tmp"

echo "workspace=$tmp"
echo "br_bin=$BR_BIN"
echo "iterations=$ITERATIONS"
echo "read_path_stress=$READ_PATH_STRESS"

run_br() {
  local output
  if ! output=$("$BR_BIN" "$@" 2>&1); then
    echo "command_failed=$BR_BIN $*" >&2
    echo "$output" >&2
    if [[ "$output" == *"FOREIGN KEY constraint failed"* ]]; then
      echo "fk_failure_detected=1" >&2
      exit 1
    fi
    exit 3
  fi
  printf '%s\n' "$output"
}

run_br init >/dev/null

for i in $(seq 1 "$ITERATIONS"); do
  parent_id=$(run_br create "probe-parent-$i" --type task --silent)
  child_id=$(run_br create "probe-child-$i" --type task --silent)

  # Stress the "post-create metadata update" surface cited in rr-1ab.5.
  run_br update "$child_id" --notes "probe-created-notes-$i" >/dev/null

  run_br dep add "$child_id" "$parent_id" --type parent-child >/dev/null
  if [[ "$READ_PATH_STRESS" == "1" ]]; then
    "$BR_BIN" ready >/dev/null 2>&1 || true
    "$BR_BIN" show "$child_id" >/dev/null 2>&1 || true
  fi
  run_br update "$child_id" --notes "probe-notes-$i" >/dev/null
  run_br update "$child_id" --acceptance-criteria "probe-acceptance-$i" >/dev/null
  run_br update "$parent_id" --notes "probe-parent-notes-$i" >/dev/null

  if (( i % 25 == 0 )); then
    echo "progress=$i/$ITERATIONS"
  fi
done

echo "fk_failure_detected=0"
sqlite3 .beads/beads.db "PRAGMA foreign_key_check;"
echo "status=pass"
