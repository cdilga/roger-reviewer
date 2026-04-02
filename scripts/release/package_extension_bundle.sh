#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/release/package_extension_bundle.sh \
    --version-metadata <path> \
    [--output-dir <path>]
EOF
}

version_metadata=""
output_dir=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version-metadata)
      version_metadata="${2:-}"
      shift 2
      ;;
    --output-dir)
      output_dir="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$version_metadata" ]]; then
  echo "error: --version-metadata is required" >&2
  usage >&2
  exit 2
fi
if [[ ! -f "$version_metadata" ]]; then
  echo "error: version metadata not found: $version_metadata" >&2
  exit 2
fi

output_dir="${output_dir:-dist/extension}"
mkdir -p "$output_dir"

version="$(jq -r '.version // empty' "$version_metadata")"
artifact_stem="$(jq -r '.artifact_stem // empty' "$version_metadata")"
tag="$(jq -r '.tag // empty' "$version_metadata")"
channel="$(jq -r '.channel // empty' "$version_metadata")"
if [[ -z "$version" || -z "$artifact_stem" || -z "$tag" || -z "$channel" ]]; then
  echo "error: version metadata missing required keys" >&2
  exit 2
fi

verify_json="${output_dir}/bridge-verify.json"
pack_json="${output_dir}/pack-extension.json"

cargo run -q -p roger-cli --bin rr -- bridge verify-contracts --robot >"$verify_json"
cargo run -q -p roger-cli --bin rr -- bridge pack-extension --output-dir "${output_dir}/unpacked" --robot >"$pack_json"

package_dir="$(jq -r '.data.package_dir // empty' "$pack_json")"
if [[ -z "$package_dir" || ! -d "$package_dir" ]]; then
  echo "error: rr bridge pack-extension did not produce package_dir" >&2
  exit 2
fi

archive_path="${output_dir}/${artifact_stem}-extension.zip"
python3 - "$package_dir" "$archive_path" <<'PY'
import pathlib
import sys
import zipfile

source = pathlib.Path(sys.argv[1])
archive = pathlib.Path(sys.argv[2])
archive.parent.mkdir(parents=True, exist_ok=True)

with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as zf:
    for path in sorted(p for p in source.rglob("*") if p.is_file()):
        zf.write(path, path.relative_to(source).as_posix())
PY

python3 - "$archive_path" "$verify_json" "$pack_json" "$output_dir" "$version" "$tag" "$channel" <<'PY'
import hashlib
import json
import pathlib
import sys

archive = pathlib.Path(sys.argv[1])
verify_json = pathlib.Path(sys.argv[2])
pack_json = pathlib.Path(sys.argv[3])
output_dir = pathlib.Path(sys.argv[4])
version = sys.argv[5]
tag = sys.argv[6]
channel = sys.argv[7]

manifest = {
    "schema": "roger.release.extension_bundle.v1",
    "lane": "release-package-extension",
    "channel": channel,
    "version": version,
    "tag": tag,
    "archive_name": archive.name,
    "archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
    "verify_contract_result": verify_json.name,
    "pack_result": pack_json.name,
}
manifest_path = output_dir / "extension-bundle-manifest.json"
manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")

print(json.dumps({
    "archive_path": archive.as_posix(),
    "manifest_path": manifest_path.as_posix(),
    "verify_result": verify_json.as_posix(),
    "pack_result": pack_json.as_posix(),
}))
PY
