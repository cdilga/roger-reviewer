#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_BEADS_DIR="${PROJECT_ROOT}/.beads"
DEFAULT_DB_PATH="${DEFAULT_BEADS_DIR}/beads.db"
DEFAULT_JSONL_PATH="${DEFAULT_BEADS_DIR}/issues.jsonl"
BR_RESOLVER="${SCRIPT_DIR}/resolve_br.sh"

BEADS_DIR="${DEFAULT_BEADS_DIR}"
DB_PATH="${DEFAULT_DB_PATH}"
JSONL_PATH="${DEFAULT_JSONL_PATH}"
RUN_BR_DOCTOR=1
BR_BIN=""

cleanup() {
  rm -f "${TMP_DB:-}" "${TMP_JSONL:-}"
}
trap cleanup EXIT

usage() {
  cat <<'EOF'
Usage: check_beads_trust.sh [--beads-dir <path>] [--db <path>] [--jsonl <path>] [--skip-br-doctor]

Run a direct workspace-trust check for the Roger beads workspace without
depending on br's higher-level queue interpretation.

Checks:
- SQLite integrity and foreign-key check
- DB issue count vs JSONL line count
- exact ID/status parity between DB and JSONL
- optional br doctor summary if br is resolvable

Exit codes:
- 0: trust checks passed
- 1: trust checks found a real mismatch or integrity failure
- 2: prerequisites or workspace files are missing
EOF
}

require_command() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "TRUST_STATUS=fail"
    echo "TRUST_REASON=missing command: $cmd"
    exit 2
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --beads-dir)
      if [[ $# -lt 2 ]]; then
        echo "TRUST_STATUS=fail"
        echo "TRUST_REASON=missing value for --beads-dir"
        exit 2
      fi
      BEADS_DIR="$2"
      DB_PATH="${BEADS_DIR}/beads.db"
      JSONL_PATH="${BEADS_DIR}/issues.jsonl"
      shift 2
      ;;
    --db)
      if [[ $# -lt 2 ]]; then
        echo "TRUST_STATUS=fail"
        echo "TRUST_REASON=missing value for --db"
        exit 2
      fi
      DB_PATH="$2"
      shift 2
      ;;
    --jsonl)
      if [[ $# -lt 2 ]]; then
        echo "TRUST_STATUS=fail"
        echo "TRUST_REASON=missing value for --jsonl"
        exit 2
      fi
      JSONL_PATH="$2"
      shift 2
      ;;
    --skip-br-doctor)
      RUN_BR_DOCTOR=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "TRUST_STATUS=fail"
      echo "TRUST_REASON=unknown argument: $1"
      usage
      exit 2
      ;;
  esac
done

if [[ "$DB_PATH" != "${DEFAULT_DB_PATH}" || "$JSONL_PATH" != "${DEFAULT_JSONL_PATH}" ]]; then
  RUN_BR_DOCTOR=0
fi

require_command sqlite3
require_command jq
require_command mktemp

if [[ ! -f "$DB_PATH" ]]; then
  echo "TRUST_STATUS=fail"
  echo "TRUST_REASON=missing db: $DB_PATH"
  exit 2
fi

if [[ ! -f "$JSONL_PATH" ]]; then
  echo "TRUST_STATUS=fail"
  echo "TRUST_REASON=missing jsonl: $JSONL_PATH"
  exit 2
fi

TMP_DB=$(mktemp)
TMP_JSONL=$(mktemp)

db_integrity=$(sqlite3 "$DB_PATH" "PRAGMA integrity_check;")
if [[ "$db_integrity" != "ok" ]]; then
  echo "TRUST_STATUS=fail"
  echo "TRUST_REASON=sqlite integrity check failed"
  echo "SQLITE_INTEGRITY=$db_integrity"
  exit 1
fi

fk_check=$(sqlite3 "$DB_PATH" "PRAGMA foreign_key_check;")
if [[ -n "$fk_check" ]]; then
  echo "TRUST_STATUS=fail"
  echo "TRUST_REASON=sqlite foreign key check failed"
  echo "SQLITE_FOREIGN_KEY_CHECK_START"
  echo "$fk_check"
  echo "SQLITE_FOREIGN_KEY_CHECK_END"
  exit 1
fi

db_total=$(sqlite3 "$DB_PATH" "select count(*) from issues;")
db_open=$(sqlite3 "$DB_PATH" "select count(*) from issues where status='open';")
db_closed=$(sqlite3 "$DB_PATH" "select count(*) from issues where status='closed';")
jsonl_total=$(wc -l < "$JSONL_PATH" | tr -d ' ')
jsonl_open=$(jq -r 'select(.status == "open") | .id' "$JSONL_PATH" | wc -l | tr -d ' ')
jsonl_closed=$(jq -r 'select(.status == "closed") | .id' "$JSONL_PATH" | wc -l | tr -d ' ')

sqlite3 "$DB_PATH" "select id || '|' || status from issues order by id;" | LC_ALL=C sort > "$TMP_DB"
jq -r '.id + "|" + .status' "$JSONL_PATH" | LC_ALL=C sort > "$TMP_JSONL"

missing_in_db=$(comm -23 "$TMP_JSONL" "$TMP_DB" || true)
missing_in_jsonl=$(comm -13 "$TMP_JSONL" "$TMP_DB" || true)

doctor_status="skipped"
doctor_summary=""
if [[ "$RUN_BR_DOCTOR" -eq 1 && -x "$BR_RESOLVER" ]] && BR_BIN="$($BR_RESOLVER --quiet --print-path 2>/dev/null)"; then
  if doctor_output=$(cd "$PROJECT_ROOT" && "$BR_BIN" doctor 2>&1); then
    doctor_status="ok"
    doctor_summary=$(printf '%s\n' "$doctor_output" | tail -n 1)
  else
    doctor_status="failed"
    doctor_summary=$(printf '%s\n' "$doctor_output" | tail -n 1)
  fi
fi

echo "DB_PATH=$DB_PATH"
echo "JSONL_PATH=$JSONL_PATH"
echo "DB_TOTAL=$db_total"
echo "DB_OPEN=$db_open"
echo "DB_CLOSED=$db_closed"
echo "JSONL_TOTAL=$jsonl_total"
echo "JSONL_OPEN=$jsonl_open"
echo "JSONL_CLOSED=$jsonl_closed"
echo "BR_DOCTOR_STATUS=$doctor_status"
if [[ -n "$doctor_summary" ]]; then
  echo "BR_DOCTOR_SUMMARY=$doctor_summary"
fi

if [[ "$db_total" != "$jsonl_total" || "$db_open" != "$jsonl_open" || "$db_closed" != "$jsonl_closed" ]]; then
  echo "TRUST_STATUS=fail"
  echo "TRUST_REASON=count mismatch between db and jsonl"
  exit 1
fi

if [[ -n "$missing_in_db" ]]; then
  echo "TRUST_STATUS=fail"
  echo "TRUST_REASON=entries present in jsonl but missing or mismatched in db"
  echo "MISSING_IN_DB_START"
  echo "$missing_in_db"
  echo "MISSING_IN_DB_END"
  exit 1
fi

if [[ -n "$missing_in_jsonl" ]]; then
  echo "TRUST_STATUS=fail"
  echo "TRUST_REASON=entries present in db but missing or mismatched in jsonl"
  echo "MISSING_IN_JSONL_START"
  echo "$missing_in_jsonl"
  echo "MISSING_IN_JSONL_END"
  exit 1
fi

echo "TRUST_STATUS=pass"
echo "TRUST_REASON=db and jsonl agree on current issue truth"
