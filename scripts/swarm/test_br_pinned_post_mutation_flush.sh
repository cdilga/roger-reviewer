#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/rr-br-pinned-flush.XXXXXX")"
trap 'rm -rf "${WORKDIR}"' EXIT

FAKE_BIN_DIR="${WORKDIR}/bin"
FAKE_SCRIPT_DIR="${WORKDIR}/scripts"
REPO_DIR="${WORKDIR}/repo"
mkdir -p "${FAKE_BIN_DIR}" "${FAKE_SCRIPT_DIR}" "${REPO_DIR}/.beads"

cp "${SCRIPT_DIR}/br_pinned.sh" "${FAKE_SCRIPT_DIR}/br_pinned.sh"
chmod +x "${FAKE_SCRIPT_DIR}/br_pinned.sh"

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
  update)
    mkdir -p .beads
    printf 'rr-test|in_progress\n' > .beads/db-state.txt
    echo "updated rr-test"
    ;;
  sync)
    if [[ "${1:-}" != "--flush-only" ]]; then
      echo "unexpected sync args: $*" >&2
      exit 2
    fi
    cp .beads/db-state.txt .beads/issues.jsonl
    : > .beads/flush-ran.marker
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

if [[ -z "${beads_dir}" ]]; then
  beads_dir="${PWD}/.beads"
fi

db_state="$(cat "${beads_dir}/db-state.txt" 2>/dev/null || true)"
jsonl_state="$(cat "${beads_dir}/issues.jsonl" 2>/dev/null || true)"

if [[ -n "${db_state}" && "${db_state}" != "${jsonl_state}" ]]; then
  echo "TRUST_STATUS=fail"
  echo "TRUST_REASON=count mismatch between db and jsonl"
  exit 1
fi

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

: > "${REPO_DIR}/.beads/beads.db"
printf 'rr-test|open\n' > "${REPO_DIR}/.beads/issues.jsonl"

pushd "${REPO_DIR}" >/dev/null
set +e
output="$(
  BR_PINNED_DIR="${FAKE_BIN_DIR}" \
  BR_PINNED_BIN="${FAKE_BIN_DIR}/br-0.1.40.pinned" \
  BR_DEFAULT_LINK="${FAKE_BIN_DIR}/br" \
  RR_BR_SERIALIZE_WRITES=0 \
  "${FAKE_SCRIPT_DIR}/br_pinned.sh" update rr-test --status in_progress 2>&1
)"
status=$?
set -e
popd >/dev/null

[[ "${status}" -eq 0 ]]
grep -q "updated rr-test" <<<"${output}"
[[ -f "${REPO_DIR}/.beads/flush-ran.marker" ]]
[[ ! -f "${REPO_DIR}/.beads/rebuild-ran.marker" ]]
grep -q '^rr-test|in_progress$' "${REPO_DIR}/.beads/issues.jsonl"
if grep -q "post-mutation trust check failed" <<<"${output}"; then
  echo "healthy mutation should not trigger post-mutation rebuild" >&2
  exit 1
fi

printf 'br_pinned post-mutation flush regression passed.\n'
