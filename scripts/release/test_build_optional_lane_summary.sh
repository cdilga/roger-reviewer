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

# Backward compatibility: an invocation that omits the skills lane must not
# emit a release-package-skills lane and must not narrow any skills claim.
jq -e '.lanes | has("release-package-skills") | not' "${workdir}/both.json" >/dev/null
jq -e '[.support_claims.narrowed_claims[] | select(. == "skills_bundle_unshipped")] | length == 0' "${workdir}/both.json" >/dev/null

# Optional skills lane, built: appears in lanes + shipped_optional_lanes, no narrow.
python3 scripts/release/build_optional_lane_summary.py \
  --version-metadata "${workdir}/release-metadata.json" \
  --bridge-status built \
  --bridge-artifact bridge-linux.tar.gz \
  --extension-status built \
  --extension-artifact extension.zip \
  --skills-status built \
  --skills-artifact roger-reviewer-2026.04.01-skills.zip \
  --output "${workdir}/with-skills.json"

jq -e '.lanes."release-package-skills".status == "built"' "${workdir}/with-skills.json" >/dev/null
jq -e '.lanes."release-package-skills".artifacts | index("roger-reviewer-2026.04.01-skills.zip")' "${workdir}/with-skills.json" >/dev/null
jq -e '.support_claims.shipped_optional_lanes | index("release-package-skills")' "${workdir}/with-skills.json" >/dev/null
jq -e '[.support_claims.narrowed_claims[] | select(. == "skills_bundle_unshipped")] | length == 0' "${workdir}/with-skills.json" >/dev/null

# Optional skills lane, skipped: lane present but narrowed claim recorded.
python3 scripts/release/build_optional_lane_summary.py \
  --version-metadata "${workdir}/release-metadata.json" \
  --bridge-status built \
  --bridge-artifact bridge-linux.tar.gz \
  --extension-status built \
  --extension-artifact extension.zip \
  --skills-status skipped \
  --output "${workdir}/skills-skipped.json"

jq -e '.lanes."release-package-skills".status == "skipped"' "${workdir}/skills-skipped.json" >/dev/null
jq -e '.support_claims.narrowed_claims[] | select(. == "skills_bundle_unshipped")' "${workdir}/skills-skipped.json" >/dev/null
jq -e '.support_claims.shipped_optional_lanes | index("release-package-skills") | not' "${workdir}/skills-skipped.json" >/dev/null

echo "test_build_optional_lane_summary: PASS"
