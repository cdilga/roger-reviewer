#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Run Roger's stitched deterministic heavyweight E2E lane (E2E-01..E2E-06).

Usage:
  run_stitched_full_surface_e2e.sh [--artifact-root DIR] [--dry-run]

Options:
  --artifact-root DIR   Aggregate artifact bundle root.
                        Default: out/operator-stability/rr-6iah.9-stitched-full-surface-<utc-ts>
  --dry-run             Emit manifest + planned commands without running cargo tests.
  -h, --help            Show this help.

Environment:
  RR_STITCHED_E2E_CARGO_BIN   Cargo binary override (default: cargo).
EOF
}

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)

ARTIFACT_ROOT=""
DRY_RUN=0

while [[ $# -gt 0 ]]; do
  case "${1}" in
    --artifact-root)
      [[ $# -ge 2 ]] || {
        echo "missing value for --artifact-root" >&2
        exit 2
      }
      ARTIFACT_ROOT="$2"
      shift 2
      ;;
    --artifact-root=*)
      ARTIFACT_ROOT="${1#*=}"
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: ${1}" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "${ARTIFACT_ROOT}" ]]; then
  ts="$(date -u +%Y%m%dT%H%M%SZ)"
  ARTIFACT_ROOT="${PROJECT_ROOT}/out/operator-stability/rr-6iah.9-stitched-full-surface-${ts}"
elif [[ "${ARTIFACT_ROOT}" != /* ]]; then
  ARTIFACT_ROOT="${PROJECT_ROOT}/${ARTIFACT_ROOT}"
fi

mkdir -p "${ARTIFACT_ROOT}"
CARGO_BIN="${RR_STITCHED_E2E_CARGO_BIN:-cargo}"
RUN_TS="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

SUITE_IDS=(
  e2e_core_review_happy_path
  e2e_cross_surface_review_continuity
  e2e_tui_first_memory_triage
  e2e_refresh_draft_reconciliation
  e2e_browser_setup_first_launch
  e2e_harness_dropout_return
)

MANIFEST_PATH="${ARTIFACT_ROOT}/00_stitched_run_manifest.json"
{
  echo "{"
  echo "  \"bead_id\": \"rr-6iah.9\","
  echo "  \"run_started_at\": \"${RUN_TS}\","
  echo "  \"artifact_root\": \"${ARTIFACT_ROOT}\","
  echo "  \"stitched_entrypoint\": \"./scripts/swarm/run_stitched_full_surface_e2e.sh\","
  echo "  \"github_read_boundary\": \"fixture_or_double_backed\","
  echo "  \"github_write_boundary\": \"mocked_or_doubled_no_live_posting\","
  echo "  \"browser_launch_boundary\": \"deterministic_extension_loaded_chromium_harness\","
  echo "  \"sacrificial_live_pr_lane_included\": false,"
  echo "  \"suite_ids\": ["
  for i in "${!SUITE_IDS[@]}"; do
    comma=","
    if [[ $i -eq $((${#SUITE_IDS[@]} - 1)) ]]; then
      comma=""
    fi
    printf '    "%s"%s\n' "${SUITE_IDS[$i]}" "${comma}"
  done
  echo "  ]"
  echo "}"
} > "${MANIFEST_PATH}"

printf '%s\n' "${SUITE_IDS[@]}" > "${ARTIFACT_ROOT}/01_stitched_suite_order.txt"

if (( DRY_RUN == 1 )); then
  for suite_id in "${SUITE_IDS[@]}"; do
    echo "[dry-run] ${CARGO_BIN} test -q -p roger-cli --test ${suite_id} -- --nocapture"
  done
  echo "[dry-run] manifest: ${MANIFEST_PATH}"
  exit 0
fi

suite_index=1
for suite_id in "${SUITE_IDS[@]}"; do
  log_path="$(printf '%s/%02d_cargo_test_%s.txt' "${ARTIFACT_ROOT}" "${suite_index}" "${suite_id}")"
  echo "running ${suite_id} ..."
  "${CARGO_BIN}" test -q -p roger-cli --test "${suite_id}" -- --nocapture 2>&1 | tee "${log_path}"
  suite_index=$((suite_index + 1))
done

{
  echo "bead_id=rr-6iah.9"
  echo "status=pass"
  echo "run_finished_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "artifact_root=${ARTIFACT_ROOT}"
  echo "suite_count=${#SUITE_IDS[@]}"
  echo "github_read_boundary=fixture_or_double_backed"
  echo "github_write_boundary=mocked_or_doubled_no_live_posting"
  echo "browser_launch_boundary=deterministic_extension_loaded_chromium_harness"
  echo "sacrificial_live_pr_lane_included=false"
} > "${ARTIFACT_ROOT}/99_stitched_run_summary.txt"

echo "stitched deterministic E2E run complete; artifacts: ${ARTIFACT_ROOT}"
