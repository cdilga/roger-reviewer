#!/usr/bin/env bash
set -euo pipefail

# macOS ships bash 3.2: the installers must stay free of bash-4-only
# constructs. Static guard so linux CI (bash 5) still catches regressions.
bashism_hits="$(grep -nE 'mapfile|readarray|\$\{[A-Za-z_0-9\[\]]+(,,|\^\^)[^}]*\}|declare -A' "$(dirname "${BASH_SOURCE[0]}")/rr-install.sh" | grep -v '^[0-9]*:#' || true)"
if [ -n "${bashism_hits}" ]; then
  echo "FAIL: rr-install.sh contains bash-4-only constructs (macOS /bin/bash is 3.2):" >&2
  echo "${bashism_hits}" >&2
  exit 1
fi
if [ -x /bin/bash ] && /bin/bash -c 'test "${BASH_VERSINFO[0]}" -le 3' 2>/dev/null; then
  /bin/bash -n "$(dirname "${BASH_SOURCE[0]}")/rr-install.sh" || {
    echo "FAIL: rr-install.sh does not parse under bash 3.2" >&2
    exit 1
  }
fi

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
  local target="${3:-${TARGET}}"
  local metadata_checksums_name="${4:-SHA256SUMS}"
  local published_checksums_name="${5:-${metadata_checksums_name}}"
  local binary_source="${6:-}"
  local tag="v${version}"
  local artifact_stem="roger-reviewer-${version}"
  local payload_dir="${artifact_stem}-core-${target}"
  local archive_name="${payload_dir}.tar.gz"
  local release_dir="${DOWNLOAD_FS_ROOT}/${tag}"
  local install_metadata_name="release-install-metadata-${version}.json"
  local core_manifest_name="release-core-manifest-${version}.json"

  mkdir -p "${release_dir}" "${TMP_DIR}/payload-${version}/${payload_dir}"
  if [[ -n "${binary_source}" ]]; then
    cp "${binary_source}" "${TMP_DIR}/payload-${version}/${payload_dir}/rr"
    chmod +x "${TMP_DIR}/payload-${version}/${payload_dir}/rr"
  else
    cat >"${TMP_DIR}/payload-${version}/${payload_dir}/rr" <<'EOF'
#!/usr/bin/env bash
echo "rr smoke ok"
EOF
    chmod +x "${TMP_DIR}/payload-${version}/${payload_dir}/rr"
  fi

  tar -czf "${release_dir}/${archive_name}" -C "${TMP_DIR}/payload-${version}" "${payload_dir}"
  local archive_sha
  archive_sha="$(sha256_file "${release_dir}/${archive_name}")"

  # Extension lane asset: a real zip whose checksum is published alongside the
  # core archive, mirroring the release layout.
  local extension_source="${TMP_DIR}/extension-source-${version}"
  mkdir -p "${extension_source}/src"
  cat >"${extension_source}/manifest.json" <<'EOF'
{
  "manifest_version": 3,
  "name": "Roger Reviewer",
  "version": "0.0.0.1"
}
EOF
  echo "export const ok = true;" >"${extension_source}/src/main.js"
  local extension_archive="${release_dir}/${artifact_stem}-extension.zip"
  (cd "${extension_source}" && python3 -m zipfile -c "${extension_archive}" manifest.json src)
  local extension_sha
  extension_sha="$(sha256_file "${extension_archive}")"

  cat >"${release_dir}/${published_checksums_name}" <<EOF
${archive_sha}  ${archive_name}
${extension_sha}  ${artifact_stem}-extension.zip
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
  "checksums_name": "${metadata_checksums_name}",
  "core_manifest_name": "${core_manifest_name}",
  "targets": [
    {
      "target": "${target}",
      "archive_name": "${archive_name}",
      "archive_sha256": "${archive_sha}",
      "payload_dir": "${payload_dir}",
      "binary_name": "rr"
    },
    {
      "target": "${target}",
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
  "checksums_name": "${metadata_checksums_name}",
  "core_manifest_name": "${core_manifest_name}",
  "targets": [
    {
      "target": "${target}",
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
      "target": "${target}",
      "archive_name": "${archive_name}",
      "archive_sha256": "${archive_sha}",
      "payload_dir": "${payload_dir}",
      "binary_name": "rr"
    }
  ]
}
EOF
}

build_actual_rr_binary() {
  local output_path="$1"
  local target_dir="${TMP_DIR}/cargo-target-rr-install-smoke"

  (
    cd "${ROOT_DIR}" && \
      CARGO_TARGET_DIR="${target_dir}" cargo build -q -p roger-cli --bin rr
  )

  cp "${target_dir}/debug/rr" "${output_path}"
  chmod +x "${output_path}"
}

make_release_payload "2026.04.01" 0

INSTALL_DIR="${TMP_DIR}/install/bin"
STORE_ROOT_0401="${TMP_DIR}/store-0401"
RR_STORE_ROOT="${STORE_ROOT_0401}" bash "${INSTALL_SCRIPT}" \
  --version "2026.04.01" \
  --download-root "${DOWNLOAD_ROOT}" \
  --install-dir "${INSTALL_DIR}" \
  --target "${TARGET}"

