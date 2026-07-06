#!/usr/bin/env bash
set -euo pipefail

# investigate_br_live_write_corruption.sh
#
# Isolation harness for bead rr-lwj2. OBSERVED in the live 538-record Roger
# .beads workspace: `br` 0.1.40 `br close`/`br update` of a pre-existing record
# (first hit on dotted hierarchical ids such as rr-...-ziv8.1) returns
# "Issue not found" AND corrupts .beads/beads.db (rowid out of order /
# free-space corruption, beads_rust#213-class), while creating + closing a
# brand-new tail record (rr-zpc8) stayed clean.
#
# This harness tries to reproduce that in a FRESH minimal workspace to isolate
# the trigger. FINDING (2026-06-19): it does NOT reproduce in isolation — a
# dotted child id, even with a real parent-child dependency edge, closes cleanly
# and keeps sqlite integrity == ok. That rules out "dotted ids are inherently
# broken" and points the root cause at the large live DB's page state rather
# than the id shape. Keep this harness to re-test future `br` builds and to
# guard any candidate fix.
#
# Strategy (no reliance on how `br` mints child ids):
#   1. fresh `br init` temp workspace
#   2. create a non-dotted parent, flush to issues.jsonl
#   3. clone the parent's JSONL record into a DOTTED child id (parent.1) and
#      re-import via `br sync --import-only --rebuild`
#   4. prove the READ path resolves the dotted id (`br show`)
#   5. CONTROL: `br close <parent>` (non-dotted) keeps sqlite integrity == ok
#   6. add a parent-child dep edge, then exercise the WRITE path on the DOTTED
#      id and check sqlite integrity + the "Issue not found" symptom
#
# Exit status (scripts/br-repros convention: exit 1 on a live reproduction):
# exit 1 if the dotted write path corrupts the DB or cannot find the bead in
# this fresh workspace; exit 0 if it stays clean (the expected result today,
# confirming the defect is live-DB/state-specific rather than id-shape-specific).

BR_BIN="${BR_BIN:-br}"
if ! command -v "$BR_BIN" >/dev/null 2>&1 && [[ ! -x "$BR_BIN" ]]; then
  echo "Missing executable BR_BIN: $BR_BIN" >&2
  exit 2
fi
BR_BIN="$(command -v "$BR_BIN" 2>/dev/null || echo "$BR_BIN")"

if ! command -v sqlite3 >/dev/null 2>&1; then
  echo "Missing sqlite3" >&2
  exit 2
fi

tmp=$(mktemp -d /tmp/br-repro-dotted-write.XXXXXX)
cleanup() { rm -rf "$tmp"; }
trap cleanup EXIT
cd "$tmp"

echo "workspace=$tmp"
echo "br_bin=$BR_BIN"

"$BR_BIN" init >/dev/null

parent_id=$("$BR_BIN" create "repro parent" --type task --silent)
echo "parent_id=$parent_id"
dotted_id="${parent_id}.1"
echo "dotted_id=$dotted_id"

# Flush the parent to canonical JSONL, then clone it into a dotted child id.
"$BR_BIN" sync --flush-only >/dev/null 2>&1 || true

PARENT_ID="$parent_id" DOTTED_ID="$dotted_id" python3 - <<'PY'
import json, os
path = ".beads/issues.jsonl"
parent_id = os.environ["PARENT_ID"]
dotted_id = os.environ["DOTTED_ID"]
lines = [l for l in open(path) if l.strip()]
parent = None
for l in lines:
    o = json.loads(l)
    if o.get("id") == parent_id:
        parent = o
        break
if parent is None:
    raise SystemExit(f"parent {parent_id} not found in JSONL")
child = dict(parent)
child["id"] = dotted_id
child["title"] = "repro dotted child"
lines.append(json.dumps(child, sort_keys=True, separators=(",", ":"), ensure_ascii=False) + "\n")
with open(path, "w") as f:
    f.writelines(lines)
print(f"jsonl_cloned dotted={dotted_id}")
PY

"$BR_BIN" sync --import-only --rebuild >/dev/null

# Read path must resolve the dotted id.
if "$BR_BIN" show "$dotted_id" >/dev/null 2>&1; then
  echo "read_path_dotted=ok"
else
  echo "read_path_dotted=FAILED"
fi

# CONTROL: non-dotted write keeps integrity.
control_integrity_before=$(sqlite3 .beads/beads.db "PRAGMA integrity_check;" 2>&1 | head -1)
echo "control_integrity_before=$control_integrity_before"
"$BR_BIN" close "$parent_id" --reason "control non-dotted close" >/dev/null 2>&1 || true
control_integrity_after=$(sqlite3 .beads/beads.db "PRAGMA integrity_check;" 2>&1 | head -1)
echo "control_integrity_after=$control_integrity_after"

# Re-import a clean DB, add a real parent-child dependency edge (the live failing
# bead ziv8.1 had one), then exercise the DOTTED write path. The dep edge is the
# leading suspect for the cascade/blocked-cache update that triggers corruption.
"$BR_BIN" sync --import-only --rebuild >/dev/null
"$BR_BIN" dep add "$dotted_id" "$parent_id" --type parent-child >/dev/null 2>&1 || \
  echo "dep_add_dotted=FAILED"
"$BR_BIN" sync --import-only --rebuild >/dev/null 2>&1 || true
dotted_integrity_before=$(sqlite3 .beads/beads.db "PRAGMA integrity_check;" 2>&1 | head -1)
echo "dotted_integrity_before=$dotted_integrity_before"

write_output=$("$BR_BIN" close "$dotted_id" --reason "dotted close attempt" 2>&1 || true)
echo "dotted_write_output<<EOF"
echo "$write_output"
echo "EOF"

dotted_integrity_after=$(sqlite3 .beads/beads.db "PRAGMA integrity_check;" 2>&1 | head -1)
echo "dotted_integrity_after=$dotted_integrity_after"

bug=0
if [[ "$dotted_integrity_after" != "ok" ]]; then
  echo "dotted_write_corrupted_db=1"
  bug=1
fi
if echo "$write_output" | grep -qi "Issue not found"; then
  echo "dotted_write_issue_not_found=1"
  bug=1
fi

if [[ "$bug" == "1" ]]; then
  echo "RESULT=write_corruption_reproduced_in_fresh_workspace"
  exit 1
fi

echo "RESULT=fresh_workspace_clean (dotted+dep write OK; live-DB corruption is state/scale-specific)"
exit 0
