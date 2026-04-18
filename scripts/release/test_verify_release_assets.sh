#!/usr/bin/env bash
set -euo pipefail

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

write_release_metadata() {
  cat >"$1" <<'EOF'
{
  "channel": "stable",
  "version": "2026.04.01",
  "tag": "v2026.04.01",
  "prerelease": false,
  "artifact_stem": "roger-reviewer-2026.04.01"
}
EOF
}

create_core_archive_fixture() {
  local archive_path="$1"
  python3 - "$archive_path" <<'PY'
import pathlib
import tarfile
import tempfile
import sys

archive = pathlib.Path(sys.argv[1])
archive.parent.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory() as tmp:
    root = pathlib.Path(tmp) / "roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu"
    root.mkdir(parents=True, exist_ok=True)
    (root / "rr").write_text("#!/bin/sh\necho rr\n", encoding="utf-8")
    with tarfile.open(archive, "w:gz") as tf:
        tf.add(root, arcname=root.name)
PY
}

archive_sha256() {
  python3 - "$1" <<'PY'
import hashlib
import pathlib
import sys
path = pathlib.Path(sys.argv[1])
print(hashlib.sha256(path.read_bytes()).hexdigest())
PY
}

write_core_manifest() {
  local path="$1"
  local sha="$2"
  cat >"$path" <<EOF
{
  "schema": "roger.release-build-core.v1",
  "channel": "stable",
  "version": "2026.04.01",
  "tag": "v2026.04.01",
  "artifact_stem": "roger-reviewer-2026.04.01",
  "targets": [
    {
      "target": "x86_64-unknown-linux-gnu",
      "binary_name": "rr",
      "archive_name": "roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz",
      "archive_sha256": "${sha}",
      "payload_dir": "roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu"
    }
  ]
}
EOF
}

write_install_metadata() {
  local path="$1"
  local sha="$2"
  local override_version="${3:-2026.04.01}"
  cat >"$path" <<EOF
{
  "schema": "roger.release.install-metadata.v1",
  "release": {
    "channel": "stable",
    "version": "${override_version}",
    "tag": "v${override_version}",
    "prerelease": false,
    "artifact_stem": "roger-reviewer-2026.04.01"
  },
  "checksums_name": "SHA256SUMS",
  "core_manifest_name": "core-manifest.json",
  "targets": [
    {
      "target": "x86_64-unknown-linux-gnu",
      "archive_name": "roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz",
      "archive_sha256": "${sha}",
      "payload_dir": "roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu",
      "binary_name": "rr"
    }
  ]
}
EOF
}

write_optional_summary() {
  local path="$1"
  local bridge_status="$2"
  local extension_status="$3"
  local narrowed_claims_json="$4"
  local bridge_artifacts_json="$5"
  local extension_artifacts_json="$6"
  local posture="core_only"
  local shipped_optional_lanes='[]'

  if [[ "$bridge_status" == "built" && "$extension_status" == "built" ]]; then
    posture="core_plus_bridge_plus_extension"
    shipped_optional_lanes='["release-package-bridge","release-package-extension"]'
  elif [[ "$bridge_status" == "built" ]]; then
    posture="core_plus_bridge"
    shipped_optional_lanes='["release-package-bridge"]'
  fi

  cat >"$path" <<EOF
{
  "schema": "roger.release.optional_lanes.v1",
  "release": {
    "channel": "stable",
    "version": "2026.04.01",
    "tag": "v2026.04.01",
    "prerelease": false,
    "artifact_stem": "roger-reviewer-2026.04.01"
  },
  "lanes": {
    "release-package-bridge": {
      "status": "${bridge_status}",
      "artifacts": ${bridge_artifacts_json},
      "notes": []
    },
    "release-package-extension": {
      "status": "${extension_status}",
      "artifacts": ${extension_artifacts_json},
      "notes": []
    }
  },
  "support_claims": {
    "posture": "${posture}",
    "shipped_optional_lanes": ${shipped_optional_lanes},
    "narrowed_claims": ${narrowed_claims_json},
    "warnings": []
  }
}
EOF
}

write_installer_bootstrap_assets() {
  local asset_root="$1"
  cat >"${asset_root}/rr-install.sh" <<'EOF'
#!/usr/bin/env bash
echo "rr install"
EOF
  cat >"${asset_root}/rr-install.ps1" <<'EOF'
Write-Output "rr install"
EOF
}

