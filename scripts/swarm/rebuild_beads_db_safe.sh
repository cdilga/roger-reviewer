#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
TRUST_CHECK="${SCRIPT_DIR}/check_beads_trust.sh"

DEFAULT_BEADS_DIR="${PROJECT_ROOT}/.beads"
BEADS_DIR="${DEFAULT_BEADS_DIR}"
INSTALL=0
KEEP_TEMP=0
BR_BIN=""
TMP_ROOT=""

usage() {
  cat <<'EOF'
Usage: rebuild_beads_db_safe.sh [--beads-dir <path>] [--br <path>] [--install] [--keep-temp]

Rebuild the beads SQLite DB from canonical issues.jsonl without using
`br sync --import-only --rebuild`.

What it does:
1. creates a fresh temporary workspace
2. copies canonical issues.jsonl + config.yaml into it
3. runs `br sync --import-only` against the fresh DB
4. validates the rebuilt DB with native sqlite3 + check_beads_trust.sh
5. optionally installs the rebuilt DB family into the live .beads directory

By default this is non-destructive and leaves the rebuilt candidate in /tmp.
Pass --install to replace the live DB family after validation.
EOF
}

die() {
  echo "rebuild_beads_db_safe: $*" >&2
  exit 2
}

cleanup() {
  if [[ -n "${TMP_ROOT}" && "${KEEP_TEMP}" -eq 0 ]]; then
    rm -rf "${TMP_ROOT}"
  fi
}
trap cleanup EXIT

while [[ $# -gt 0 ]]; do
  case "$1" in
    --beads-dir)
      [[ $# -ge 2 ]] || die "missing value for --beads-dir"
      BEADS_DIR="$2"
      shift 2
      ;;
    --br)
      [[ $# -ge 2 ]] || die "missing value for --br"
      BR_BIN="$2"
      shift 2
      ;;
    --install)
      INSTALL=1
      shift
      ;;
    --keep-temp)
      KEEP_TEMP=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

[[ -d "${BEADS_DIR}" ]] || die "missing beads directory: ${BEADS_DIR}"
[[ -f "${BEADS_DIR}/issues.jsonl" ]] || die "missing canonical JSONL: ${BEADS_DIR}/issues.jsonl"
[[ -f "${BEADS_DIR}/config.yaml" ]] || die "missing config.yaml: ${BEADS_DIR}/config.yaml"
command -v sqlite3 >/dev/null 2>&1 || die "sqlite3 is required"

if [[ -z "${BR_BIN}" ]]; then
  BR_BIN="${RR_BR_BIN:-$(command -v br 2>/dev/null || true)}"
fi
[[ -x "${BR_BIN}" ]] || die "br binary is not executable: ${BR_BIN}"

TMP_ROOT=$(mktemp -d /tmp/rr-beads-safe-rebuild.XXXXXX)
WORK_DIR="${TMP_ROOT}/work"
mkdir -p "${WORK_DIR}"

(
  cd "${WORK_DIR}"
  "${BR_BIN}" init >/dev/null
  cp "${BEADS_DIR}/issues.jsonl" .beads/issues.jsonl
  cp "${BEADS_DIR}/config.yaml" .beads/config.yaml
  "${BR_BIN}" sync --import-only >/tmp/rr-beads-safe-rebuild.out 2>/tmp/rr-beads-safe-rebuild.err
)

printf 'rebuild_workspace=%s\n' "${WORK_DIR}"
printf 'br_version=%s\n' "$("${BR_BIN}" --version)"
printf 'import_stdout_start\n'
cat /tmp/rr-beads-safe-rebuild.out
printf 'import_stdout_end\n'
printf 'import_stderr_start\n'
cat /tmp/rr-beads-safe-rebuild.err
printf 'import_stderr_end\n'

candidate_db="${WORK_DIR}/.beads/beads.db"
candidate_jsonl="${WORK_DIR}/.beads/issues.jsonl"

integrity=$(sqlite3 "${candidate_db}" "PRAGMA integrity_check;")
[[ "${integrity}" == "ok" ]] || die "candidate integrity_check failed: ${integrity}"

fk_check=$(sqlite3 "${candidate_db}" "PRAGMA foreign_key_check;")
[[ -z "${fk_check}" ]] || die "candidate foreign_key_check failed: ${fk_check}"

"${TRUST_CHECK}" --db "${candidate_db}" --jsonl "${candidate_jsonl}" --skip-br-doctor

if [[ "${INSTALL}" -eq 0 ]]; then
  cat <<EOF
rebuild_beads_db_safe: candidate DB family is valid and staged at:
  ${WORK_DIR}/.beads
rebuild_beads_db_safe: rerun with --install to replace the live DB family
EOF
  exit 0
fi

stamp=$(date -u +%Y%m%d_%H%M%S)
backup_dir="${BEADS_DIR}/.br_recovery/safe_rebuild_${stamp}"
mkdir -p "${backup_dir}"

restore_backup() {
  rm -f "${BEADS_DIR}/beads.db" "${BEADS_DIR}/beads.db-wal" "${BEADS_DIR}/beads.db-shm" "${BEADS_DIR}/beads.db-journal"
  local path
  for path in "${backup_dir}"/beads.db*; do
    [[ -e "${path}" ]] || continue
    mv "${path}" "${BEADS_DIR}/"
  done
}

for suffix in "" "-wal" "-shm" "-journal"; do
  live_path="${BEADS_DIR}/beads.db${suffix}"
  if [[ -e "${live_path}" ]]; then
    mv "${live_path}" "${backup_dir}/"
  fi
done

for suffix in "" "-wal" "-shm" "-journal"; do
  staged_path="${WORK_DIR}/.beads/beads.db${suffix}"
  if [[ -e "${staged_path}" ]]; then
    cp -a "${staged_path}" "${BEADS_DIR}/"
  fi
done

if ! "${TRUST_CHECK}" --db "${BEADS_DIR}/beads.db" --jsonl "${BEADS_DIR}/issues.jsonl" --skip-br-doctor >/tmp/rr-beads-safe-install-trust.out 2>/tmp/rr-beads-safe-install-trust.err; then
  cat /tmp/rr-beads-safe-install-trust.out >&2 || true
  cat /tmp/rr-beads-safe-install-trust.err >&2 || true
  echo "rebuild_beads_db_safe: installed DB failed validation; restoring backup" >&2
  restore_backup
  exit 1
fi

cat <<EOF
rebuild_beads_db_safe: installed validated DB family into ${BEADS_DIR}
rebuild_beads_db_safe: previous DB family backed up at ${backup_dir}
EOF
