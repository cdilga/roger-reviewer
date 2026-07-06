#!/usr/bin/env bash
set -euo pipefail

# set_bead_status_via_jsonl.sh
#
# Safe status mutation for beads that the pinned `br` binary cannot mutate
# through its normal write path. See bead rr-lwj2: `br` 0.1.40 fails with
# "Issue not found" AND corrupts .beads/beads.db when `br update`/`br close`
# targets a DOTTED hierarchical id (e.g. rr-...-ziv8.1), while the same
# operations succeed on non-dotted ids. The READ path (`br show`) and the
# JSONL import/rebuild path both handle dotted ids correctly.
#
# This helper edits the canonical .beads/issues.jsonl record in place (the
# source of truth) and then rebuilds the DB family from it via
# rebuild_beads_db_safe.sh, so the logical mutation lands without leaving a
# corrupt binary DB behind. Prefer normal `br update`/`br close` for
# non-dotted ids; use this only when `br` cannot address the bead.
#
# Usage:
#   set_bead_status_via_jsonl.sh --bead <id> --status <status> [--reason "<text>"]
#
# Examples:
#   set_bead_status_via_jsonl.sh --bead rr-foo-ziv8.1 --status in_progress
#   set_bead_status_via_jsonl.sh --bead rr-foo-ziv8.1 --status closed --reason "Done: ..."

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
JSONL_PATH="${PROJECT_ROOT}/.beads/issues.jsonl"
REBUILD_SCRIPT="${SCRIPT_DIR}/rebuild_beads_db_safe.sh"

BEAD_ID=""
STATUS=""
REASON=""

usage() {
  grep '^#' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --bead) BEAD_ID="${2:-}"; shift 2 ;;
    --status) STATUS="${2:-}"; shift 2 ;;
    --reason) REASON="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ -z "${BEAD_ID}" || -z "${STATUS}" ]]; then
  echo "ERROR: --bead and --status are required" >&2
  usage >&2
  exit 2
fi

case "${STATUS}" in
  open|in_progress|blocked|closed) ;;
  *) echo "ERROR: unsupported --status '${STATUS}' (open|in_progress|blocked|closed)" >&2; exit 2 ;;
esac

if [[ ! -f "${JSONL_PATH}" ]]; then
  echo "ERROR: canonical JSONL not found: ${JSONL_PATH}" >&2
  exit 2
fi

# Edit the canonical record. Keys are written alphabetically with compact
# separators to match `br`'s own JSONL serialization, so the rebuild stays a
# clean round-trip with no spurious diff churn.
BEAD_ID="${BEAD_ID}" STATUS="${STATUS}" REASON="${REASON}" JSONL_PATH="${JSONL_PATH}" \
python3 - <<'PY'
import json, os, datetime

bead_id = os.environ["BEAD_ID"]
status = os.environ["STATUS"]
reason = os.environ.get("REASON", "")
path = os.environ["JSONL_PATH"]
ts = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%f") + "Z"

out = []
found = False
with open(path) as f:
    for line in f:
        s = line.strip()
        if not s:
            out.append(line)
            continue
        o = json.loads(s)
        if o.get("id") == bead_id:
            found = True
            o["status"] = status
            o["updated_at"] = ts
            if status == "closed":
                o["closed_at"] = ts
                if reason:
                    o["close_reason"] = reason
            else:
                # Reopening clears the closed marker so state stays truthful.
                o.pop("closed_at", None)
            out.append(json.dumps(o, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n")
        else:
            out.append(line)

if not found:
    raise SystemExit(f"ERROR: bead id not found in JSONL: {bead_id}")

with open(path, "w") as f:
    f.writelines(out)

# Validate every record still parses before we hand off to the rebuild.
with open(path) as f:
    for line in f:
        if line.strip():
            json.loads(line)

print(f"jsonl_updated id={bead_id} status={status} at={ts}")
PY

# Rebuild the DB family from the canonical JSONL (validated + integrity-checked
# inside the rebuild) and install it.
"${REBUILD_SCRIPT}" --install

echo "set_bead_status_via_jsonl: ${BEAD_ID} -> ${STATUS} (verify with: br show ${BEAD_ID})"