run_verify() {
  local metadata="$1"
  local core_manifest="$2"
  local asset_root="$3"
  local optional_summary="$4"
  local output_dir="$5"

  python3 scripts/release/verify_release_assets.py \
    --version-metadata "$metadata" \
    --core-manifest "$core_manifest" \
    --asset-root "$asset_root" \
    --optional-summary "$optional_summary" \
    --output-dir "$output_dir"
}

# PASS CASE
pass_dir="${workdir}/pass"
mkdir -p "${pass_dir}/assets" "${pass_dir}/out"
write_release_metadata "${pass_dir}/release-metadata.json"
create_core_archive_fixture "${pass_dir}/assets/roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz"
sha="$(archive_sha256 "${pass_dir}/assets/roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz")"
write_core_manifest "${pass_dir}/core-manifest.json" "${sha}"
write_install_metadata "${pass_dir}/assets/release-install-metadata-2026.04.01.json" "${sha}"
write_installer_bootstrap_assets "${pass_dir}/assets"
echo "bridge lane artifact payload" >"${pass_dir}/assets/roger-reviewer-2026.04.01-bridge-linux.tar.gz"
write_optional_summary \
  "${pass_dir}/optional-summary.json" \
  "built" \
  "skipped" \
  '["extension_sideload_unshipped"]' \
  '["roger-reviewer-2026.04.01-bridge-linux.tar.gz"]' \
  '[]'

run_verify \
  "${pass_dir}/release-metadata.json" \
  "${pass_dir}/core-manifest.json" \
  "${pass_dir}/assets" \
  "${pass_dir}/optional-summary.json" \
  "${pass_dir}/out"

test -f "${pass_dir}/out/SHA256SUMS"
test -f "${pass_dir}/out/release-asset-manifest.json"
test -f "${pass_dir}/out/release-notes-signing.md"
jq -e '.publish_gate.publish_allowed == true' "${pass_dir}/out/release-asset-manifest.json" >/dev/null
jq -e '
  .core.assets
  | any(
      .kind == "core_manifest"
      and (
        .path == "core-manifest.json"
        or (.path | endswith("/core-manifest.json"))
      )
    )
' "${pass_dir}/out/release-asset-manifest.json" >/dev/null
jq -e '
  .core.assets
  | map(select(.kind == "install_bootstrap") | .label)
  | sort
  == ["rr-install.ps1","rr-install.sh"]
' "${pass_dir}/out/release-asset-manifest.json" >/dev/null
grep -q "core-manifest.json$" "${pass_dir}/out/SHA256SUMS"
grep -q "rr-install.sh$" "${pass_dir}/out/SHA256SUMS"
grep -q "rr-install.ps1$" "${pass_dir}/out/SHA256SUMS"

# FAIL CASE 1: missing archive
missing_dir="${workdir}/missing"
mkdir -p "${missing_dir}/assets" "${missing_dir}/out"
write_release_metadata "${missing_dir}/release-metadata.json"
write_core_manifest "${missing_dir}/core-manifest.json" "deadbeef"
write_install_metadata "${missing_dir}/assets/release-install-metadata-2026.04.01.json" "deadbeef"
write_installer_bootstrap_assets "${missing_dir}/assets"
write_optional_summary \
  "${missing_dir}/optional-summary.json" \
  "skipped" \
  "skipped" \
  '["bridge_registration_unshipped","extension_sideload_unshipped"]' \
  '[]' \
  '[]'

if run_verify \
  "${missing_dir}/release-metadata.json" \
  "${missing_dir}/core-manifest.json" \
  "${missing_dir}/assets" \
  "${missing_dir}/optional-summary.json" \
  "${missing_dir}/out"; then
  echo "expected missing-archive verification failure" >&2
  exit 1
fi

# FAIL CASE 2: checksum mismatch
checksum_dir="${workdir}/checksum"
mkdir -p "${checksum_dir}/assets" "${checksum_dir}/out"
write_release_metadata "${checksum_dir}/release-metadata.json"
create_core_archive_fixture "${checksum_dir}/assets/roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz"
write_core_manifest "${checksum_dir}/core-manifest.json" "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
write_install_metadata "${checksum_dir}/assets/release-install-metadata-2026.04.01.json" "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
write_installer_bootstrap_assets "${checksum_dir}/assets"
write_optional_summary \
  "${checksum_dir}/optional-summary.json" \
  "skipped" \
  "skipped" \
  '["bridge_registration_unshipped","extension_sideload_unshipped"]' \
  '[]' \
  '[]'

