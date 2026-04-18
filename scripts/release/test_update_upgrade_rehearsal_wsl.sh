#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: test_update_upgrade_rehearsal_wsl.sh [--output-dir <path>]

Run the dedicated WSL install/update rehearsal command.

- When executed inside WSL, this runs the deterministic upgrade rehearsal
  through rr-install.sh and rr update, then records a pass/fail summary.
- When executed outside WSL, this emits a blocked summary with
  reason_code=wsl_host_required so release claims stay narrowed.

Options:
  --output-dir <path>   Directory for rehearsal artifacts (default: mktemp dir)
  -h, --help            Show this help
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

is_wsl_host() {
  if [[ "$(uname -s)" != "Linux" ]]; then
    return 1
  fi
  if grep -qiE "(microsoft|wsl)" /proc/version 2>/dev/null; then
    return 0
  fi
  if [[ -n "${WSL_INTEROP:-}" || -n "${WSL_DISTRO_NAME:-}" ]]; then
    return 0
  fi
  return 1
}

write_summary() {
  local path="$1"
  local status="$2"
  local reason_code="$3"
  local manifest_path="$4"
  local host_environment="$5"
  local run_utc="$6"

  python3 - "$path" "$status" "$reason_code" "$manifest_path" "$host_environment" "$run_utc" <<'PY'
import json
import os
import pathlib
import sys

path, status, reason_code, manifest_path, host_environment, run_utc = sys.argv[1:7]

if manifest_path == "":
    manifest_path = None
if reason_code == "none":
    reason_code = None

run_url = None
repo = os.environ.get("GITHUB_REPOSITORY")
run_id = os.environ.get("GITHUB_RUN_ID")
if repo and run_id:
    run_url = f"https://github.com/{repo}/actions/runs/{run_id}"

payload = {
    "schema": "roger.update.wsl-install-update-rehearsal.v1",
    "lane": "wsl-install-update-rehearsal",
    "status": status,
    "reason_code": reason_code,
    "host_environment": host_environment,
    "run_utc": run_utc,
    "rehearsal_manifest_path": manifest_path,
    "run_provenance": {
        "runner_kind": "github_actions" if run_url else "local",
        "run_url": run_url,
        "workspace": os.getcwd(),
    },
}

pathlib.Path(path).write_text(
    json.dumps(payload, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
}

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BASE_REHEARSAL_SCRIPT="${ROOT_DIR}/scripts/release/test_update_upgrade_rehearsal.sh"
[[ -f "${BASE_REHEARSAL_SCRIPT}" ]] || die "missing script: ${BASE_REHEARSAL_SCRIPT}"

OUTPUT_DIR=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --output-dir)
      [[ $# -ge 2 ]] || die "--output-dir requires a path"
      OUTPUT_DIR="$2"
      shift 2
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

need_cmd bash
need_cmd python3
need_cmd date
need_cmd uname

WORKDIR="$(mktemp -d)"
cleanup() {
  local status=$?
  if [[ $status -eq 0 ]]; then
    rm -rf "${WORKDIR}"
  else
    echo "wsl rehearsal artifacts preserved at ${WORKDIR}" >&2
  fi
}
trap cleanup EXIT

if [[ -n "${OUTPUT_DIR}" ]]; then
  mkdir -p "${OUTPUT_DIR}"
  ARTIFACT_DIR="${OUTPUT_DIR}"
else
  ARTIFACT_DIR="${WORKDIR}/out"
  mkdir -p "${ARTIFACT_DIR}"
fi

SUMMARY_PATH="${ARTIFACT_DIR}/wsl-install-update-rehearsal-summary.json"
RUN_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
HOST_ENVIRONMENT="native"
if is_wsl_host; then
  HOST_ENVIRONMENT="wsl"
fi

if [[ "${HOST_ENVIRONMENT}" != "wsl" ]]; then
  write_summary "${SUMMARY_PATH}" "blocked" "wsl_host_required" "" "${HOST_ENVIRONMENT}" "${RUN_UTC}"
  echo "test_update_upgrade_rehearsal_wsl: BLOCKED (reason_code=wsl_host_required)"
  echo "summary: ${SUMMARY_PATH}"
  exit 0
fi

if bash "${BASE_REHEARSAL_SCRIPT}" --output-dir "${ARTIFACT_DIR}"; then
  MANIFEST_PATH="${ARTIFACT_DIR}/update-upgrade-rehearsal-manifest.json"
  [[ -f "${MANIFEST_PATH}" ]] || die "missing rehearsal manifest: ${MANIFEST_PATH}"
  write_summary "${SUMMARY_PATH}" "pass" "none" "${MANIFEST_PATH}" "${HOST_ENVIRONMENT}" "${RUN_UTC}"
  echo "test_update_upgrade_rehearsal_wsl: PASS"
  echo "summary: ${SUMMARY_PATH}"
  exit 0
fi

MANIFEST_PATH="${ARTIFACT_DIR}/update-upgrade-rehearsal-manifest.json"
if [[ ! -f "${MANIFEST_PATH}" ]]; then
  MANIFEST_PATH=""
fi
write_summary "${SUMMARY_PATH}" "failed" "wsl_rehearsal_command_failed" "${MANIFEST_PATH}" "${HOST_ENVIRONMENT}" "${RUN_UTC}"
echo "test_update_upgrade_rehearsal_wsl: FAIL (reason_code=wsl_rehearsal_command_failed)" >&2
echo "summary: ${SUMMARY_PATH}" >&2
exit 1
