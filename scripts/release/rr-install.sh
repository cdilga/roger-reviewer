#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: rr-install.sh [options]

Install the Roger `rr` binary from published release artifacts.

Options:
  --version <version>       Install an explicit version (for example 0.1.0 or 2026.04.01)
  --channel <stable|rc>     Channel when --version is omitted (default: stable)
  --repo <owner/repo>       GitHub repository slug (default: cdilga/roger-reviewer)
  --api-root <url>          Override GitHub API root
  --download-root <url>     Override release download root
  --install-dir <path>      Destination directory (default: $HOME/.local/bin)
  --target <triple>         Override auto-detected target triple
  --dry-run                 Print resolved metadata and exit without install
  -h, --help                Show this help

Environment overrides:
  RR_INSTALL_REPO
  RR_INSTALL_API_ROOT
  RR_INSTALL_DOWNLOAD_ROOT
  RR_INSTALL_DIR
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

normalize_calver_version() {
  local value="$1"
  value="${value#v}"
  if [[ ! "$value" =~ ^[0-9]{4}\.[0-9]{2}\.[0-9]{2}(-rc\.[0-9]+)?$ ]]; then
    die "invalid CalVer format: ${value} (expected YYYY.MM.DD or YYYY.MM.DD-rc.N)"
  fi
  printf '%s\n' "$value"
}

normalize_requested_version() {
  local value="$1"
  value="${value#v}"
  if [[ "$value" == "0.1.0" ]]; then
    printf '%s\n' "$value"
    return
  fi
  if [[ "$value" =~ ^[0-9]{4}\.[0-9]{2}\.[0-9]{2}(-rc\.[0-9]+)?$ ]]; then
    printf '%s\n' "$value"
    return
  fi
  die "invalid version format: ${value} (expected 0.1.0 alias or YYYY.MM.DD[-rc.N])"
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
    *)
      die "unsupported host platform: ${os}/${arch}; pass --target explicitly"
      ;;
  esac
}

sha256_file() {
  local path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$path" | awk '{print $1}'
  else
    die "neither sha256sum nor shasum is available"
  fi
}

resolve_latest_tag() {
  local api_root="$1"
  local channel="$2"
  local response

  if [[ "$channel" == "stable" ]]; then
    response="$(curl -fsSL "${api_root}/releases/latest")" || die "failed to query latest stable release"
    python3 - "$response" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
tag = payload.get("tag_name")
if not tag:
    raise SystemExit("missing tag_name in GitHub latest release response")
print(tag)
PY
    return
  fi

  response="$(curl -fsSL "${api_root}/releases?per_page=30")" || die "failed to query releases for rc channel"
  python3 - "$response" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
for entry in payload:
    tag = entry.get("tag_name", "")
    if entry.get("prerelease") and "-rc." in tag:
        print(tag)
        raise SystemExit(0)

raise SystemExit("no rc prerelease found in release feed")
PY
}

read_install_metadata_fields() {
  local install_metadata_path="$1"
  local target="$2"
  local version="$3"
  python3 - "$install_metadata_path" "$target" "$version" <<'PY'
import json
import sys

install_metadata_path, target, version = sys.argv[1:4]
with open(install_metadata_path, "r", encoding="utf-8") as handle:
    metadata = json.load(handle)

if metadata.get("schema") != "roger.release.install-metadata.v1":
    raise SystemExit(
        f"install metadata schema mismatch: {metadata.get('schema')!r}"
    )

release = metadata.get("release")
if not isinstance(release, dict):
    raise SystemExit("install metadata missing release object")

release_version = release.get("version")
if release_version != version:
    raise SystemExit(
        f"install metadata version mismatch: expected {version}, got {release_version!r}"
    )

checksums_name = metadata.get("checksums_name")
core_manifest_name = metadata.get("core_manifest_name")
if not isinstance(checksums_name, str) or not checksums_name:
    raise SystemExit("install metadata missing checksums_name")
if "/" in checksums_name or "\\" in checksums_name:
    raise SystemExit("install metadata checksums_name must be a file name")
if not isinstance(core_manifest_name, str) or not core_manifest_name:
    raise SystemExit("install metadata missing core_manifest_name")
if "/" in core_manifest_name or "\\" in core_manifest_name:
    raise SystemExit("install metadata core_manifest_name must be a file name")

matches = [entry for entry in metadata.get("targets", []) if entry.get("target") == target]
if not matches:
    raise SystemExit(f"install metadata has no entry for target {target}")
if len(matches) > 1:
    raise SystemExit(f"install metadata has ambiguous entries for target {target}")

entry = matches[0]
required = ("archive_name", "archive_sha256", "payload_dir", "binary_name")
missing = [key for key in required if not entry.get(key)]
if missing:
    raise SystemExit(
        "install metadata target entry missing required fields: " + ", ".join(missing)
    )

print(checksums_name)
print(core_manifest_name)
print(entry["archive_name"])
print(entry["archive_sha256"])
print(entry["payload_dir"])
print(entry["binary_name"])
PY
}

