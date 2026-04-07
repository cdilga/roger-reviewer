#!/usr/bin/env bash
set -euo pipefail

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

sha256_file() {
  python3 - "$1" <<'PY'
import hashlib
import pathlib
import sys
path = pathlib.Path(sys.argv[1])
print(hashlib.sha256(path.read_bytes()).hexdigest())
PY
}

create_core_archive() {
  local archive_path="$1"
  python3 - "$archive_path" <<'PY'
import pathlib
import tarfile
import tempfile
import sys

archive = pathlib.Path(sys.argv[1])
archive.parent.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory() as tmp:
    root = pathlib.Path(tmp) / "roger-reviewer-core-x86_64-unknown-linux-gnu"
    root.mkdir(parents=True, exist_ok=True)
    (root / "rr").write_text("#!/bin/sh\necho rr\n", encoding="utf-8")
    with tarfile.open(archive, "w:gz") as tf:
        tf.add(root, arcname=root.name)
PY
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

write_version_metadata() {
  local path="$1"
  local channel="$2"
  local version="$3"
  local tag="$4"
  local prerelease="$5"
  local source_ref="$6"
  cat >"$path" <<EOF
{
  "channel": "${channel}",
  "version": "${version}",
  "tag": "${tag}",
  "prerelease": ${prerelease},
  "release_name": "Roger Reviewer ${version}",
  "artifact_stem": "roger-reviewer-${version}",
  "provenance": {
    "source_ref": "${source_ref}",
    "source_sha": "0123456789abcdef0123456789abcdef01234567",
    "source_short_sha": "0123456789ab",
    "date_basis": "2026.04.01",
    "version_source": "tag"
  }
}
EOF
}

write_verified_manifest() {
  local path="$1"
  local channel="$2"
  local version="$3"
  local tag="$4"
  local prerelease="$5"
  local publish_allowed="$6"
  local core_archive_name="$7"
  local core_archive_sha="$8"
  local bridge_status="$9"
  local extension_status="${10}"
  local bridge_artifacts_json="${11}"
  local extension_artifacts_json="${12}"

  cat >"$path" <<EOF
{
  "schema": "roger.release-verify-assets.v1",
  "release": {
    "channel": "${channel}",
    "version": "${version}",
    "tag": "${tag}",
    "prerelease": ${prerelease},
    "artifact_stem": "roger-reviewer-${version}"
  },
  "core": {
    "built_target_count": 1,
    "manifest_target_count": 1,
    "assets": [
      {
        "lane": "release-build-core",
        "kind": "core_archive",
        "label": "x86_64-unknown-linux-gnu",
        "path": "${core_archive_name}",
        "sha256": "${core_archive_sha}",
        "bytes": 1
      },
      {
        "lane": "release-build-core",
        "kind": "install_bootstrap",
        "label": "rr-install.sh",
        "path": "rr-install.sh",
        "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "bytes": 1
      },
      {
        "lane": "release-build-core",
        "kind": "install_bootstrap",
        "label": "rr-install.ps1",
        "path": "rr-install.ps1",
        "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "bytes": 1
      }
    ]
  },
  "optional_lanes": {
    "lane_summary": {
      "release-package-bridge": {
        "status": "${bridge_status}",
        "artifacts": ${bridge_artifacts_json},
        "observed_statuses": ["${bridge_status}"],
        "sources": ["fixture"]
      },
      "release-package-extension": {
        "status": "${extension_status}",
        "artifacts": ${extension_artifacts_json},
        "observed_statuses": ["${extension_status}"],
        "sources": ["fixture"]
      }
    },
    "assets": []
  },
  "publish_gate": {
    "publish_allowed": ${publish_allowed},
    "failure_count": 0,
    "warning_count": 0
  }
}
EOF
}

# PASS CASE: RC draft release rehearsal
pass_dir="${workdir}/pass"
mkdir -p "${pass_dir}/assets" "${pass_dir}/out"
create_core_archive "${pass_dir}/assets/roger-reviewer-2026.04.01-rc.2-core-x86_64-unknown-linux-gnu.tar.gz"
echo "bridge payload" >"${pass_dir}/assets/roger-reviewer-2026.04.01-rc.2-bridge-linux.tar.gz"
write_installer_bootstrap_assets "${pass_dir}/assets"
core_sha="$(sha256_file "${pass_dir}/assets/roger-reviewer-2026.04.01-rc.2-core-x86_64-unknown-linux-gnu.tar.gz")"
write_version_metadata \
  "${pass_dir}/release-metadata.json" \
  "rc" \
  "2026.04.01-rc.2" \
  "v2026.04.01-rc.2" \
  "true" \
  "refs/tags/v2026.04.01-rc.2"
write_verified_manifest \
  "${pass_dir}/release-asset-manifest.json" \
  "rc" \
  "2026.04.01-rc.2" \
  "v2026.04.01-rc.2" \
  "true" \
  "true" \
  "roger-reviewer-2026.04.01-rc.2-core-x86_64-unknown-linux-gnu.tar.gz" \
  "${core_sha}" \
  "built" \
  "skipped" \
  '["roger-reviewer-2026.04.01-rc.2-bridge-linux.tar.gz"]' \
  '[]'
cp "${pass_dir}/release-asset-manifest.json" "${pass_dir}/upstream-release-asset-manifest.json"
cat >"${pass_dir}/SHA256SUMS" <<'EOF'
0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  sample
EOF
cat >"${pass_dir}/release-notes-signing.md" <<'EOF'
# Signing Status

- `x86_64-unknown-linux-gnu` (unsigned_placeholder)
EOF

python3 scripts/release/publish_release.py \
  --version-metadata "${pass_dir}/release-metadata.json" \
  --verified-manifest "${pass_dir}/release-asset-manifest.json" \
  --upstream-verified-manifest "${pass_dir}/upstream-release-asset-manifest.json" \
  --core-run-url "https://github.com/example/repo/actions/runs/2001" \
  --verify-run-url "https://github.com/example/repo/actions/runs/2002" \
  --bridge-run-url "https://github.com/example/repo/actions/runs/2003" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${pass_dir}/SHA256SUMS" \
  --signing-notes "${pass_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${pass_dir}/out"

jq -e '.release.publish_mode == "draft" and .release.draft == true and .release.channel == "rc"' "${pass_dir}/out/release-plan.json" >/dev/null
jq -e '.verification.upstream_runs.core == "https://github.com/example/repo/actions/runs/2001"' "${pass_dir}/out/release-plan.json" >/dev/null
jq -e '.verification.upstream_runs.verify == "https://github.com/example/repo/actions/runs/2002"' "${pass_dir}/out/release-plan.json" >/dev/null
jq -e '.verification.upstream_runs.bridge == "https://github.com/example/repo/actions/runs/2003"' "${pass_dir}/out/release-plan.json" >/dev/null
jq -e '
  .assets
  | map(split("/")[-1])
  | index("rr-install.sh")
  and index("rr-install.ps1")
' "${pass_dir}/out/release-plan.json" >/dev/null
grep -q 'Shipped optional lanes: `release-package-bridge`' "${pass_dir}/out/release-notes.md"
grep -q 'Checksums: `SHA256SUMS`' "${pass_dir}/out/release-notes.md"
grep -q 'Core build run: `https://github.com/example/repo/actions/runs/2001`' "${pass_dir}/out/release-notes.md"
grep -q 'Verify-assets run: `https://github.com/example/repo/actions/runs/2002`' "${pass_dir}/out/release-notes.md"
grep -q 'Bridge package run: `https://github.com/example/repo/actions/runs/2003`' "${pass_dir}/out/release-notes.md"
grep -q 'docs/release-publish-operator-smoke.md' "${pass_dir}/out/release-notes.md"

# FAIL CASE: verified manifest missing install bootstrap entries must fail
missing_install_entries_dir="${workdir}/missing-install-bootstrap-entries"
mkdir -p "${missing_install_entries_dir}"
cp "${pass_dir}/release-metadata.json" "${missing_install_entries_dir}/release-metadata.json"
cp "${pass_dir}/release-asset-manifest.json" "${missing_install_entries_dir}/release-asset-manifest.json"
cp "${pass_dir}/SHA256SUMS" "${missing_install_entries_dir}/SHA256SUMS"
cp "${pass_dir}/release-notes-signing.md" "${missing_install_entries_dir}/release-notes-signing.md"
jq '
  .core.assets |= map(select(.kind != "install_bootstrap"))
' "${missing_install_entries_dir}/release-asset-manifest.json" >"${missing_install_entries_dir}/tmp.json"
mv "${missing_install_entries_dir}/tmp.json" "${missing_install_entries_dir}/release-asset-manifest.json"

if python3 scripts/release/publish_release.py \
  --version-metadata "${missing_install_entries_dir}/release-metadata.json" \
  --verified-manifest "${missing_install_entries_dir}/release-asset-manifest.json" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${missing_install_entries_dir}/SHA256SUMS" \
  --signing-notes "${missing_install_entries_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${missing_install_entries_dir}/out"; then
  echo "expected missing install bootstrap entry failure" >&2
  exit 1
fi

# FAIL CASE: publish mode cannot ship rc
if python3 scripts/release/publish_release.py \
  --version-metadata "${pass_dir}/release-metadata.json" \
  --verified-manifest "${pass_dir}/release-asset-manifest.json" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${pass_dir}/SHA256SUMS" \
  --signing-notes "${pass_dir}/release-notes-signing.md" \
  --publish-mode publish \
  --operator-smoke-ack \
  --output-dir "${pass_dir}/out-publish-rc"; then
  echo "expected rc publish-mode failure" >&2
  exit 1
fi

# FAIL CASE: upstream publish gate false must fail
bad_upstream_dir="${workdir}/bad-upstream"
mkdir -p "${bad_upstream_dir}"
cp "${pass_dir}/release-metadata.json" "${bad_upstream_dir}/release-metadata.json"
cp "${pass_dir}/release-asset-manifest.json" "${bad_upstream_dir}/release-asset-manifest.json"
cp "${pass_dir}/SHA256SUMS" "${bad_upstream_dir}/SHA256SUMS"
cp "${pass_dir}/release-notes-signing.md" "${bad_upstream_dir}/release-notes-signing.md"
cp "${pass_dir}/release-asset-manifest.json" "${bad_upstream_dir}/upstream-release-asset-manifest.json"
jq '.publish_gate.publish_allowed=false' "${bad_upstream_dir}/upstream-release-asset-manifest.json" >"${bad_upstream_dir}/tmp.json"
mv "${bad_upstream_dir}/tmp.json" "${bad_upstream_dir}/upstream-release-asset-manifest.json"

if python3 scripts/release/publish_release.py \
  --version-metadata "${bad_upstream_dir}/release-metadata.json" \
  --verified-manifest "${bad_upstream_dir}/release-asset-manifest.json" \
  --upstream-verified-manifest "${bad_upstream_dir}/upstream-release-asset-manifest.json" \
  --core-run-url "https://github.com/example/repo/actions/runs/2001" \
  --verify-run-url "https://github.com/example/repo/actions/runs/2002" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${bad_upstream_dir}/SHA256SUMS" \
  --signing-notes "${bad_upstream_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${bad_upstream_dir}/out"; then
  echo "expected upstream publish_gate failure" >&2
  exit 1
fi

# FAIL CASE: verified manifest schema mismatch must fail closed
bad_verified_schema_dir="${workdir}/bad-verified-schema"
mkdir -p "${bad_verified_schema_dir}"
cp "${pass_dir}/release-metadata.json" "${bad_verified_schema_dir}/release-metadata.json"
cp "${pass_dir}/release-asset-manifest.json" "${bad_verified_schema_dir}/release-asset-manifest.json"
cp "${pass_dir}/SHA256SUMS" "${bad_verified_schema_dir}/SHA256SUMS"
cp "${pass_dir}/release-notes-signing.md" "${bad_verified_schema_dir}/release-notes-signing.md"
jq '.schema="roger.release-verify-assets.v0"' \
  "${bad_verified_schema_dir}/release-asset-manifest.json" >"${bad_verified_schema_dir}/tmp.json"
mv "${bad_verified_schema_dir}/tmp.json" "${bad_verified_schema_dir}/release-asset-manifest.json"

if python3 scripts/release/publish_release.py \
  --version-metadata "${bad_verified_schema_dir}/release-metadata.json" \
  --verified-manifest "${bad_verified_schema_dir}/release-asset-manifest.json" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${bad_verified_schema_dir}/SHA256SUMS" \
  --signing-notes "${bad_verified_schema_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${bad_verified_schema_dir}/out"; then
  echo "expected verified-manifest schema failure" >&2
  exit 1
fi

# FAIL CASE: upstream manifest schema mismatch must fail closed
bad_upstream_schema_dir="${workdir}/bad-upstream-schema"
mkdir -p "${bad_upstream_schema_dir}"
cp "${pass_dir}/release-metadata.json" "${bad_upstream_schema_dir}/release-metadata.json"
cp "${pass_dir}/release-asset-manifest.json" "${bad_upstream_schema_dir}/release-asset-manifest.json"
cp "${pass_dir}/upstream-release-asset-manifest.json" "${bad_upstream_schema_dir}/upstream-release-asset-manifest.json"
cp "${pass_dir}/SHA256SUMS" "${bad_upstream_schema_dir}/SHA256SUMS"
cp "${pass_dir}/release-notes-signing.md" "${bad_upstream_schema_dir}/release-notes-signing.md"
jq '.schema="roger.release-verify-assets.v0"' \
  "${bad_upstream_schema_dir}/upstream-release-asset-manifest.json" >"${bad_upstream_schema_dir}/tmp.json"
mv "${bad_upstream_schema_dir}/tmp.json" "${bad_upstream_schema_dir}/upstream-release-asset-manifest.json"

if python3 scripts/release/publish_release.py \
  --version-metadata "${bad_upstream_schema_dir}/release-metadata.json" \
  --verified-manifest "${bad_upstream_schema_dir}/release-asset-manifest.json" \
  --upstream-verified-manifest "${bad_upstream_schema_dir}/upstream-release-asset-manifest.json" \
  --core-run-url "https://github.com/example/repo/actions/runs/2001" \
  --verify-run-url "https://github.com/example/repo/actions/runs/2002" \
  --bridge-run-url "https://github.com/example/repo/actions/runs/2003" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${bad_upstream_schema_dir}/SHA256SUMS" \
  --signing-notes "${bad_upstream_schema_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${bad_upstream_schema_dir}/out"; then
  echo "expected upstream-manifest schema failure" >&2
  exit 1
fi

# FAIL CASE: upstream manifest requires core/verify run URL provenance
if python3 scripts/release/publish_release.py \
  --version-metadata "${pass_dir}/release-metadata.json" \
  --verified-manifest "${pass_dir}/release-asset-manifest.json" \
  --upstream-verified-manifest "${pass_dir}/upstream-release-asset-manifest.json" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${pass_dir}/SHA256SUMS" \
  --signing-notes "${pass_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${pass_dir}/out-missing-run-provenance"; then
  echo "expected missing run-provenance failure" >&2
  exit 1
fi

# FAIL CASE: upstream run URLs must be GitHub Actions run URLs
if python3 scripts/release/publish_release.py \
  --version-metadata "${pass_dir}/release-metadata.json" \
  --verified-manifest "${pass_dir}/release-asset-manifest.json" \
  --upstream-verified-manifest "${pass_dir}/upstream-release-asset-manifest.json" \
  --core-run-url "https://example.com/not-a-run/2001" \
  --verify-run-url "https://github.com/example/repo/actions/runs/2002" \
  --bridge-run-url "https://github.com/example/repo/actions/runs/2003" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${pass_dir}/SHA256SUMS" \
  --signing-notes "${pass_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${pass_dir}/out-invalid-run-url"; then
  echo "expected invalid run-url format failure" >&2
  exit 1
fi

# FAIL CASE: upstream built bridge lane requires bridge run URL provenance
if python3 scripts/release/publish_release.py \
  --version-metadata "${pass_dir}/release-metadata.json" \
  --verified-manifest "${pass_dir}/release-asset-manifest.json" \
  --upstream-verified-manifest "${pass_dir}/upstream-release-asset-manifest.json" \
  --core-run-url "https://github.com/example/repo/actions/runs/2001" \
  --verify-run-url "https://github.com/example/repo/actions/runs/2002" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${pass_dir}/SHA256SUMS" \
  --signing-notes "${pass_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${pass_dir}/out-missing-bridge-run-url"; then
  echo "expected missing bridge run-url provenance failure" >&2
  exit 1
fi

# FAIL CASE: bridge run URL must be omitted when upstream lane is skipped
extraneous_bridge_run_url_dir="${workdir}/extraneous-bridge-run-url"
mkdir -p "${extraneous_bridge_run_url_dir}"
cp "${pass_dir}/release-metadata.json" "${extraneous_bridge_run_url_dir}/release-metadata.json"
cp "${pass_dir}/release-asset-manifest.json" "${extraneous_bridge_run_url_dir}/release-asset-manifest.json"
cp "${pass_dir}/upstream-release-asset-manifest.json" "${extraneous_bridge_run_url_dir}/upstream-release-asset-manifest.json"
cp "${pass_dir}/SHA256SUMS" "${extraneous_bridge_run_url_dir}/SHA256SUMS"
cp "${pass_dir}/release-notes-signing.md" "${extraneous_bridge_run_url_dir}/release-notes-signing.md"
jq '
  .optional_lanes.lane_summary["release-package-bridge"].status="skipped"
  | .optional_lanes.lane_summary["release-package-bridge"].artifacts=[]
' "${extraneous_bridge_run_url_dir}/release-asset-manifest.json" >"${extraneous_bridge_run_url_dir}/tmp.json"
mv "${extraneous_bridge_run_url_dir}/tmp.json" "${extraneous_bridge_run_url_dir}/release-asset-manifest.json"
jq '
  .optional_lanes.lane_summary["release-package-bridge"].status="skipped"
  | .optional_lanes.lane_summary["release-package-bridge"].artifacts=[]
' "${extraneous_bridge_run_url_dir}/upstream-release-asset-manifest.json" >"${extraneous_bridge_run_url_dir}/tmp.json"
mv "${extraneous_bridge_run_url_dir}/tmp.json" "${extraneous_bridge_run_url_dir}/upstream-release-asset-manifest.json"

if python3 scripts/release/publish_release.py \
  --version-metadata "${extraneous_bridge_run_url_dir}/release-metadata.json" \
  --verified-manifest "${extraneous_bridge_run_url_dir}/release-asset-manifest.json" \
  --upstream-verified-manifest "${extraneous_bridge_run_url_dir}/upstream-release-asset-manifest.json" \
  --core-run-url "https://github.com/example/repo/actions/runs/2001" \
  --verify-run-url "https://github.com/example/repo/actions/runs/2002" \
  --bridge-run-url "https://github.com/example/repo/actions/runs/2003" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${extraneous_bridge_run_url_dir}/SHA256SUMS" \
  --signing-notes "${extraneous_bridge_run_url_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${extraneous_bridge_run_url_dir}/out"; then
  echo "expected extraneous bridge run-url failure for skipped lane" >&2
  exit 1
fi

# FAIL CASE: extension run URL must be omitted when upstream lane is skipped
extraneous_extension_run_url_dir="${workdir}/extraneous-extension-run-url"
mkdir -p "${extraneous_extension_run_url_dir}"
cp "${pass_dir}/release-metadata.json" "${extraneous_extension_run_url_dir}/release-metadata.json"
cp "${pass_dir}/release-asset-manifest.json" "${extraneous_extension_run_url_dir}/release-asset-manifest.json"
cp "${pass_dir}/upstream-release-asset-manifest.json" "${extraneous_extension_run_url_dir}/upstream-release-asset-manifest.json"
cp "${pass_dir}/SHA256SUMS" "${extraneous_extension_run_url_dir}/SHA256SUMS"
cp "${pass_dir}/release-notes-signing.md" "${extraneous_extension_run_url_dir}/release-notes-signing.md"
jq '
  .optional_lanes.lane_summary["release-package-extension"].status="skipped"
  | .optional_lanes.lane_summary["release-package-extension"].artifacts=[]
' "${extraneous_extension_run_url_dir}/release-asset-manifest.json" >"${extraneous_extension_run_url_dir}/tmp.json"
mv "${extraneous_extension_run_url_dir}/tmp.json" "${extraneous_extension_run_url_dir}/release-asset-manifest.json"
jq '
  .optional_lanes.lane_summary["release-package-extension"].status="skipped"
  | .optional_lanes.lane_summary["release-package-extension"].artifacts=[]
' "${extraneous_extension_run_url_dir}/upstream-release-asset-manifest.json" >"${extraneous_extension_run_url_dir}/tmp.json"
mv "${extraneous_extension_run_url_dir}/tmp.json" "${extraneous_extension_run_url_dir}/upstream-release-asset-manifest.json"

if python3 scripts/release/publish_release.py \
  --version-metadata "${extraneous_extension_run_url_dir}/release-metadata.json" \
  --verified-manifest "${extraneous_extension_run_url_dir}/release-asset-manifest.json" \
  --upstream-verified-manifest "${extraneous_extension_run_url_dir}/upstream-release-asset-manifest.json" \
  --core-run-url "https://github.com/example/repo/actions/runs/2001" \
  --verify-run-url "https://github.com/example/repo/actions/runs/2002" \
  --extension-run-url "https://github.com/example/repo/actions/runs/2004" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${extraneous_extension_run_url_dir}/SHA256SUMS" \
  --signing-notes "${extraneous_extension_run_url_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${extraneous_extension_run_url_dir}/out"; then
  echo "expected extraneous extension run-url failure for skipped lane" >&2
  exit 1
fi

# FAIL CASE: upstream built extension lane requires extension run URL provenance
extension_dir="${workdir}/missing-extension-run-url"
mkdir -p "${extension_dir}"
cp "${pass_dir}/release-metadata.json" "${extension_dir}/release-metadata.json"
cp "${pass_dir}/release-asset-manifest.json" "${extension_dir}/release-asset-manifest.json"
cp "${pass_dir}/upstream-release-asset-manifest.json" "${extension_dir}/upstream-release-asset-manifest.json"
cp "${pass_dir}/SHA256SUMS" "${extension_dir}/SHA256SUMS"
cp "${pass_dir}/release-notes-signing.md" "${extension_dir}/release-notes-signing.md"
jq '.optional_lanes.lane_summary["release-package-extension"].status="built"' \
  "${extension_dir}/release-asset-manifest.json" >"${extension_dir}/tmp.json"
mv "${extension_dir}/tmp.json" "${extension_dir}/release-asset-manifest.json"
jq '.optional_lanes.lane_summary["release-package-extension"].status="built"' \
  "${extension_dir}/upstream-release-asset-manifest.json" >"${extension_dir}/tmp.json"
mv "${extension_dir}/tmp.json" "${extension_dir}/upstream-release-asset-manifest.json"

if python3 scripts/release/publish_release.py \
  --version-metadata "${extension_dir}/release-metadata.json" \
  --verified-manifest "${extension_dir}/release-asset-manifest.json" \
  --upstream-verified-manifest "${extension_dir}/upstream-release-asset-manifest.json" \
  --core-run-url "https://github.com/example/repo/actions/runs/2001" \
  --verify-run-url "https://github.com/example/repo/actions/runs/2002" \
  --bridge-run-url "https://github.com/example/repo/actions/runs/2003" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${extension_dir}/SHA256SUMS" \
  --signing-notes "${extension_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${extension_dir}/out"; then
  echo "expected missing extension run-url provenance failure" >&2
  exit 1
fi

# FAIL CASE: upstream skipped lane cannot be widened to built in reverified manifest
lane_widening_dir="${workdir}/lane-widening"
mkdir -p "${lane_widening_dir}"
cp "${pass_dir}/release-metadata.json" "${lane_widening_dir}/release-metadata.json"
cp "${pass_dir}/release-asset-manifest.json" "${lane_widening_dir}/release-asset-manifest.json"
cp "${pass_dir}/upstream-release-asset-manifest.json" "${lane_widening_dir}/upstream-release-asset-manifest.json"
cp "${pass_dir}/SHA256SUMS" "${lane_widening_dir}/SHA256SUMS"
cp "${pass_dir}/release-notes-signing.md" "${lane_widening_dir}/release-notes-signing.md"

# Make upstream skip bridge lane while reverified manifest still claims it as built.
jq '.optional_lanes.lane_summary["release-package-bridge"].status="skipped"' \
  "${lane_widening_dir}/upstream-release-asset-manifest.json" >"${lane_widening_dir}/tmp.json"
mv "${lane_widening_dir}/tmp.json" "${lane_widening_dir}/upstream-release-asset-manifest.json"

if python3 scripts/release/publish_release.py \
  --version-metadata "${lane_widening_dir}/release-metadata.json" \
  --verified-manifest "${lane_widening_dir}/release-asset-manifest.json" \
  --upstream-verified-manifest "${lane_widening_dir}/upstream-release-asset-manifest.json" \
  --core-run-url "https://github.com/example/repo/actions/runs/2001" \
  --verify-run-url "https://github.com/example/repo/actions/runs/2002" \
  --bridge-run-url "https://github.com/example/repo/actions/runs/2003" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${lane_widening_dir}/SHA256SUMS" \
  --signing-notes "${lane_widening_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${lane_widening_dir}/out"; then
  echo "expected optional-lane widening parity failure" >&2
  exit 1
fi

# FAIL CASE: upstream built lane cannot be downgraded to skipped in reverified manifest
lane_downgrade_dir="${workdir}/lane-downgrade"
mkdir -p "${lane_downgrade_dir}"
cp "${pass_dir}/release-metadata.json" "${lane_downgrade_dir}/release-metadata.json"
cp "${pass_dir}/release-asset-manifest.json" "${lane_downgrade_dir}/release-asset-manifest.json"
cp "${pass_dir}/upstream-release-asset-manifest.json" "${lane_downgrade_dir}/upstream-release-asset-manifest.json"
cp "${pass_dir}/SHA256SUMS" "${lane_downgrade_dir}/SHA256SUMS"
cp "${pass_dir}/release-notes-signing.md" "${lane_downgrade_dir}/release-notes-signing.md"

# Upstream keeps bridge lane built; reverified manifest incorrectly downgrades it to skipped.
jq '.optional_lanes.lane_summary["release-package-bridge"].status="skipped"' \
  "${lane_downgrade_dir}/release-asset-manifest.json" >"${lane_downgrade_dir}/tmp.json"
mv "${lane_downgrade_dir}/tmp.json" "${lane_downgrade_dir}/release-asset-manifest.json"

if python3 scripts/release/publish_release.py \
  --version-metadata "${lane_downgrade_dir}/release-metadata.json" \
  --verified-manifest "${lane_downgrade_dir}/release-asset-manifest.json" \
  --upstream-verified-manifest "${lane_downgrade_dir}/upstream-release-asset-manifest.json" \
  --core-run-url "https://github.com/example/repo/actions/runs/2001" \
  --verify-run-url "https://github.com/example/repo/actions/runs/2002" \
  --bridge-run-url "https://github.com/example/repo/actions/runs/2003" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${lane_downgrade_dir}/SHA256SUMS" \
  --signing-notes "${lane_downgrade_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${lane_downgrade_dir}/out"; then
  echo "expected optional-lane downgrade parity failure" >&2
  exit 1
fi

# FAIL CASE: upstream optional lane status must be built/skipped
invalid_upstream_status_dir="${workdir}/invalid-upstream-lane-status"
mkdir -p "${invalid_upstream_status_dir}"
cp "${pass_dir}/release-metadata.json" "${invalid_upstream_status_dir}/release-metadata.json"
cp "${pass_dir}/release-asset-manifest.json" "${invalid_upstream_status_dir}/release-asset-manifest.json"
cp "${pass_dir}/upstream-release-asset-manifest.json" "${invalid_upstream_status_dir}/upstream-release-asset-manifest.json"
cp "${pass_dir}/SHA256SUMS" "${invalid_upstream_status_dir}/SHA256SUMS"
cp "${pass_dir}/release-notes-signing.md" "${invalid_upstream_status_dir}/release-notes-signing.md"
jq '.optional_lanes.lane_summary["release-package-bridge"].status="failed"' \
  "${invalid_upstream_status_dir}/upstream-release-asset-manifest.json" >"${invalid_upstream_status_dir}/tmp.json"
mv "${invalid_upstream_status_dir}/tmp.json" "${invalid_upstream_status_dir}/upstream-release-asset-manifest.json"

if python3 scripts/release/publish_release.py \
  --version-metadata "${invalid_upstream_status_dir}/release-metadata.json" \
  --verified-manifest "${invalid_upstream_status_dir}/release-asset-manifest.json" \
  --upstream-verified-manifest "${invalid_upstream_status_dir}/upstream-release-asset-manifest.json" \
  --core-run-url "https://github.com/example/repo/actions/runs/2001" \
  --verify-run-url "https://github.com/example/repo/actions/runs/2002" \
  --bridge-run-url "https://github.com/example/repo/actions/runs/2003" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${invalid_upstream_status_dir}/SHA256SUMS" \
  --signing-notes "${invalid_upstream_status_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${invalid_upstream_status_dir}/out"; then
  echo "expected invalid upstream optional-lane status failure" >&2
  exit 1
fi

# FAIL CASE: reverified optional lane status must be built/skipped
invalid_reverified_status_dir="${workdir}/invalid-reverified-lane-status"
mkdir -p "${invalid_reverified_status_dir}"
cp "${pass_dir}/release-metadata.json" "${invalid_reverified_status_dir}/release-metadata.json"
cp "${pass_dir}/release-asset-manifest.json" "${invalid_reverified_status_dir}/release-asset-manifest.json"
cp "${pass_dir}/upstream-release-asset-manifest.json" "${invalid_reverified_status_dir}/upstream-release-asset-manifest.json"
cp "${pass_dir}/SHA256SUMS" "${invalid_reverified_status_dir}/SHA256SUMS"
cp "${pass_dir}/release-notes-signing.md" "${invalid_reverified_status_dir}/release-notes-signing.md"
jq '.optional_lanes.lane_summary["release-package-bridge"].status="failed"' \
  "${invalid_reverified_status_dir}/release-asset-manifest.json" >"${invalid_reverified_status_dir}/tmp.json"
mv "${invalid_reverified_status_dir}/tmp.json" "${invalid_reverified_status_dir}/release-asset-manifest.json"

if python3 scripts/release/publish_release.py \
  --version-metadata "${invalid_reverified_status_dir}/release-metadata.json" \
  --verified-manifest "${invalid_reverified_status_dir}/release-asset-manifest.json" \
  --upstream-verified-manifest "${invalid_reverified_status_dir}/upstream-release-asset-manifest.json" \
  --core-run-url "https://github.com/example/repo/actions/runs/2001" \
  --verify-run-url "https://github.com/example/repo/actions/runs/2002" \
  --bridge-run-url "https://github.com/example/repo/actions/runs/2003" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${invalid_reverified_status_dir}/SHA256SUMS" \
  --signing-notes "${invalid_reverified_status_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${invalid_reverified_status_dir}/out"; then
  echo "expected invalid reverified optional-lane status failure" >&2
  exit 1
fi

# FAIL CASE: invalid optional-lane status fails even without upstream manifest
invalid_status_no_upstream_dir="${workdir}/invalid-lane-status-no-upstream"
mkdir -p "${invalid_status_no_upstream_dir}"
cp "${pass_dir}/release-metadata.json" "${invalid_status_no_upstream_dir}/release-metadata.json"
cp "${pass_dir}/release-asset-manifest.json" "${invalid_status_no_upstream_dir}/release-asset-manifest.json"
cp "${pass_dir}/SHA256SUMS" "${invalid_status_no_upstream_dir}/SHA256SUMS"
cp "${pass_dir}/release-notes-signing.md" "${invalid_status_no_upstream_dir}/release-notes-signing.md"
jq '.optional_lanes.lane_summary["release-package-extension"].status="unknown"' \
  "${invalid_status_no_upstream_dir}/release-asset-manifest.json" >"${invalid_status_no_upstream_dir}/tmp.json"
mv "${invalid_status_no_upstream_dir}/tmp.json" "${invalid_status_no_upstream_dir}/release-asset-manifest.json"

if python3 scripts/release/publish_release.py \
  --version-metadata "${invalid_status_no_upstream_dir}/release-metadata.json" \
  --verified-manifest "${invalid_status_no_upstream_dir}/release-asset-manifest.json" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${invalid_status_no_upstream_dir}/SHA256SUMS" \
  --signing-notes "${invalid_status_no_upstream_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${invalid_status_no_upstream_dir}/out"; then
  echo "expected invalid optional-lane status failure without upstream manifest" >&2
  exit 1
fi

# FAIL CASE: verified manifest must include explicit lane-summary entries
missing_lane_entry_dir="${workdir}/missing-lane-entry"
mkdir -p "${missing_lane_entry_dir}"
cp "${pass_dir}/release-metadata.json" "${missing_lane_entry_dir}/release-metadata.json"
cp "${pass_dir}/release-asset-manifest.json" "${missing_lane_entry_dir}/release-asset-manifest.json"
cp "${pass_dir}/SHA256SUMS" "${missing_lane_entry_dir}/SHA256SUMS"
cp "${pass_dir}/release-notes-signing.md" "${missing_lane_entry_dir}/release-notes-signing.md"
jq 'del(.optional_lanes.lane_summary["release-package-extension"])' \
  "${missing_lane_entry_dir}/release-asset-manifest.json" >"${missing_lane_entry_dir}/tmp.json"
mv "${missing_lane_entry_dir}/tmp.json" "${missing_lane_entry_dir}/release-asset-manifest.json"

if python3 scripts/release/publish_release.py \
  --version-metadata "${missing_lane_entry_dir}/release-metadata.json" \
  --verified-manifest "${missing_lane_entry_dir}/release-asset-manifest.json" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${missing_lane_entry_dir}/SHA256SUMS" \
  --signing-notes "${missing_lane_entry_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${missing_lane_entry_dir}/out"; then
  echo "expected missing optional-lane entry failure" >&2
  exit 1
fi

# FAIL CASE: non-tag source_ref rejected
bad_ref_dir="${workdir}/bad-ref"
mkdir -p "${bad_ref_dir}"
cp "${pass_dir}/release-asset-manifest.json" "${bad_ref_dir}/release-asset-manifest.json"
cp "${pass_dir}/SHA256SUMS" "${bad_ref_dir}/SHA256SUMS"
cp "${pass_dir}/release-notes-signing.md" "${bad_ref_dir}/release-notes-signing.md"
write_version_metadata \
  "${bad_ref_dir}/release-metadata.json" \
  "rc" \
  "2026.04.01-rc.2" \
  "v2026.04.01-rc.2" \
  "true" \
  "refs/heads/main"

if python3 scripts/release/publish_release.py \
  --version-metadata "${bad_ref_dir}/release-metadata.json" \
  --verified-manifest "${bad_ref_dir}/release-asset-manifest.json" \
  --asset-root "${pass_dir}/assets" \
  --checksums "${bad_ref_dir}/SHA256SUMS" \
  --signing-notes "${bad_ref_dir}/release-notes-signing.md" \
  --publish-mode draft \
  --output-dir "${bad_ref_dir}/out"; then
  echo "expected source_ref approval-policy failure" >&2
  exit 1
fi

# PASS CASE: stable publish requires smoke ack and succeeds with it
stable_dir="${workdir}/stable"
mkdir -p "${stable_dir}/assets" "${stable_dir}/out"
create_core_archive "${stable_dir}/assets/roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz"
write_installer_bootstrap_assets "${stable_dir}/assets"
stable_sha="$(sha256_file "${stable_dir}/assets/roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz")"
write_version_metadata \
  "${stable_dir}/release-metadata.json" \
  "stable" \
  "2026.04.01" \
  "v2026.04.01" \
  "false" \
  "refs/tags/v2026.04.01"
write_verified_manifest \
  "${stable_dir}/release-asset-manifest.json" \
  "stable" \
  "2026.04.01" \
  "v2026.04.01" \
  "false" \
  "true" \
  "roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz" \
  "${stable_sha}" \
  "skipped" \
  "skipped" \
  '[]' \
  '[]'
cat >"${stable_dir}/SHA256SUMS" <<'EOF'
0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef  sample
EOF
cat >"${stable_dir}/release-notes-signing.md" <<'EOF'
# Signing Status

- `x86_64-unknown-linux-gnu` (unsigned_placeholder)
EOF

if python3 scripts/release/publish_release.py \
  --version-metadata "${stable_dir}/release-metadata.json" \
  --verified-manifest "${stable_dir}/release-asset-manifest.json" \
  --asset-root "${stable_dir}/assets" \
  --checksums "${stable_dir}/SHA256SUMS" \
  --signing-notes "${stable_dir}/release-notes-signing.md" \
  --publish-mode publish \
  --output-dir "${stable_dir}/out-no-ack"; then
  echo "expected publish-mode without smoke ack failure" >&2
  exit 1
fi

python3 scripts/release/publish_release.py \
  --version-metadata "${stable_dir}/release-metadata.json" \
  --verified-manifest "${stable_dir}/release-asset-manifest.json" \
  --asset-root "${stable_dir}/assets" \
  --checksums "${stable_dir}/SHA256SUMS" \
  --signing-notes "${stable_dir}/release-notes-signing.md" \
  --publish-mode publish \
  --operator-smoke-ack \
  --output-dir "${stable_dir}/out"

jq -e '.release.publish_mode == "publish" and .release.draft == false and .release.channel == "stable" and .release.operator_smoke_ack == true' "${stable_dir}/out/release-plan.json" >/dev/null

echo "test_publish_release: PASS"
