#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/release/package_bridge_bundle.sh \
    --version-metadata <path> \
    --os <macos|windows|linux> \
    [--output-dir <path>] \
    [--bridge-binary-placeholder <path-token>] \
    [--extension-id-placeholder <id-token>]
EOF
}

version_metadata=""
target_os=""
output_dir=""
bridge_binary_placeholder="__RR_BRIDGE_BINARY__"
extension_id_placeholder="__RR_EXTENSION_ID__"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version-metadata)
      version_metadata="${2:-}"
      shift 2
      ;;
    --os)
      target_os="${2:-}"
      shift 2
      ;;
    --output-dir)
      output_dir="${2:-}"
      shift 2
      ;;
    --bridge-binary-placeholder)
      bridge_binary_placeholder="${2:-}"
      shift 2
      ;;
    --extension-id-placeholder)
      extension_id_placeholder="${2:-}"
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

if [[ -z "$version_metadata" || -z "$target_os" ]]; then
  echo "error: --version-metadata and --os are required" >&2
  usage >&2
  exit 2
fi
if [[ ! -f "$version_metadata" ]]; then
  echo "error: version metadata not found: $version_metadata" >&2
  exit 2
fi
case "$target_os" in
  macos|windows|linux) ;;
  *)
    echo "error: unsupported --os value: $target_os" >&2
    exit 2
    ;;
esac

output_dir="${output_dir:-dist/bridge}"
mkdir -p "$output_dir"

version="$(jq -r '.version // empty' "$version_metadata")"
tag="$(jq -r '.tag // empty' "$version_metadata")"
channel="$(jq -r '.channel // empty' "$version_metadata")"
prerelease="$(jq -r '.prerelease // false' "$version_metadata")"
artifact_stem="$(jq -r '.artifact_stem // empty' "$version_metadata")"

if [[ -z "$version" || -z "$tag" || -z "$channel" || -z "$artifact_stem" ]]; then
  echo "error: version metadata missing required keys" >&2
  exit 2
fi

bundle_name="${artifact_stem}-bridge-${target_os}"
bundle_dir="${output_dir}/${bundle_name}"
rm -rf "$bundle_dir"
mkdir -p "$bundle_dir/templates" "$bundle_dir/helpers" "$bundle_dir/scripts"

manifest_path_for_browser() {
  local os="$1"
  local browser="$2"
  case "$os:$browser" in
    macos:chrome)
      echo '__HOME__/Library/Application Support/Google/Chrome/NativeMessagingHosts/com.roger_reviewer.bridge.json'
      ;;
    macos:edge)
      echo '__HOME__/Library/Application Support/Microsoft Edge/NativeMessagingHosts/com.roger_reviewer.bridge.json'
      ;;
    macos:brave)
      echo '__HOME__/Library/Application Support/BraveSoftware/Brave-Browser/NativeMessagingHosts/com.roger_reviewer.bridge.json'
      ;;
    windows:chrome)
      echo '__HOME__/AppData/Local/Google/Chrome/User Data/NativeMessagingHosts/com.roger_reviewer.bridge.json'
      ;;
    windows:edge)
      echo '__HOME__/AppData/Local/Microsoft/Edge/User Data/NativeMessagingHosts/com.roger_reviewer.bridge.json'
      ;;
    windows:brave)
      echo '__HOME__/AppData/Local/BraveSoftware/Brave-Browser/User Data/NativeMessagingHosts/com.roger_reviewer.bridge.json'
      ;;
    linux:chrome)
      echo '__HOME__/.config/google-chrome/NativeMessagingHosts/com.roger_reviewer.bridge.json'
      ;;
    linux:edge)
      echo '__HOME__/.config/microsoft-edge/NativeMessagingHosts/com.roger_reviewer.bridge.json'
      ;;
    linux:brave)
      echo '__HOME__/.config/BraveSoftware/Brave-Browser/NativeMessagingHosts/com.roger_reviewer.bridge.json'
      ;;
    *)
      echo "error: unsupported browser mapping: ${os}:${browser}" >&2
      exit 2
      ;;
  esac
}