verify_manifest_target() {
  local manifest_path="$1"
  local target="$2"
  local version="$3"
  local archive_name="$4"
  local archive_sha256="$5"
  local payload_dir="$6"
  local binary_name="$7"
  python3 - "$manifest_path" "$target" "$version" "$archive_name" "$archive_sha256" "$payload_dir" "$binary_name" <<'PY'
import json
import sys

(
    manifest_path,
    target,
    version,
    archive_name,
    archive_sha256,
    payload_dir,
    binary_name,
) = sys.argv[1:8]

with open(manifest_path, "r", encoding="utf-8") as handle:
    manifest = json.load(handle)

manifest_version = manifest.get("version")
if manifest_version != version:
    raise SystemExit(
        f"manifest version mismatch: expected {version}, got {manifest_version!r}"
    )

matches = [entry for entry in manifest.get("targets", []) if entry.get("target") == target]
if not matches:
    raise SystemExit(f"manifest has no entry for target {target}")
if len(matches) > 1:
    raise SystemExit(f"manifest has ambiguous entries for target {target}")

entry = matches[0]
checks = {
    "archive_name": archive_name,
    "payload_dir": payload_dir,
    "binary_name": binary_name,
}
for key, expected in checks.items():
    observed = entry.get(key)
    if observed != expected:
        raise SystemExit(
            f"manifest target mismatch for {key}: expected {expected!r}, got {observed!r}"
        )

observed_sha = str(entry.get("archive_sha256", "")).lower()
if observed_sha != archive_sha256.lower():
    raise SystemExit(
        f"manifest target mismatch for archive_sha256: expected {archive_sha256!r}, got {entry.get('archive_sha256')!r}"
    )
PY
}

read_checksums_entry() {
  local checksums_path="$1"
  local archive_name="$2"
  python3 - "$checksums_path" "$archive_name" <<'PY'
import sys

checksums_path, archive_name = sys.argv[1:3]
matches = []
with open(checksums_path, "r", encoding="utf-8") as handle:
    for raw_line in handle:
        line = raw_line.strip()
        if not line:
            continue
        parts = line.split()
        if len(parts) < 2:
            continue
        candidate_name = parts[-1].lstrip("*")
        candidate_basename = candidate_name.rsplit("/", 1)[-1].rsplit("\\", 1)[-1]
        if candidate_name == archive_name or candidate_basename == archive_name:
            matches.append(parts[0].lower())

if not matches:
    raise SystemExit(f"checksums file missing entry for {archive_name}")
if len(matches) > 1:
    raise SystemExit(f"checksums file has ambiguous entries for {archive_name}")

print(matches[0])
PY
}

