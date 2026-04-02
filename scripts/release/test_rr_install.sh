#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INSTALL_SCRIPT="${ROOT_DIR}/scripts/release/rr-install.sh"

if [[ ! -f "${INSTALL_SCRIPT}" ]]; then
  echo "missing installer script: ${INSTALL_SCRIPT}" >&2
  exit 1
fi

sha256_file() {
  local path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
  else
    shasum -a 256 "$path" | awk '{print $1}'
  fi
}

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

DOWNLOAD_ROOT="file://${TMP_DIR}/releases/download"
DOWNLOAD_FS_ROOT="${TMP_DIR}/releases/download"
TARGET="x86_64-unknown-linux-gnu"

make_release_payload() {
  local version="$1"
  local ambiguous="${2:-0}"
  local tag="v${version}"
  local artifact_stem="roger-reviewer-${version}"
  local payload_dir="${artifact_stem}-core-${TARGET}"
  local archive_name="${payload_dir}.tar.gz"
  local release_dir="${DOWNLOAD_FS_ROOT}/${tag}"
  local checksums_name="${artifact_stem}-checksums.txt"
  local install_metadata_name="release-install-metadata-${version}.json"
  local core_manifest_name="release-core-manifest-${version}.json"

  mkdir -p "${release_dir}" "${TMP_DIR}/payload-${version}/${payload_dir}"
  cat >"${TMP_DIR}/payload-${version}/${payload_dir}/rr" <<'EOF'
#!/usr/bin/env bash
echo "rr smoke ok"
EOF
  chmod +x "${TMP_DIR}/payload-${version}/${payload_dir}/rr"

  tar -czf "${release_dir}/${archive_name}" -C "${TMP_DIR}/payload-${version}" "${payload_dir}"
  local archive_sha
  archive_sha="$(sha256_file "${release_dir}/${archive_name}")"

  cat >"${release_dir}/${checksums_name}" <<EOF
${archive_sha}  ${archive_name}
EOF

  if [[ "${ambiguous}" == "1" ]]; then
    cat >"${release_dir}/${install_metadata_name}" <<EOF
{
  "schema": "roger.release.install-metadata.v1",
  "release": {
    "channel": "stable",
    "version": "${version}",
    "tag": "v${version}",
    "prerelease": false,
    "artifact_stem": "${artifact_stem}"
  },
  "checksums_name": "${checksums_name}",
  "core_manifest_name": "${core_manifest_name}",
  "targets": [
    {
      "target": "${TARGET}",
      "archive_name": "${archive_name}",
      "archive_sha256": "${archive_sha}",
      "payload_dir": "${payload_dir}",
      "binary_name": "rr"
    },
    {
      "target": "${TARGET}",
      "archive_name": "${archive_name}",
      "archive_sha256": "${archive_sha}",
      "payload_dir": "${payload_dir}",
      "binary_name": "rr"
    }
  ]
}
EOF
  else
    cat >"${release_dir}/${install_metadata_name}" <<EOF
{
  "schema": "roger.release.install-metadata.v1",
  "release": {
    "channel": "stable",
    "version": "${version}",
    "tag": "v${version}",
    "prerelease": false,
    "artifact_stem": "${artifact_stem}"
  },
  "checksums_name": "${checksums_name}",
  "core_manifest_name": "${core_manifest_name}",
  "targets": [
    {
      "target": "${TARGET}",
      "archive_name": "${archive_name}",
      "archive_sha256": "${archive_sha}",
      "payload_dir": "${payload_dir}",
      "binary_name": "rr"
    }
  ]
}
EOF
  fi

  cat >"${release_dir}/${core_manifest_name}" <<EOF
{
  "schema": "roger.release-build-core.v1",
  "version": "${version}",
  "artifact_stem": "${artifact_stem}",
  "targets": [
    {
      "target": "${TARGET}",
      "archive_name": "${archive_name}",
      "archive_sha256": "${archive_sha}",
      "payload_dir": "${payload_dir}",
      "binary_name": "rr"
    }
  ]
}
EOF
}

make_release_payload "2026.04.01" 0

INSTALL_DIR="${TMP_DIR}/install/bin"
bash "${INSTALL_SCRIPT}" \
  --version "2026.04.01" \
  --download-root "${DOWNLOAD_ROOT}" \
  --install-dir "${INSTALL_DIR}" \
  --target "${TARGET}"

