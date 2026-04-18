#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/rr-exact-id-fallback.XXXXXX")"
trap 'rm -rf "${WORKDIR}"' EXIT

BEADS_DIR="${WORKDIR}/.beads"
mkdir -p "${BEADS_DIR}"

cat >"${BEADS_DIR}/issues.jsonl" <<'JSONL'
{"id":"rr-parent","title":"Open parent","status":"open","priority":0,"issue_type":"epic","created_at":"2026-04-14T20:41:14Z","updated_at":"2026-04-16T23:43:13Z","source_repo":"."}
{"id":"rr-real-blocker","title":"Open blocker","status":"open","priority":1,"issue_type":"task","created_at":"2026-04-14T20:41:14Z","updated_at":"2026-04-16T23:43:13Z","source_repo":"."}
{"id":"rr-child-ok","title":"Leaf with open parent","status":"open","priority":1,"issue_type":"task","created_at":"2026-04-14T20:41:14Z","updated_at":"2026-04-16T23:43:13Z","source_repo":".","dependencies":[{"issue_id":"rr-child-ok","depends_on_id":"rr-parent","type":"parent-child","created_at":"2026-04-16T23:43:13Z","created_by":"test","metadata":"{}","thread_id":""}]}
{"id":"rr-child-blocked","title":"Leaf with real blocker","status":"open","priority":1,"issue_type":"task","created_at":"2026-04-14T20:41:14Z","updated_at":"2026-04-16T23:43:13Z","source_repo":".","dependencies":[{"issue_id":"rr-child-blocked","depends_on_id":"rr-real-blocker","type":"blocks","created_at":"2026-04-16T23:43:13Z","created_by":"test","metadata":"{}","thread_id":""}]}
JSONL

"${SCRIPT_DIR}/exact_id_status_fallback.py" \
  --beads-dir "${BEADS_DIR}" \
  --command close \
  --id rr-child-ok \
  --reason "validated close"

grep -q '"id":"rr-child-ok","title":"Leaf with open parent","status":"closed"' "${BEADS_DIR}/issues.jsonl"
grep -q '"close_reason":"validated close"' "${BEADS_DIR}/issues.jsonl"

set +e
blocked_output="$(
  "${SCRIPT_DIR}/exact_id_status_fallback.py" \
    --beads-dir "${BEADS_DIR}" \
    --command close \
    --id rr-child-blocked \
    --reason "should stay open" 2>&1
)"
blocked_status=$?
set -e

[[ "${blocked_status}" -eq 4 ]]
grep -q 'open dependencies: rr-real-blocker' <<<"${blocked_output}"
grep -q '"id":"rr-child-blocked","title":"Leaf with real blocker","status":"open"' "${BEADS_DIR}/issues.jsonl"

printf 'exact_id_status_fallback parent-child regression passed.\n'