version=""
channel="stable"
repo_slug="${RR_INSTALL_REPO:-cdilga/roger-reviewer}"
api_root="${RR_INSTALL_API_ROOT:-}"
download_root="${RR_INSTALL_DOWNLOAD_ROOT:-}"
install_dir="${RR_INSTALL_DIR:-${HOME}/.local/bin}"
target_override=""
dry_run=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)
      [[ $# -ge 2 ]] || die "--version requires a value"
      version="$2"
      shift 2
      ;;
    --channel)
      [[ $# -ge 2 ]] || die "--channel requires a value"
      channel="$2"
      shift 2
      ;;
    --repo)
      [[ $# -ge 2 ]] || die "--repo requires a value"
      repo_slug="$2"
      shift 2
      ;;
    --api-root)
      [[ $# -ge 2 ]] || die "--api-root requires a value"
      api_root="$2"
      shift 2
      ;;
    --download-root)
      [[ $# -ge 2 ]] || die "--download-root requires a value"
      download_root="$2"
      shift 2
      ;;
    --install-dir)
      [[ $# -ge 2 ]] || die "--install-dir requires a value"
      install_dir="$2"
      shift 2
      ;;
    --target)
      [[ $# -ge 2 ]] || die "--target requires a value"
      target_override="$2"
      shift 2
      ;;
    --dry-run)
      dry_run=1
      shift
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

case "$channel" in
  stable|rc) ;;
  *)
    die "unsupported channel: ${channel} (expected stable or rc)"
    ;;
esac

if [[ -z "$api_root" ]]; then
  api_root="https://api.github.com/repos/${repo_slug}"
fi
if [[ -z "$download_root" ]]; then
  download_root="https://github.com/${repo_slug}/releases/download"
fi

need_cmd curl
need_cmd python3
need_cmd tar

if [[ -n "$version" ]]; then
  requested_version="$(normalize_requested_version "$version")"
  if [[ "$requested_version" == "0.1.0" ]]; then
    if [[ "$channel" != "stable" ]]; then
      die "--version 0.1.0 is a stable-only alias; omit --channel or use --channel stable"
    fi
    tag="$(resolve_latest_tag "$api_root" "stable")"
    [[ -n "$tag" ]] || die "failed to resolve stable release for --version 0.1.0 alias"
    version="$(normalize_calver_version "$tag")"
  else
    version="$requested_version"
    tag="v${version}"
  fi
else
  tag="$(resolve_latest_tag "$api_root" "$channel")"
  [[ -n "$tag" ]] || die "failed to resolve release tag"
  version="$(normalize_calver_version "$tag")"
fi

target="${target_override:-$(detect_target)}"
install_metadata_name="release-install-metadata-${version}.json"
install_metadata_url="${download_root}/${tag}/${install_metadata_name}"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

install_metadata_path="${tmp_dir}/${install_metadata_name}"
if ! curl -fsSL "$install_metadata_url" -o "$install_metadata_path"; then
  die "failed to download install metadata bundle: ${install_metadata_url}"
fi

install_metadata_values="$(
  read_install_metadata_fields "$install_metadata_path" "$target" "$version"
)" || die "invalid install metadata bundle"
mapfile -t install_metadata_lines <<<"$install_metadata_values"
(( ${#install_metadata_lines[@]} == 6 )) || die "unexpected install metadata field shape"
checksums_name="${install_metadata_lines[0]}"
manifest_name="${install_metadata_lines[1]}"
archive_name="${install_metadata_lines[2]}"
archive_sha256="${install_metadata_lines[3],,}"
payload_dir="${install_metadata_lines[4]}"
binary_name="${install_metadata_lines[5]}"

manifest_url="${download_root}/${tag}/${manifest_name}"
manifest_path="${tmp_dir}/${manifest_name}"
if ! curl -fsSL "$manifest_url" -o "$manifest_path"; then
  die "failed to download core manifest: ${manifest_url}"
fi

verify_manifest_target \
  "$manifest_path" \
  "$target" \
  "$version" \
  "$archive_name" \
  "$archive_sha256" \
  "$payload_dir" \
  "$binary_name" || die "core manifest does not match install metadata"

checksums_url="${download_root}/${tag}/${checksums_name}"
checksums_path="${tmp_dir}/${checksums_name}"

if ! curl -fsSL "$checksums_url" -o "$checksums_path"; then
  fallback_checksums_name="SHA256SUMS"
  fallback_checksums_url="${download_root}/${tag}/${fallback_checksums_name}"
  fallback_checksums_path="${tmp_dir}/${fallback_checksums_name}"
  if curl -fsSL "$fallback_checksums_url" -o "$fallback_checksums_path"; then
    checksums_name="${fallback_checksums_name}"
    checksums_url="${fallback_checksums_url}"
    checksums_path="${fallback_checksums_path}"
  else
    die "failed to download checksums file: ${checksums_url} (fallback also failed: ${fallback_checksums_url})"
  fi
fi

checksum_index_value="$(read_checksums_entry "$checksums_path" "$archive_name")" || die "invalid checksums file"
if [[ "${checksum_index_value}" != "${archive_sha256}" ]]; then
  die "manifest/checksums mismatch for ${archive_name}"
fi

archive_url="${download_root}/${tag}/${archive_name}"
archive_path="${tmp_dir}/${archive_name}"

if [[ "$dry_run" -eq 1 ]]; then
  cat <<EOF
rr-install dry-run
  version:      ${version}
  tag:          ${tag}
  target:       ${target}
  install_dir:  ${install_dir}
  install_metadata_url: ${install_metadata_url}
  manifest_url: ${manifest_url}
  checksums_url:${checksums_url}
  archive_url:  ${archive_url}
EOF
  exit 0
fi

if ! curl -fsSL "$archive_url" -o "$archive_path"; then
  die "failed to download archive: ${archive_url}"
fi

actual_sha256="$(sha256_file "$archive_path")"
if [[ "${actual_sha256,,}" != "${archive_sha256}" ]]; then
  die "archive checksum mismatch for ${archive_name}"
fi

extract_dir="${tmp_dir}/extract"
mkdir -p "$extract_dir"
tar -xzf "$archive_path" -C "$extract_dir"

binary_source="${extract_dir}/${payload_dir}/${binary_name}"
[[ -f "$binary_source" ]] || die "archive missing expected binary: ${payload_dir}/${binary_name}"

mkdir -p "$install_dir"
install_path="${install_dir}/rr"
cp "$binary_source" "$install_path"
chmod +x "$install_path"

echo "Installed rr ${version} to ${install_path}"
if [[ ":$PATH:" != *":${install_dir}:"* ]]; then
  echo "Note: ${install_dir} is not currently on PATH."
fi