[[ -x "${INSTALL_DIR}/rr" ]] || { echo "install did not create executable rr" >&2; exit 1; }
[[ "$("${INSTALL_DIR}/rr")" == "rr smoke ok" ]] || { echo "installed rr smoke output mismatch" >&2; exit 1; }

if bash "${INSTALL_SCRIPT}" \
  --version "2026.04.02" \
  --download-root "${DOWNLOAD_ROOT}" \
  --install-dir "${TMP_DIR}/missing/bin" \
  --target "${TARGET}" >/dev/null 2>&1; then
  echo "expected missing metadata install to fail closed" >&2
  exit 1
fi

make_release_payload "2026.04.03" 1
if bash "${INSTALL_SCRIPT}" \
  --version "2026.04.03" \
  --download-root "${DOWNLOAD_ROOT}" \
  --install-dir "${TMP_DIR}/ambiguous/bin" \
  --target "${TARGET}" >/dev/null 2>&1; then
  echo "expected ambiguous metadata install to fail closed" >&2
  exit 1
fi

# Legacy release payload fallback: metadata expects <artifact>-checksums.txt,
# but only SHA256SUMS is present in the release assets.
make_release_payload "2026.04.04" 0
archive_name_0404="roger-reviewer-2026.04.04-core-${TARGET}.tar.gz"
archive_sha_0404="$(sha256_file "${DOWNLOAD_FS_ROOT}/v2026.04.04/${archive_name_0404}")"
mv \
  "${DOWNLOAD_FS_ROOT}/v2026.04.04/roger-reviewer-2026.04.04-checksums.txt" \
  "${DOWNLOAD_FS_ROOT}/v2026.04.04/SHA256SUMS"
cat >"${DOWNLOAD_FS_ROOT}/v2026.04.04/SHA256SUMS" <<EOF
${archive_sha_0404}  core-${TARGET}/${archive_name_0404}
EOF

bash "${INSTALL_SCRIPT}" \
  --version "2026.04.04" \
  --download-root "${DOWNLOAD_ROOT}" \
  --install-dir "${TMP_DIR}/fallback/bin" \
  --target "${TARGET}"

[[ -x "${TMP_DIR}/fallback/bin/rr" ]] || { echo "fallback install did not create executable rr" >&2; exit 1; }
[[ "$("${TMP_DIR}/fallback/bin/rr")" == "rr smoke ok" ]] || { echo "fallback-installed rr smoke output mismatch" >&2; exit 1; }

# Stable-channel lookup should resolve tag via API payload and install without --version.
API_FS_ROOT="${TMP_DIR}/api/repos/cdilga/roger-reviewer/releases"
mkdir -p "${API_FS_ROOT}"
cat >"${API_FS_ROOT}/latest" <<'EOF'
{
  "tag_name": "v2026.04.04"
}
EOF

bash "${INSTALL_SCRIPT}" \
  --version "0.1.0" \
  --api-root "file://${TMP_DIR}/api/repos/cdilga/roger-reviewer" \
  --download-root "${DOWNLOAD_ROOT}" \
  --install-dir "${TMP_DIR}/semver-alias/bin" \
  --target "${TARGET}"

[[ -x "${TMP_DIR}/semver-alias/bin/rr" ]] || { echo "0.1.0 alias install did not create executable rr" >&2; exit 1; }
[[ "$("${TMP_DIR}/semver-alias/bin/rr")" == "rr smoke ok" ]] || { echo "0.1.0 alias rr smoke output mismatch" >&2; exit 1; }

if bash "${INSTALL_SCRIPT}" \
  --version "0.2.0" \
  --download-root "${DOWNLOAD_ROOT}" \
  --install-dir "${TMP_DIR}/bad-semver/bin" \
  --target "${TARGET}" >/dev/null 2>&1; then
  echo "expected unsupported semver alias to fail closed" >&2
  exit 1
fi

bash "${INSTALL_SCRIPT}" \
  --channel stable \
  --api-root "file://${TMP_DIR}/api/repos/cdilga/roger-reviewer" \
  --download-root "${DOWNLOAD_ROOT}" \
  --install-dir "${TMP_DIR}/stable/bin" \
  --target "${TARGET}"

[[ -x "${TMP_DIR}/stable/bin/rr" ]] || { echo "stable-channel install did not create executable rr" >&2; exit 1; }
[[ "$("${TMP_DIR}/stable/bin/rr")" == "rr smoke ok" ]] || { echo "stable-channel rr smoke output mismatch" >&2; exit 1; }

echo "rr-install.sh smoke: ok"