[[ -x "${INSTALL_DIR}/rr" ]] || { echo "install did not create executable rr" >&2; exit 1; }
[[ "$("${INSTALL_DIR}/rr")" == "rr smoke ok" ]] || { echo "installed rr smoke output mismatch" >&2; exit 1; }
[[ -f "${STORE_ROOT_0401}/bridge/extension-package/2026.04.01/roger-extension-unpacked/manifest.json" ]] || {
  echo "install did not unpack the extension package into the installed layout" >&2
  exit 1
}

REAL_RR_SOURCE="${TMP_DIR}/rr-real"
build_actual_rr_binary "${REAL_RR_SOURCE}"
make_release_payload "2026.04.06" 0 "${TARGET}" "SHA256SUMS" "SHA256SUMS" "${REAL_RR_SOURCE}"

REAL_INSTALL_DIR="${TMP_DIR}/real-install/bin"
REAL_STORE="${TMP_DIR}/real-store"
RR_STORE_ROOT="${REAL_STORE}" bash "${INSTALL_SCRIPT}" \
  --version "2026.04.06" \
  --download-root "${DOWNLOAD_ROOT}" \
  --install-dir "${REAL_INSTALL_DIR}" \
  --target "${TARGET}"

REAL_RR="${REAL_INSTALL_DIR}/rr"
[[ -x "${REAL_RR}" ]] || { echo "real install did not create executable rr" >&2; exit 1; }

# The installer auto-bootstraps the Roger store via `rr init --robot`.
[[ -f "${REAL_STORE}/bootstrap/init-marker.v1.json" ]] || {
  echo "install did not auto-bootstrap the Roger store via rr init" >&2
  exit 1
}
[[ -f "${REAL_STORE}/bridge/extension-package/2026.04.06/roger-extension-unpacked/manifest.json" ]] || {
  echo "real install did not unpack the extension package into the installed layout" >&2
  exit 1
}

REAL_WORKSPACE="${TMP_DIR}/real-workspace"
mkdir -p "${REAL_WORKSPACE}" "${REAL_STORE}"
git -C "${REAL_WORKSPACE}" init >/dev/null

(
  cd "${REAL_WORKSPACE}"
  RR_STORE_ROOT="${REAL_STORE}" "${REAL_RR}" init --robot >"${TMP_DIR}/real-init.json"
  RR_STORE_ROOT="${REAL_STORE}" "${REAL_RR}" doctor --robot >"${TMP_DIR}/real-doctor.json"
)

python3 - "${TMP_DIR}/real-init.json" "${TMP_DIR}/real-doctor.json" <<'PY'
import json
import pathlib
import sys

init_path = pathlib.Path(sys.argv[1])
doctor_path = pathlib.Path(sys.argv[2])
init_payload = json.loads(init_path.read_text(encoding="utf-8"))
doctor_payload = json.loads(doctor_path.read_text(encoding="utf-8"))

if init_payload.get("schema_id") != "rr.robot.init.v1":
    raise SystemExit(f"unexpected init schema: {init_payload.get('schema_id')!r}")
if init_payload.get("outcome") != "complete":
    raise SystemExit(f"unexpected init outcome: {init_payload.get('outcome')!r}")
if doctor_payload.get("schema_id") != "rr.robot.doctor.v1":
    raise SystemExit(f"unexpected doctor schema: {doctor_payload.get('schema_id')!r}")
if doctor_payload.get("outcome") != "complete":
    raise SystemExit(f"unexpected doctor outcome: {doctor_payload.get('outcome')!r}")
PY

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
make_release_payload \
  "2026.04.04" \
  0 \
  "${TARGET}" \
  "roger-reviewer-2026.04.04-checksums.txt" \
  "SHA256SUMS"
archive_name_0404="roger-reviewer-2026.04.04-core-${TARGET}.tar.gz"
archive_sha_0404="$(sha256_file "${DOWNLOAD_FS_ROOT}/v2026.04.04/${archive_name_0404}")"
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

# Linux/aarch64 auto-detection path should resolve without requiring --target.
make_release_payload "2026.04.05" 0 "aarch64-unknown-linux-gnu"
MOCK_UNAME_DIR="${TMP_DIR}/mock-uname"
mkdir -p "${MOCK_UNAME_DIR}"
cat >"${MOCK_UNAME_DIR}/uname" <<'EOF'
#!/usr/bin/env bash
if [[ "${1:-}" == "-s" ]]; then
  echo "Linux"
  exit 0
fi
if [[ "${1:-}" == "-m" ]]; then
  echo "aarch64"
  exit 0
fi
echo "Linux"
EOF
chmod +x "${MOCK_UNAME_DIR}/uname"

PATH="${MOCK_UNAME_DIR}:${PATH}" bash "${INSTALL_SCRIPT}" \
  --version "2026.04.05" \
  --download-root "${DOWNLOAD_ROOT}" \
  --install-dir "${TMP_DIR}/linux-aarch64/bin"

[[ -x "${TMP_DIR}/linux-aarch64/bin/rr" ]] || { echo "linux/aarch64 auto-detect install did not create executable rr" >&2; exit 1; }
[[ "$("${TMP_DIR}/linux-aarch64/bin/rr")" == "rr smoke ok" ]] || { echo "linux/aarch64 auto-detect rr smoke output mismatch" >&2; exit 1; }

echo "rr-install.sh smoke: ok"
