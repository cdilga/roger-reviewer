#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: test_update_upgrade_rehearsal.sh [--output-dir <path>]

Build two synthetic published Roger releases, install release N through the
official installer, run the installed old binary, update to release N+1, and
verify the replaced binary is still usable and now reports a same-version no-op
against the new synthetic release.

Options:
  --output-dir <path>   Directory for preserved rehearsal artifacts
  -h, --help            Show this help
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "${os}:${arch}" in
    Darwin:arm64|Darwin:aarch64)
      echo "aarch64-apple-darwin"
      ;;
    Darwin:x86_64)
      echo "x86_64-apple-darwin"
      ;;
    Linux:x86_64|Linux:amd64)
      echo "x86_64-unknown-linux-gnu"
      ;;
    Linux:arm64|Linux:aarch64)
      echo "aarch64-unknown-linux-gnu"
      ;;
    *)
      die "unsupported host platform: ${os}/${arch}; pass a representative target via a future dedicated lane"
      ;;
  esac
}

sha256_file() {
  python3 - "$1" <<'PY'
import hashlib
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
print(hashlib.sha256(path.read_bytes()).hexdigest())
PY
}

sanitize_key() {
  echo "$1" | tr -c 'A-Za-z0-9' '_'
}

build_rr_binary() {
  local version="$1"
  local channel="$2"
  local tag="$3"
  local output_path="$4"
  local build_key
  build_key="$(sanitize_key "${version}-${channel}")"
  local target_dir="${WORKDIR}/target-${build_key}"

  (
    cd "${ROOT_DIR}" && \
      CARGO_TARGET_DIR="${target_dir}" \
      ROGER_RELEASE_VERSION="${version}" \
      ROGER_RELEASE_CHANNEL="${channel}" \
      ROGER_RELEASE_TAG="${tag}" \
      cargo build -q -p roger-cli --bin rr
  )

  cp "${target_dir}/debug/rr" "${output_path}"
  chmod +x "${output_path}"
}

