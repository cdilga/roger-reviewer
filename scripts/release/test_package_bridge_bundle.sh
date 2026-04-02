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

for os_name in linux macos windows; do
  bash scripts/release/package_bridge_bundle.sh \
    --version-metadata "${workdir}/release-metadata.json" \
    --os "${os_name}" \
    --output-dir "${workdir}/dist"

  archive="${workdir}/dist/roger-reviewer-2026.04.01-bridge-${os_name}.tar.gz"
  bundle_dir="${workdir}/dist/roger-reviewer-2026.04.01-bridge-${os_name}"

  test -f "${archive}"
  test -f "${bundle_dir}/bridge-bundle-manifest.json"
  test -f "${bundle_dir}/asset-manifest.json"
  test -f "${bundle_dir}/SHA256SUMS"

  jq -e --arg os_name "${os_name}" '.target_os == $os_name' "${bundle_dir}/bridge-bundle-manifest.json" >/dev/null
  tar -tzf "${archive}" | rg -q 'bridge-bundle-manifest.json'
done

echo "test_package_bridge_bundle: PASS"
