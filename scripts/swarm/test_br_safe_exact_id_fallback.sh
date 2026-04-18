#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/rr-br-safe-test.XXXXXX")"
trap 'rm -rf "${WORKDIR}"' EXIT

FAKE_SCRIPT_DIR="${WORKDIR}/scripts/swarm"
REPO_DIR="${WORKDIR}/repo"
mkdir -p "${FAKE_SCRIPT_DIR}" "${REPO_DIR}/.beads"

cp "${SCRIPT_DIR}/br_safe.sh" "${FAKE_SCRIPT_DIR}/br_safe.sh"
chmod +x "${FAKE_SCRIPT_DIR}/br_safe.sh"

cat >"${FAKE_SCRIPT_DIR}/br_pinned.sh" <<'BR'
#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "close" ]]; then
  echo "Error: Issue not found: rr-test" >&2
  exit 3
fi

if [[ "${1:-}" == "update" ]]; then
  echo "Error: Issue not found: rr-test" >&2
  exit 3
fi

echo "unexpected fake br command: $*" >&2
exit 2
BR
chmod +x "${FAKE_SCRIPT_DIR}/br_pinned.sh"

cat >"${FAKE_SCRIPT_DIR}/check_beads_trust.sh" <<'TRUST'
#!/usr/bin/env bash
set -euo pipefail
echo "TRUST_STATUS=pass"
echo "TRUST_REASON=synthetic trust clean"
TRUST
chmod +x "${FAKE_SCRIPT_DIR}/check_beads_trust.sh"

cat >"${FAKE_SCRIPT_DIR}/rebuild_beads_db_safe.sh" <<'REBUILD'
#!/usr/bin/env bash
set -euo pipefail

if [[ ! -f ".beads/issues.jsonl" ]]; then
  echo "missing issues.jsonl" >&2
  exit 2
fi

jq -r 'select(.id == "rr-test") | .status' .beads/issues.jsonl > .beads/status.marker
jq -r 'select(.id == "rr-test") | .close_reason // ""' .beads/issues.jsonl > .beads/reason.marker
echo "synthetic rebuild ok" >&2
REBUILD
chmod +x "${FAKE_SCRIPT_DIR}/rebuild_beads_db_safe.sh"

cat >"${REPO_DIR}/.beads/issues.jsonl" <<'JSONL'
{"id":"rr-test","status":"open","title":"Synthetic issue"}
JSONL

pushd "${REPO_DIR}" >/dev/null
close_output="$("${FAKE_SCRIPT_DIR}/br_safe.sh" close rr-test --reason "closed via fallback" 2>&1)"
[[ "$(cat .beads/status.marker)" == "closed" ]]
[[ "$(cat .beads/reason.marker)" == "closed via fallback" ]]
grep -q "recovered close for canonical issue rr-test via JSONL fallback" <<<"${close_output}"

cat >"${REPO_DIR}/.beads/issues.jsonl" <<'JSONL'
{"id":"rr-test","status":"open","title":"Synthetic issue"}
JSONL

update_output="$("${FAKE_SCRIPT_DIR}/br_safe.sh" update rr-test --status in_progress 2>&1)"
[[ "$(cat .beads/status.marker)" == "in_progress" ]]
[[ "$(cat .beads/reason.marker)" == "" ]]
grep -q "recovered update for canonical issue rr-test via JSONL fallback" <<<"${update_output}"
popd >/dev/null

printf 'br_safe failed issue-resolution fallback regression passed.\n'