create_release_payload() {
  local version="$1"
  local channel="$2"
  local binary_source="$3"

  local tag="v${version}"
  local artifact_stem="roger-reviewer-${version}"
  local payload_dir="${artifact_stem}-core-${TARGET}"
  local archive_name="${payload_dir}.tar.gz"
  local legacy_checksums_name="${artifact_stem}-checksums.txt"
  local release_dir="${DOWNLOAD_FS_ROOT}/${tag}"
  local payload_root="${WORKDIR}/payload-${version}"
  local payload_dir_path="${payload_root}/${payload_dir}"
  local install_metadata_name="release-install-metadata-${version}.json"
  local core_manifest_name="release-core-manifest-${version}.json"

  mkdir -p "${release_dir}" "${payload_dir_path}"
  cp "${binary_source}" "${payload_dir_path}/rr"
  chmod +x "${payload_dir_path}/rr"

  tar -czf "${release_dir}/${archive_name}" -C "${payload_root}" "${payload_dir}"
  local archive_sha
  archive_sha="$(sha256_file "${release_dir}/${archive_name}")"

  cat >"${release_dir}/SHA256SUMS" <<EOF
${archive_sha}  core-${TARGET}/${archive_name}
EOF

  cat >"${release_dir}/${install_metadata_name}" <<EOF
{
  "schema": "roger.release.install-metadata.v1",
  "release": {
    "channel": "${channel}",
    "version": "${version}",
    "tag": "${tag}",
    "prerelease": false,
    "artifact_stem": "${artifact_stem}"
  },
  "checksums_name": "${legacy_checksums_name}",
  "core_manifest_name": "${core_manifest_name}",
  "targets": [
    {
      "target": "${TARGET}",
      "archive_name": "${archive_name}",
      "archive_sha256": "${archive_sha}",
      "payload_dir": "${payload_dir}",
      "binary_name": "rr"
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

  cat >"${release_dir}/${core_manifest_name}" <<EOF
{
  "schema": "roger.release-build-core.v1",
  "channel": "${channel}",
  "version": "${version}",
  "tag": "${tag}",
  "prerelease": false,
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

write_latest_release_pointer() {
  local tag="$1"
  mkdir -p "${API_ROOT_FS}/releases"
  cat >"${API_ROOT_FS}/releases/latest" <<EOF
{
  "tag_name": "${tag}"
}
EOF
}

assert_robot_payload_common() {
  local json_path="$1"
  local expected_outcome="$2"
  local expected_target_version="$3"

  python3 - "$json_path" "$expected_outcome" "$expected_target_version" <<'PY'
import json
import pathlib
import sys

json_path, expected_outcome, expected_target_version = sys.argv[1:4]
payload = json.loads(pathlib.Path(json_path).read_text(encoding="utf-8"))

if payload.get("schema_id") != "rr.robot.update.v1":
    raise SystemExit(f"unexpected schema_id: {payload.get('schema_id')!r}")
if payload.get("outcome") != expected_outcome:
    raise SystemExit(
        f"unexpected outcome for {json_path}: expected {expected_outcome!r}, got {payload.get('outcome')!r}"
    )

data = payload.get("data") or {}
target_release = data.get("target_release") or {}
observed_target_version = target_release.get("version")
if expected_target_version == "null":
    if observed_target_version is not None:
        raise SystemExit(
            f"expected null target_release.version for {json_path}, got {observed_target_version!r}"
        )
else:
    if observed_target_version != expected_target_version:
        raise SystemExit(
            f"unexpected target_release.version for {json_path}: expected {expected_target_version!r}, got {observed_target_version!r}"
        )
PY
}

assert_same_version_noop_payload() {
  local json_path="$1"
  local expected_current_version="$2"

  assert_robot_payload_common "${json_path}" "empty" "null"

  python3 - "$json_path" "$expected_current_version" <<'PY'
import json
import pathlib
import sys

json_path, expected_current_version = sys.argv[1:3]
payload = json.loads(pathlib.Path(json_path).read_text(encoding="utf-8"))
data = payload.get("data") or {}

if data.get("up_to_date") is not True:
    raise SystemExit(
        f"unexpected up_to_date for {json_path}: expected True, got {data.get('up_to_date')!r}"
    )

if data.get("current_version") != expected_current_version:
    raise SystemExit(
        f"unexpected current_version for {json_path}: expected {expected_current_version!r}, got {data.get('current_version')!r}"
    )

current_release = data.get("current_release") or {}
if current_release.get("version") != expected_current_version:
    raise SystemExit(
        f"unexpected current_release.version for {json_path}: expected {expected_current_version!r}, got {current_release.get('version')!r}"
    )

confirmation = data.get("confirmation") or {}
if confirmation.get("mode") != "not_required_up_to_date":
    raise SystemExit(
        f"unexpected confirmation.mode for {json_path}: expected 'not_required_up_to_date', got {confirmation.get('mode')!r}"
    )
PY
}

assert_apply_payload() {
  local json_path="$1"
  local expected_current_version="$2"
  local expected_target_version="$3"

  assert_robot_payload_common "${json_path}" "complete" "${expected_target_version}"

  python3 - "$json_path" "$expected_current_version" "$expected_target_version" <<'PY'
import json
import pathlib
import sys

json_path, expected_current_version, expected_target_version = sys.argv[1:4]
payload = json.loads(pathlib.Path(json_path).read_text(encoding="utf-8"))
data = payload.get("data") or {}

if "up_to_date" in data:
    raise SystemExit(f"did not expect up_to_date field for apply payload in {json_path}")

current_release = data.get("current_release") or {}
if current_release.get("version") != expected_current_version:
    raise SystemExit(
        f"unexpected current_release.version for {json_path}: expected {expected_current_version!r}, got {current_release.get('version')!r}"
    )

if data.get("mode") != "in_place_apply":
    raise SystemExit(
        f"unexpected mode for {json_path}: expected 'in_place_apply', got {data.get('mode')!r}"
    )

confirmation = data.get("confirmation") or {}
if confirmation.get("required") is not True or confirmation.get("confirmed") is not True:
    raise SystemExit(
        f"unexpected confirmation gate for {json_path}: expected required=true and confirmed=true, got {confirmation!r}"
    )
if confirmation.get("mode") != "yes_flag":
    raise SystemExit(
        f"unexpected confirmation.mode for {json_path}: expected 'yes_flag', got {confirmation.get('mode')!r}"
    )

target_release = data.get("target_release") or {}
if target_release.get("version") != expected_target_version:
    raise SystemExit(
        f"unexpected target_release.version for {json_path}: expected {expected_target_version!r}, got {target_release.get('version')!r}"
    )

apply_block = data.get("apply") or {}
for key in ("install_path", "backup_path"):
    value = apply_block.get(key)
    if not value:
        raise SystemExit(f"missing apply.{key} in {json_path}")
PY
}

write_manifest() {
  local path="$1"
  local old_version="$2"
  local new_version="$3"
  local pre_noop_json="$4"
  local apply_json="$5"
  local post_noop_json="$6"

  python3 - "$path" "$old_version" "$new_version" "$TARGET" "$pre_noop_json" "$apply_json" "$post_noop_json" <<'PY'
import json
import pathlib
import sys

manifest_path, old_version, new_version, target, pre_noop_json, apply_json, post_noop_json = sys.argv[1:8]

def load(path):
    return json.loads(pathlib.Path(path).read_text(encoding="utf-8"))

manifest = {
    "schema": "roger.update.upgrade-rehearsal.v1",
    "channel": "stable",
    "target": target,
    "old_version": old_version,
    "new_version": new_version,
    "checks": {
        "pre_update_same_version_noop": load(pre_noop_json),
        "update_apply": load(apply_json),
        "post_update_same_version_noop": load(post_noop_json),
    },
}

pathlib.Path(manifest_path).write_text(
    json.dumps(manifest, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY
}

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INSTALL_SCRIPT="${ROOT_DIR}/scripts/release/rr-install.sh"
[[ -f "${INSTALL_SCRIPT}" ]] || die "missing installer script: ${INSTALL_SCRIPT}"

OUTPUT_DIR=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --output-dir)
      [[ $# -ge 2 ]] || die "--output-dir requires a path"
      OUTPUT_DIR="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

need_cmd cargo
need_cmd python3
need_cmd tar
need_cmd curl
need_cmd bash

WORKDIR="$(mktemp -d)"
cleanup() {
  local status=$?
  if [[ $status -eq 0 ]]; then
    rm -rf "${WORKDIR}"
  else
    echo "upgrade rehearsal artifacts preserved at ${WORKDIR}" >&2
  fi
}
trap cleanup EXIT

if [[ -n "${OUTPUT_DIR}" ]]; then
  mkdir -p "${OUTPUT_DIR}"
  ARTIFACT_DIR="${OUTPUT_DIR}"
else
  ARTIFACT_DIR="${WORKDIR}/out"
  mkdir -p "${ARTIFACT_DIR}"
fi

TARGET="$(detect_target)"
API_ROOT_FS="${WORKDIR}/api/repos/cdilga/roger-reviewer"
DOWNLOAD_FS_ROOT="${WORKDIR}/releases/download"
INSTALL_ROOT="${WORKDIR}/install/bin"
mkdir -p "${DOWNLOAD_FS_ROOT}" "${INSTALL_ROOT}"

OLD_VERSION="2026.04.07"
NEW_VERSION="2026.04.08"
OLD_TAG="v${OLD_VERSION}"
NEW_TAG="v${NEW_VERSION}"
OLD_BINARY="${WORKDIR}/rr-old"
NEW_BINARY="${WORKDIR}/rr-new"

build_rr_binary "${OLD_VERSION}" "stable" "${OLD_TAG}" "${OLD_BINARY}"
build_rr_binary "${NEW_VERSION}" "stable" "${NEW_TAG}" "${NEW_BINARY}"
create_release_payload "${OLD_VERSION}" "stable" "${OLD_BINARY}"
create_release_payload "${NEW_VERSION}" "stable" "${NEW_BINARY}"

bash "${INSTALL_SCRIPT}" \
  --version "${OLD_VERSION}" \
  --download-root "file://${DOWNLOAD_FS_ROOT}" \
  --install-dir "${INSTALL_ROOT}" \
  --target "${TARGET}"

[[ -x "${INSTALL_ROOT}/rr" ]] || die "installer did not create executable rr"
"${INSTALL_ROOT}/rr" --help >/dev/null

write_latest_release_pointer "${OLD_TAG}"
PRE_NOOP_JSON="${ARTIFACT_DIR}/pre-update-same-version-noop.json"
RR_STORE_ROOT="${WORKDIR}/store-pre-noop" \
  "${INSTALL_ROOT}/rr" update \
    --api-root "file://${API_ROOT_FS}" \
    --download-root "file://${DOWNLOAD_FS_ROOT}" \
    --dry-run \
    --robot >"${PRE_NOOP_JSON}"
assert_same_version_noop_payload "${PRE_NOOP_JSON}" "${OLD_VERSION}"

write_latest_release_pointer "${NEW_TAG}"
APPLY_JSON="${ARTIFACT_DIR}/update-apply.json"
RR_STORE_ROOT="${WORKDIR}/store-apply" \
  "${INSTALL_ROOT}/rr" update \
    --api-root "file://${API_ROOT_FS}" \
    --download-root "file://${DOWNLOAD_FS_ROOT}" \
    --yes \
    --robot >"${APPLY_JSON}"
assert_apply_payload "${APPLY_JSON}" "${OLD_VERSION}" "${NEW_VERSION}"

"${INSTALL_ROOT}/rr" --help >/dev/null

POST_NOOP_JSON="${ARTIFACT_DIR}/post-update-same-version-noop.json"
RR_STORE_ROOT="${WORKDIR}/store-post-noop" \
  "${INSTALL_ROOT}/rr" update \
    --api-root "file://${API_ROOT_FS}" \
    --download-root "file://${DOWNLOAD_FS_ROOT}" \
    --dry-run \
    --robot >"${POST_NOOP_JSON}"
assert_same_version_noop_payload "${POST_NOOP_JSON}" "${NEW_VERSION}"

MANIFEST_PATH="${ARTIFACT_DIR}/update-upgrade-rehearsal-manifest.json"
write_manifest \
  "${MANIFEST_PATH}" \
  "${OLD_VERSION}" \
  "${NEW_VERSION}" \
  "${PRE_NOOP_JSON}" \
  "${APPLY_JSON}" \
  "${POST_NOOP_JSON}"

echo "test_update_upgrade_rehearsal: PASS"
echo "manifest: ${MANIFEST_PATH}"