render_native_manifest_template() {
  local browser="$1"
  local out="$2"
  local install_path="$3"
  cat >"$out" <<EOF
{
  "name": "com.roger_reviewer.bridge",
  "description": "Roger Reviewer browser-to-local launch bridge",
  "path": "${bridge_binary_placeholder}",
  "type": "stdio",
  "allowed_origins": [
    "chrome-extension://${extension_id_placeholder}/"
  ],
  "install_path": "${install_path}"
}
EOF
}

for browser in chrome edge brave; do
  install_path="$(manifest_path_for_browser "$target_os" "$browser")"
  render_native_manifest_template \
    "$browser" \
    "${bundle_dir}/templates/com.roger_reviewer.bridge.${browser}.json" \
    "$install_path"
done

helper_filename=""
case "$target_os" in
  macos)
    helper_filename="register-roger-url.command"
    cat >"${bundle_dir}/helpers/${helper_filename}" <<EOF
#!/usr/bin/env sh
# Roger custom URL registration helper template (macOS)
# Replace ${bridge_binary_placeholder} with your installed rr binary path before use.
# This helper is explicit and not auto-run.
# Example launch command:
#   ${bridge_binary_placeholder} "roger://launch/<owner>/<repo>/<pr>"
EOF
    chmod +x "${bundle_dir}/helpers/${helper_filename}"

    cat >"${bundle_dir}/scripts/install.sh" <<'EOF'
#!/usr/bin/env sh
set -eu

extension_id="${1:-}"
bridge_binary="${2:-}"
if [ -z "$extension_id" ] || [ -z "$bridge_binary" ]; then
  echo "usage: install.sh <extension-id> <bridge-binary-path>" >&2
  exit 2
fi

rr bridge install --extension-id "$extension_id" --bridge-binary "$bridge_binary"
EOF
    chmod +x "${bundle_dir}/scripts/install.sh"

    cat >"${bundle_dir}/scripts/uninstall.sh" <<'EOF'
#!/usr/bin/env sh
set -eu
rr bridge uninstall
EOF
    chmod +x "${bundle_dir}/scripts/uninstall.sh"
    ;;
  windows)
    helper_filename="register-roger-url.reg"
    cat >"${bundle_dir}/helpers/${helper_filename}" <<EOF
Windows Registry Editor Version 5.00

[HKEY_CURRENT_USER\\Software\\Classes\\roger]
@="URL:Roger Protocol"
"URL Protocol"=""

[HKEY_CURRENT_USER\\Software\\Classes\\roger\\shell\\open\\command]
@="\"${bridge_binary_placeholder}\" \"%1\""
EOF

    cat >"${bundle_dir}/scripts/install.ps1" <<'EOF'
param(
  [Parameter(Mandatory = $true)][string]$ExtensionId,
  [Parameter(Mandatory = $true)][string]$BridgeBinary
)

rr bridge install --extension-id $ExtensionId --bridge-binary $BridgeBinary
EOF

    cat >"${bundle_dir}/scripts/uninstall.ps1" <<'EOF'
rr bridge uninstall
EOF
    ;;
  linux)
    helper_filename="register-roger-url.desktop"
    cat >"${bundle_dir}/helpers/${helper_filename}" <<EOF
[Desktop Entry]
Name=Roger Reviewer URL Handler
Type=Application
NoDisplay=true
MimeType=x-scheme-handler/roger;
Exec=${bridge_binary_placeholder} %u
EOF

    cat >"${bundle_dir}/scripts/install.sh" <<'EOF'
#!/usr/bin/env sh
set -eu

extension_id="${1:-}"
bridge_binary="${2:-}"
if [ -z "$extension_id" ] || [ -z "$bridge_binary" ]; then
  echo "usage: install.sh <extension-id> <bridge-binary-path>" >&2
  exit 2
