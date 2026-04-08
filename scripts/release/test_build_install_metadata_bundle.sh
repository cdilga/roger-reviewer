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
  "artifact_stem": "roger-reviewer-2026.04.01",
  "workspace_version": "0.1.0",
  "provenance": {
    "source_ref": "refs/tags/v2026.04.01",
    "source_sha": "1111111111111111111111111111111111111111"
  }
}
EOF

cat >"${workdir}/release-core-manifest-2026.04.01.json" <<'EOF'
{
  "schema": "roger.release-build-core.v1",
  "channel": "stable",
  "version": "2026.04.01",
  "tag": "v2026.04.01",
  "prerelease": false,
  "artifact_stem": "roger-reviewer-2026.04.01",
  "targets": [
    {
      "target": "x86_64-unknown-linux-gnu",
      "archive_name": "roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz",
      "archive_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "payload_dir": "roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu",
      "binary_name": "rr",
      "runner_os": "Linux"
    },
    {
      "target": "x86_64-pc-windows-msvc",
      "archive_name": "roger-reviewer-2026.04.01-core-x86_64-pc-windows-msvc.tar.gz",
      "archive_sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
      "payload_dir": "roger-reviewer-2026.04.01-core-x86_64-pc-windows-msvc",
      "binary_name": "rr.exe",
      "runner_os": "Windows"
    }
  ],
  "store_compatibility": {
    "envelope_version": 1,
    "store_schema_version": 10,
    "min_supported_store_schema": 0,
    "auto_migrate_from": 0,
    "migration_policy": "binary_only",
    "migration_class_max_auto": "none",
    "sidecar_generation": "v1",
    "backup_required": true
  }
}
EOF

python3 scripts/release/build_install_metadata_bundle.py \
  --version-metadata "${workdir}/release-metadata.json" \
  --core-manifest "${workdir}/release-core-manifest-2026.04.01.json" \
  --output "${workdir}/release-install-metadata-2026.04.01.json"

jq -e '.schema == "roger.release.install-metadata.v1"' \
  "${workdir}/release-install-metadata-2026.04.01.json" >/dev/null
jq -e '.release.version == "2026.04.01"' \
  "${workdir}/release-install-metadata-2026.04.01.json" >/dev/null
jq -e '.checksums_name == "roger-reviewer-2026.04.01-checksums.txt"' \
  "${workdir}/release-install-metadata-2026.04.01.json" >/dev/null
jq -e '.core_manifest_name == "release-core-manifest-2026.04.01.json"' \
  "${workdir}/release-install-metadata-2026.04.01.json" >/dev/null
jq -e '.targets | length == 2' \
  "${workdir}/release-install-metadata-2026.04.01.json" >/dev/null
jq -e '.targets[] | select(.target == "x86_64-pc-windows-msvc")' \
  "${workdir}/release-install-metadata-2026.04.01.json" >/dev/null
jq -e '.store_compatibility.migration_policy == "binary_only"' \
  "${workdir}/release-install-metadata-2026.04.01.json" >/dev/null
jq -e '.store_compatibility.store_schema_version == 10' \
  "${workdir}/release-install-metadata-2026.04.01.json" >/dev/null
jq -e '.store_compatibility.backup_required == true' \
  "${workdir}/release-install-metadata-2026.04.01.json" >/dev/null

cat >"${workdir}/mismatch-core-manifest.json" <<'EOF'
{
  "schema": "roger.release-build-core.v1",
  "channel": "stable",
  "version": "2026.04.02",
  "tag": "v2026.04.02",
  "prerelease": false,
  "artifact_stem": "roger-reviewer-2026.04.02",
  "targets": []
}
EOF

if python3 scripts/release/build_install_metadata_bundle.py \
  --version-metadata "${workdir}/release-metadata.json" \
  --core-manifest "${workdir}/mismatch-core-manifest.json" \
  --output "${workdir}/should-fail.json" >/dev/null 2>&1; then
  echo "expected mismatch core manifest generation to fail" >&2
  exit 1
fi

cat >"${workdir}/invalid-compat-core-manifest.json" <<'EOF'
{
  "schema": "roger.release-build-core.v1",
  "channel": "stable",
  "version": "2026.04.01",
  "tag": "v2026.04.01",
  "prerelease": false,
  "artifact_stem": "roger-reviewer-2026.04.01",
  "targets": [
    {
      "target": "x86_64-unknown-linux-gnu",
      "archive_name": "roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz",
      "archive_sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      "payload_dir": "roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu",
      "binary_name": "rr",
      "runner_os": "Linux"
    }
  ],
  "store_compatibility": {
    "envelope_version": 1,
    "store_schema_version": 10,
    "min_supported_store_schema": 0,
    "auto_migrate_from": 0,
    "migration_policy": "bad-policy",
    "migration_class_max_auto": "none",
    "sidecar_generation": "v1",
    "backup_required": true
  }
}
EOF

if python3 scripts/release/build_install_metadata_bundle.py \
  --version-metadata "${workdir}/release-metadata.json" \
  --core-manifest "${workdir}/invalid-compat-core-manifest.json" \
  --output "${workdir}/should-fail-invalid-compat.json" >/dev/null 2>&1; then
  echo "expected invalid store_compatibility policy generation to fail" >&2
  exit 1
fi

echo "test_build_install_metadata_bundle: PASS"
