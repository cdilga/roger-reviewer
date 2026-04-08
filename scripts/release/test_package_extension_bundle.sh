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

bash scripts/release/package_extension_bundle.sh \
  --version-metadata "${workdir}/release-metadata.json" \
  --output-dir "${workdir}/dist"

archive="${workdir}/dist/roger-reviewer-2026.04.01-extension.zip"
manifest="${workdir}/dist/extension-bundle-manifest.json"
verify_json="${workdir}/dist/bridge-verify.json"
pack_json="${workdir}/dist/pack-extension.json"

test -f "${archive}"
test -f "${manifest}"
test -f "${verify_json}"
test -f "${pack_json}"

jq -e '.schema == "roger.release.extension_bundle.v1"' "${manifest}" >/dev/null
jq -e '.lane == "release-package-extension"' "${manifest}" >/dev/null
jq -e '.version == "2026.04.01"' "${manifest}" >/dev/null
jq -e '.tag == "v2026.04.01"' "${manifest}" >/dev/null
jq -e '.channel == "stable"' "${manifest}" >/dev/null
jq -e '.archive_name == "roger-reviewer-2026.04.01-extension.zip"' "${manifest}" >/dev/null
jq -e '.verify_contract_result == "bridge-verify.json"' "${manifest}" >/dev/null
jq -e '.pack_result == "pack-extension.json"' "${manifest}" >/dev/null

jq -e '.outcome == "complete"' "${verify_json}" >/dev/null
jq -e '.outcome == "complete"' "${pack_json}" >/dev/null
jq -e '.data.subcommand == "pack-extension"' "${pack_json}" >/dev/null

python3 - "${archive}" <<'PY'
import pathlib
import sys
import zipfile

archive = pathlib.Path(sys.argv[1])
with zipfile.ZipFile(archive) as zf:
    names = set(zf.namelist())

required = {
    "manifest.json",
    "src/generated/bridge.ts",
    "src/background/main.js",
}
missing = sorted(required - names)
if missing:
    raise SystemExit(f"extension archive missing required entries: {missing}")
PY

echo "test_package_extension_bundle: PASS"
