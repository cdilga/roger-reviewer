#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/rr-br-pinned-test.XXXXXX")"
trap 'rm -rf "$WORKDIR"' EXIT

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

if [[ "${1:-}" == "update" ]]; then
  mkdir -p .beads
  : > .beads/partial-mutation.marker
  echo "synthetic br mutation failure" >&2
  exit 3
fi

echo "unexpected fake br command: $*" >&2
exit 2
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

if [[ -f "${beads_dir}/partial-mutation.marker" ]]; then
  echo "TRUST_STATUS=fail"
  echo "TRUST_REASON=synthetic corruption"
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

if [[ -z "${beads_dir}" ]]; then
  echo "missing beads dir" >&2
  exit 2
fi

rm -f "${beads_dir}/partial-mutation.marker"
: > "${beads_dir}/repair-ran.marker"
echo "synthetic rebuild completed for ${beads_dir}" >&2
REBUILD
chmod +x "${FAKE_SCRIPT_DIR}/rebuild_beads_db_safe.sh"

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

[[ "${status}" -eq 3 ]]
grep -q "synthetic br mutation failure" <<<"${output}"
grep -q "post-mutation trust check failed (synthetic corruption); rebuilding DB from canonical JSONL" <<<"${output}"
[[ -f "${REPO_DIR}/.beads/repair-ran.marker" ]]
[[ ! -f "${REPO_DIR}/.beads/partial-mutation.marker" ]]

printf 'br_pinned failed-mutation repair regression passed.\n'
