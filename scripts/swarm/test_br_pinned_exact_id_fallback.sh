#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/rr-br-pinned-exact-id.XXXXXX")"
trap 'rm -rf "${WORKDIR}"' EXIT

FAKE_BIN_DIR="${WORKDIR}/bin"
FAKE_SCRIPT_DIR="${WORKDIR}/scripts"
REPO_DIR="${WORKDIR}/repo"
mkdir -p "${FAKE_BIN_DIR}" "${FAKE_SCRIPT_DIR}" "${REPO_DIR}/.beads"

cp "${SCRIPT_DIR}/br_pinned.sh" "${FAKE_SCRIPT_DIR}/br_pinned.sh"
cp "${SCRIPT_DIR}/exact_id_status_fallback.py" "${FAKE_SCRIPT_DIR}/exact_id_status_fallback.py"
chmod +x "${FAKE_SCRIPT_DIR}/br_pinned.sh" "${FAKE_SCRIPT_DIR}/exact_id_status_fallback.py"

cat >"${FAKE_BIN_DIR}/br-0.1.40.pinned" <<'BR'
#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "--version" ]]; then
  echo "br 0.1.40"
  exit 0
fi

cmd="${1:-}"
shift || true
case "${cmd}" in
  close)
    if [[ "${1:-}" == "--help" ]]; then
      echo "Close help"
      exit 0
    fi
    echo "Error: Ambiguous ID 'rr-h8bb': matches [\"rr-h8bb\", \"rr-h8bb.1\"]" >&2
    exit 3
    ;;
  update)
    echo '{"error":{"code":"ISSUE_NOT_FOUND","message":"Issue not found: rr-h8bb"}}'
    exit 3
    ;;
  *)
    echo "unexpected fake br command: ${cmd} $*" >&2
    exit 2
    ;;
esac
BR
chmod +x "${FAKE_BIN_DIR}/br-0.1.40.pinned"

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

beads_dir=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --beads-dir)
      beads_dir="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done

[[ -n "${beads_dir}" ]]
: > "${beads_dir}/rebuild-ran.marker"
echo "synthetic rebuild completed for ${beads_dir}" >&2
REBUILD
chmod +x "${FAKE_SCRIPT_DIR}/rebuild_beads_db_safe.sh"

cat >"${REPO_DIR}/.beads/config.yaml" <<'CFG'
repo: .
CFG

cat >"${REPO_DIR}/.beads/issues.jsonl" <<'JSONL'
{"id":"rr-h8bb","title":"Release truth lane","status":"open","priority":0,"issue_type":"epic","created_at":"2026-04-14T20:41:14Z","updated_at":"2026-04-16T23:43:13Z","source_repo":"."}
{"id":"rr-h8bb.1","title":"Release truth child","status":"closed","priority":1,"issue_type":"task","created_at":"2026-04-14T20:41:14Z","updated_at":"2026-04-16T23:43:13Z","source_repo":"."}
JSONL

pushd "${REPO_DIR}" >/dev/null

set +e
help_output="$(
  BR_PINNED_DIR="${FAKE_BIN_DIR}" \
  BR_PINNED_BIN="${FAKE_BIN_DIR}/br-0.1.40.pinned" \
  BR_DEFAULT_LINK="${FAKE_BIN_DIR}/br" \
  RR_BR_SERIALIZE_WRITES=0 \
  "${FAKE_SCRIPT_DIR}/br_pinned.sh" close --help 2>&1
)"
help_status=$?
set -e
[[ "${help_status}" -eq 0 ]]
grep -q "Close help" <<<"${help_output}"
[[ ! -f "${REPO_DIR}/.beads/rebuild-ran.marker" ]]

set +e
close_output="$(
  BR_PINNED_DIR="${FAKE_BIN_DIR}" \
  BR_PINNED_BIN="${FAKE_BIN_DIR}/br-0.1.40.pinned" \
  BR_DEFAULT_LINK="${FAKE_BIN_DIR}/br" \
  RR_BR_SERIALIZE_WRITES=0 \
  "${FAKE_SCRIPT_DIR}/br_pinned.sh" close rr-h8bb --reason "validated close" 2>&1
)"
close_status=$?
set -e
[[ "${close_status}" -eq 0 ]]
grep -q "applying canonical JSONL fallback" <<<"${close_output}"
grep -q "synthetic rebuild completed" <<<"${close_output}"
[[ -f "${REPO_DIR}/.beads/rebuild-ran.marker" ]]
grep -q '"id":"rr-h8bb","title":"Release truth lane","status":"closed"' "${REPO_DIR}/.beads/issues.jsonl"
grep -q '"close_reason":"validated close"' "${REPO_DIR}/.beads/issues.jsonl"

rm -f "${REPO_DIR}/.beads/rebuild-ran.marker"

set +e
update_output="$(
  BR_PINNED_DIR="${FAKE_BIN_DIR}" \
  BR_PINNED_BIN="${FAKE_BIN_DIR}/br-0.1.40.pinned" \
  BR_DEFAULT_LINK="${FAKE_BIN_DIR}/br" \
  RR_BR_SERIALIZE_WRITES=0 \
  "${FAKE_SCRIPT_DIR}/br_pinned.sh" update rr-h8bb --status in_progress --json 2>&1
)"
update_status=$?
set -e
[[ "${update_status}" -eq 0 ]]
grep -q 'canonical_jsonl_exact_id' <<<"${update_output}"
[[ -f "${REPO_DIR}/.beads/rebuild-ran.marker" ]]
grep -q '"id":"rr-h8bb","title":"Release truth lane","status":"in_progress"' "${REPO_DIR}/.beads/issues.jsonl"
if grep -q '"close_reason"' "${REPO_DIR}/.beads/issues.jsonl"; then
  echo "expected reopen/update fallback to clear close metadata" >&2
  exit 1
fi

popd >/dev/null

printf 'br_pinned exact-id fallback regression passed.\n'
