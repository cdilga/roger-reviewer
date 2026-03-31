#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT_PATH="${ROOT_DIR}/scripts/release/derive_calver_version.py"

if [[ ! -x "${SCRIPT_PATH}" ]]; then
  echo "missing executable: ${SCRIPT_PATH}" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

run_case() {
  local case_name="$1"
  local ref="$2"
  local sha="$3"
  local today="$4"
  local expected_channel="$5"
  local expected_version="$6"
  local expected_tag="$7"
  local expected_prerelease="$8"
  local expected_promotable="$9"

  local output_path="${TMP_DIR}/${case_name}.json"

  python3 "${SCRIPT_PATH}" \
    --ref "${ref}" \
    --sha "${sha}" \
    --today "${today}" \
    --workspace-version "0.1.0" >"${output_path}"

  jq -e --arg expected "${expected_channel}" '.channel == $expected' "${output_path}" >/dev/null
  jq -e --arg expected "${expected_version}" '.version == $expected' "${output_path}" >/dev/null
  jq -e --arg expected "${expected_tag}" '.tag == $expected' "${output_path}" >/dev/null
  jq -e --argjson expected "${expected_prerelease}" '.prerelease == $expected' "${output_path}" >/dev/null
  jq -e --argjson expected "${expected_promotable}" '.promotable == $expected' "${output_path}" >/dev/null
  jq -e '.workspace_version == "0.1.0"' "${output_path}" >/dev/null
  jq -e '.artifact_stem | startswith("roger-reviewer-")' "${output_path}" >/dev/null
}

run_case \
  "stable" \
  "refs/tags/v2026.03.31" \
  "0123456789abcdef0123456789abcdef01234567" \
  "2026.04.01" \
  "stable" \
  "2026.03.31" \
  "v2026.03.31" \
  false \
  true

run_case \
  "rc" \
  "refs/tags/v2026.03.31-rc.2" \
  "89abcdef0123456789abcdef0123456789abcdef" \
  "2026.04.01" \
  "rc" \
  "2026.03.31-rc.2" \
  "v2026.03.31-rc.2" \
  true \
  true

run_case \
  "nightly" \
  "refs/heads/main" \
  "fedcba9876543210fedcba9876543210fedcba98" \
  "2026.04.01" \
  "nightly" \
  "2026.04.01-nightly.fedcba987654" \
  "nightly-2026.04.01-fedcba987654" \
  true \
  false

# Determinism check on synthetic ref input.
python3 "${SCRIPT_PATH}" \
  --ref "refs/heads/main" \
  --sha "fedcba9876543210fedcba9876543210fedcba98" \
  --today "2026.04.01" \
  --workspace-version "0.1.0" >"${TMP_DIR}/determinism-a.json"
python3 "${SCRIPT_PATH}" \
  --ref "refs/heads/main" \
  --sha "fedcba9876543210fedcba9876543210fedcba98" \
  --today "2026.04.01" \
  --workspace-version "0.1.0" >"${TMP_DIR}/determinism-b.json"
diff -u "${TMP_DIR}/determinism-a.json" "${TMP_DIR}/determinism-b.json" >/dev/null

# Invalid tag should fail closed.
if python3 "${SCRIPT_PATH}" --ref "refs/tags/v2026.3.1" --sha "0123456789abcdef" --today "2026.04.01" >/dev/null 2>&1; then
  echo "expected malformed tag to fail" >&2
  exit 1
fi

echo "derive_calver_version: ok"