fi

rr bridge install --extension-id "$extension_id" --bridge-binary "$bridge_binary"
EOF
    chmod +x "${bundle_dir}/scripts/install.sh"

    cat >"${bundle_dir}/scripts/uninstall.sh" <<'EOF'
#!/usr/bin/env sh
set -eu
rr bridge uninstall
EOF
    chmod +x "${bundle_dir}/scripts/uninstall.sh"
    ;;
esac

cat >"${bundle_dir}/README.md" <<EOF
# Roger Bridge Registration Bundle (${target_os})

Generated by the \`release-package-bridge\` workflow lane.

Contents:
- Native Messaging host manifest templates for Chrome, Edge, and Brave
- OS-specific custom-URL helper template
- Thin install/uninstall wrappers that call \`rr bridge install\` and \`rr bridge uninstall\`

Usage:
1. Replace placeholder values:
   - bridge binary path token: ${bridge_binary_placeholder}
   - extension id token: ${extension_id_placeholder}
2. Run the install helper from \`scripts/\` to register host assets on this OS.
3. Use the uninstall helper to remove Roger-owned bridge registration assets.

Safety:
- This bundle registers host assets only.
- Browser extension install/update remains a separate manual or Roger-owned lane.
EOF

cat >"${bundle_dir}/bridge-bundle-manifest.json" <<EOF
{
  "schema": "roger.release.bridge_bundle.v1",
  "lane": "release-package-bridge",
  "target_os": "${target_os}",
  "channel": "${channel}",
  "version": "${version}",
  "tag": "${tag}",
  "prerelease": ${prerelease},
  "artifact_stem": "${artifact_stem}",
  "bundle_name": "${bundle_name}",
  "native_manifest_templates": [
    {
      "browser": "chrome",
      "path": "templates/com.roger_reviewer.bridge.chrome.json"
    },
    {
      "browser": "edge",
      "path": "templates/com.roger_reviewer.bridge.edge.json"
    },
    {
      "browser": "brave",
      "path": "templates/com.roger_reviewer.bridge.brave.json"
    }
  ],
  "helper_template": "helpers/${helper_filename}"
}
EOF

python3 - "$bundle_dir" <<'PY'
import hashlib
import pathlib
import sys

bundle = pathlib.Path(sys.argv[1])
lines = []
for path in sorted(p for p in bundle.rglob("*") if p.is_file()):
    if path.name in {"SHA256SUMS", "asset-manifest.json"}:
        continue
    rel = path.relative_to(bundle).as_posix()
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    lines.append(f"{digest}  {rel}")
(bundle / "SHA256SUMS").write_text("\n".join(lines) + "\n", encoding="utf-8")

checksums = (bundle / "SHA256SUMS").read_bytes()
summary = {
    "schema": "roger.release.bridge_bundle_assets.v1",
    "bundle_dir": bundle.name,
    "checksums_path": "SHA256SUMS",
    "package_digest_sha256": hashlib.sha256(checksums).hexdigest(),
}
import json
(bundle / "asset-manifest.json").write_text(
    json.dumps(summary, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY

archive_path="${output_dir}/${bundle_name}.tar.gz"
tar -czf "$archive_path" -C "$output_dir" "$bundle_name"

python3 - "$archive_path" "$bundle_dir" <<'PY'
import json
import pathlib
import sys

archive = pathlib.Path(sys.argv[1])
bundle_dir = pathlib.Path(sys.argv[2])
manifest = {
    "archive_path": archive.as_posix(),
    "bundle_manifest": (bundle_dir / "bridge-bundle-manifest.json").as_posix(),
    "asset_manifest": (bundle_dir / "asset-manifest.json").as_posix(),
    "checksums": (bundle_dir / "SHA256SUMS").as_posix(),
}
print(json.dumps(manifest))
PY