if run_verify \
  "${checksum_dir}/release-metadata.json" \
  "${checksum_dir}/core-manifest.json" \
  "${checksum_dir}/assets" \
  "${checksum_dir}/optional-summary.json" \
  "${checksum_dir}/out"; then
  echo "expected checksum verification failure" >&2
  exit 1
fi

# FAIL CASE 3: lane-claim drift
drift_dir="${workdir}/drift"
mkdir -p "${drift_dir}/assets" "${drift_dir}/out"
write_release_metadata "${drift_dir}/release-metadata.json"
create_core_archive_fixture "${drift_dir}/assets/roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz"
sha_drift="$(archive_sha256 "${drift_dir}/assets/roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz")"
write_core_manifest "${drift_dir}/core-manifest.json" "${sha_drift}"
write_install_metadata "${drift_dir}/assets/release-install-metadata-2026.04.01.json" "${sha_drift}"
write_installer_bootstrap_assets "${drift_dir}/assets"
echo "bridge lane artifact payload" >"${drift_dir}/assets/roger-reviewer-2026.04.01-bridge-linux.tar.gz"
write_optional_summary \
  "${drift_dir}/optional-summary.json" \
  "built" \
  "skipped" \
  '["bridge_registration_unshipped","extension_sideload_unshipped"]' \
  '["roger-reviewer-2026.04.01-bridge-linux.tar.gz"]' \
  '[]'

if run_verify \
  "${drift_dir}/release-metadata.json" \
  "${drift_dir}/core-manifest.json" \
  "${drift_dir}/assets" \
  "${drift_dir}/optional-summary.json" \
  "${drift_dir}/out"; then
  echo "expected lane-claim drift verification failure" >&2
  exit 1
fi

# FAIL CASE 4: install metadata release mismatch
metadata_drift_dir="${workdir}/metadata-drift"
mkdir -p "${metadata_drift_dir}/assets" "${metadata_drift_dir}/out"
write_release_metadata "${metadata_drift_dir}/release-metadata.json"
create_core_archive_fixture "${metadata_drift_dir}/assets/roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz"
sha_metadata_drift="$(archive_sha256 "${metadata_drift_dir}/assets/roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz")"
write_core_manifest "${metadata_drift_dir}/core-manifest.json" "${sha_metadata_drift}"
write_install_metadata \
  "${metadata_drift_dir}/assets/release-install-metadata-2026.04.01.json" \
  "${sha_metadata_drift}" \
  "2026.04.99"
write_installer_bootstrap_assets "${metadata_drift_dir}/assets"
write_optional_summary \
  "${metadata_drift_dir}/optional-summary.json" \
  "skipped" \
  "skipped" \
  '["bridge_registration_unshipped","extension_sideload_unshipped"]' \
  '[]' \
  '[]'

if run_verify \
  "${metadata_drift_dir}/release-metadata.json" \
  "${metadata_drift_dir}/core-manifest.json" \
  "${metadata_drift_dir}/assets" \
  "${metadata_drift_dir}/optional-summary.json" \
  "${metadata_drift_dir}/out"; then
  echo "expected install metadata release mismatch failure" >&2
  exit 1
fi

# FAIL CASE 5: missing installer bootstrap assets
missing_installer_dir="${workdir}/missing-installer"
mkdir -p "${missing_installer_dir}/assets" "${missing_installer_dir}/out"
write_release_metadata "${missing_installer_dir}/release-metadata.json"
create_core_archive_fixture "${missing_installer_dir}/assets/roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz"
sha_missing_installer="$(archive_sha256 "${missing_installer_dir}/assets/roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz")"
write_core_manifest "${missing_installer_dir}/core-manifest.json" "${sha_missing_installer}"
write_install_metadata "${missing_installer_dir}/assets/release-install-metadata-2026.04.01.json" "${sha_missing_installer}"
write_optional_summary \
  "${missing_installer_dir}/optional-summary.json" \
  "skipped" \
  "skipped" \
  '["bridge_registration_unshipped","extension_sideload_unshipped"]' \
  '[]' \
  '[]'

if run_verify \
  "${missing_installer_dir}/release-metadata.json" \
  "${missing_installer_dir}/core-manifest.json" \
  "${missing_installer_dir}/assets" \
  "${missing_installer_dir}/optional-summary.json" \
  "${missing_installer_dir}/out"; then
  echo "expected missing installer bootstrap verification failure" >&2
  exit 1
fi

echo "test_verify_release_assets: PASS"
