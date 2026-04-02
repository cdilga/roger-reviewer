#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/release/build_release_notes.sh \
    --manifest <release-asset-manifest.json> \
    --checksums <SHA256SUMS> \
    --signing-notes <release-notes-signing.md> \
    --output <release-notes.md> \
    [--publish-mode <draft|publish>] \
    [--verify-run-id <run-id>] \
    [--verify-run-url <run-url>]
EOF
}

manifest_path=""
checksums_path=""
signing_notes_path=""
output_path=""
publish_mode="draft"
verify_run_id="unknown"
verify_run_url=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest)
      manifest_path="${2:-}"
      shift 2
      ;;
    --checksums)
      checksums_path="${2:-}"
      shift 2
      ;;
    --signing-notes)
      signing_notes_path="${2:-}"
      shift 2
      ;;
    --output)
      output_path="${2:-}"
      shift 2
      ;;
    --publish-mode)
      publish_mode="${2:-}"
      shift 2
      ;;
    --verify-run-id)
      verify_run_id="${2:-}"
      shift 2
      ;;
    --verify-run-url)
      verify_run_url="${2:-}"
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

if [[ -z "$manifest_path" || -z "$checksums_path" || -z "$signing_notes_path" || -z "$output_path" ]]; then
  echo "error: --manifest, --checksums, --signing-notes, and --output are required" >&2
  usage >&2
  exit 2
fi
if [[ ! -f "$manifest_path" ]]; then
  echo "error: manifest not found: $manifest_path" >&2
  exit 2
fi
if [[ ! -f "$checksums_path" ]]; then
  echo "error: checksums file not found: $checksums_path" >&2
  exit 2
fi
if [[ ! -f "$signing_notes_path" ]]; then
  echo "error: signing notes file not found: $signing_notes_path" >&2
  exit 2
fi
if [[ "$publish_mode" != "draft" && "$publish_mode" != "publish" ]]; then
  echo "error: --publish-mode must be draft or publish (got: $publish_mode)" >&2
  exit 2
fi
if [[ -n "$verify_run_url" && ! "$verify_run_url" =~ ^https:// ]]; then
  echo "error: --verify-run-url must be an https URL (got: $verify_run_url)" >&2
  exit 2
fi

manifest_tmp="$(mktemp)"
trap 'rm -f "$manifest_tmp"' EXIT
cp "$manifest_path" "$manifest_tmp"

schema="$(jq -r '.schema // empty' "$manifest_tmp")"
if [[ "$schema" != "roger.release-verify-assets.v1" ]]; then
  echo "error: unexpected manifest schema: ${schema:-<missing>}" >&2
  exit 2
fi

tag="$(jq -r '.release.tag // empty' "$manifest_tmp")"
version="$(jq -r '.release.version // empty' "$manifest_tmp")"
channel="$(jq -r '.release.channel // empty' "$manifest_tmp")"
publish_allowed="$(jq -r '.publish_gate.publish_allowed // false' "$manifest_tmp")"
support_posture="$(jq -r '.optional_lanes.lane_summary.support_claims.posture // "core_only"' "$manifest_tmp")"
checksum_entry_count="$(jq -r '.checksums.entries // 0' "$manifest_tmp")"

if [[ -z "$tag" || -z "$version" || -z "$channel" ]]; then
  echo "error: manifest release section is missing required keys (tag/version/channel)" >&2
  exit 2
fi

mk_list() {
  local jq_expr="$1"
  local empty_label="$2"
  local rendered
  rendered="$(jq -r "$jq_expr" "$manifest_tmp")"
  if [[ -z "$rendered" ]]; then
    printf -- "- %s\n" "$empty_label"
  else
    printf '%s\n' "$rendered"
  fi
}

shipped_optional_lanes="$(mk_list '.optional_lanes.lane_summary.support_claims.shipped_optional_lanes[]? | "- `" + . + "`"' "none")"
narrowed_claims="$(mk_list '.optional_lanes.lane_summary.support_claims.narrowed_claims[]? | "- `" + . + "`"' "none")"
core_assets="$(mk_list '.core.assets[]? | "- `" + .label + "`: `" + .path + "` (`" + .sha256 + "`)"' "none")"
optional_assets="$(mk_list '.optional_lanes.assets[]? | "- `" + .lane + "/" + .label + "`: `" + .path + "` (`" + .sha256 + "`)"' "none")"
verify_warnings="$(mk_list '.warnings[]? | "- " + .' "none")"
verify_failures="$(mk_list '.failures[]? | "- " + .' "none")"

checksums_name="$(basename "$checksums_path")"

{
  cat <<EOF
# Roger Reviewer ${tag}

## Release Summary

- Version: \`${version}\`
- Channel: \`${channel}\`
- Publish mode: \`${publish_mode}\`
- Verified workflow run: \`${verify_run_id}\`
- Verified workflow URL: \`${verify_run_url:-not-recorded}\`
- Publish gate: \`${publish_allowed}\`

## Support Lane Truth

- Posture: \`${support_posture}\`
- Shipped optional lanes:
${shipped_optional_lanes}
- Narrowed claims:
${narrowed_claims}

## Checksums

- Attached checksum manifest: \`${checksums_name}\`
- Checksum entries: \`${checksum_entry_count}\`

## Verified Assets

### Core assets
${core_assets}

### Optional-lane assets
${optional_assets}

## Verify Warnings
${verify_warnings}

## Verify Failures
${verify_failures}

## Signing Status

EOF
  cat "$signing_notes_path"
  echo
} >"$output_path"
