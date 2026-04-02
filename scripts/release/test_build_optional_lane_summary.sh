#!/usr/bin/env bash
set -euo pipefail

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

cat >"${workdir}/release-metadata.json" <<'EOF'
{
  "channel": "stable",
  "version": "2026.04.01",
  "tag": "v2026.04.01",
  "prerelease": false,
  "artifact_stem": "roger-reviewer-2026.04.01"
}
EOF

python3 scripts/release/build_optional_lane_summary.py \
  --version-metadata "${workdir}/release-metadata.json" \
  --bridge-status built \
  --bridge-artifact bridge-linux.tar.gz \
  --extension-status skipped \
  --output "${workdir}/bridge-only.json"

jq -e '.support_claims.posture == "core_plus_bridge"' "${workdir}/bridge-only.json" >/dev/null
jq -e '.support_claims.narrowed_claims[] | select(. == "extension_sideload_unshipped")' "${workdir}/bridge-only.json" >/dev/null

python3 scripts/release/build_optional_lane_summary.py \
  --version-metadata "${workdir}/release-metadata.json" \
  --bridge-status skipped \
  --extension-status built \
  --extension-artifact extension.zip \
  --output "${workdir}/extension-only.json"

jq -e '.support_claims.posture == "core_only"' "${workdir}/extension-only.json" >/dev/null
jq -e '.support_claims.narrowed_claims[] | select(. == "browser_launch_claim_blocked_without_bridge")' "${workdir}/extension-only.json" >/dev/null

python3 scripts/release/build_optional_lane_summary.py \
  --version-metadata "${workdir}/release-metadata.json" \
  --bridge-status built \
  --bridge-artifact bridge-linux.tar.gz \
  --extension-status built \
  --extension-artifact extension.zip \
  --output "${workdir}/both.json"

jq -e '.support_claims.posture == "core_plus_bridge_plus_extension"' "${workdir}/both.json" >/dev/null
jq -e '.support_claims.narrowed_claims | length == 0' "${workdir}/both.json" >/dev/null

echo "test_build_optional_lane_summary: PASS"
