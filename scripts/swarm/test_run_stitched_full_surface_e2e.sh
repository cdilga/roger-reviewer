#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
WORKDIR="$(mktemp -d "${TMPDIR:-/tmp}/rr-stitched-e2e-test.XXXXXX")"
trap 'rm -rf "$WORKDIR"' EXIT

FAKE_BIN="${WORKDIR}/bin"
mkdir -p "${FAKE_BIN}"

cat >"${FAKE_BIN}/cargo" <<'CARGO'
#!/usr/bin/env bash
set -euo pipefail
echo "$*" >>"${RR_FAKE_CARGO_LOG}"
echo "fake cargo ok: $*"
CARGO
chmod +x "${FAKE_BIN}/cargo"

SCRIPT_UNDER_TEST="${WORKDIR}/run_stitched_full_surface_e2e.sh"
cp "${SCRIPT_DIR}/run_stitched_full_surface_e2e.sh" "${SCRIPT_UNDER_TEST}"
chmod +x "${SCRIPT_UNDER_TEST}"

OUT_DIR="${WORKDIR}/stitched-out"
export RR_FAKE_CARGO_LOG="${WORKDIR}/cargo_calls.log"

RR_STITCHED_E2E_CARGO_BIN="${FAKE_BIN}/cargo" \
  "${SCRIPT_UNDER_TEST}" --artifact-root "${OUT_DIR}" >/dev/null

test -f "${OUT_DIR}/00_stitched_run_manifest.json"
test -f "${OUT_DIR}/99_stitched_run_summary.txt"
test -f "${OUT_DIR}/01_stitched_suite_order.txt"

grep -q '"github_write_boundary": "mocked_or_doubled_no_live_posting"' \
  "${OUT_DIR}/00_stitched_run_manifest.json"
grep -q '^status=pass$' "${OUT_DIR}/99_stitched_run_summary.txt"

test_count="$(wc -l < "${RR_FAKE_CARGO_LOG}" | tr -d '[:space:]')"
[[ "${test_count}" == "6" ]]

for suite_id in \
  e2e_core_review_happy_path \
  e2e_cross_surface_review_continuity \
  e2e_tui_first_memory_triage \
  e2e_refresh_draft_reconciliation \
  e2e_browser_setup_first_launch \
  e2e_harness_dropout_return
do
  grep -q -- "--test ${suite_id}" "${RR_FAKE_CARGO_LOG}"
done

echo "stitched full-surface E2E runner script test passed."
