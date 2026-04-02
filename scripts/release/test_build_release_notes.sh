#!/usr/bin/env bash
set -euo pipefail

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

manifest="${workdir}/release-asset-manifest.json"
checksums="${workdir}/SHA256SUMS"
signing="${workdir}/release-notes-signing.md"
output="${workdir}/RELEASE_NOTES.md"

cat >"$manifest" <<'EOF'
{
  "schema": "roger.release-verify-assets.v1",
  "release": {
    "channel": "stable",
    "version": "2026.04.01",
    "tag": "v2026.04.01",
    "prerelease": false,
    "artifact_stem": "roger-reviewer-2026.04.01"
  },
  "core": {
    "assets": [
      {
        "lane": "release-build-core",
        "kind": "core_archive",
        "label": "x86_64-unknown-linux-gnu",
        "path": "roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz",
        "sha256": "abc123",
        "bytes": 12345
      }
    ]
  },
  "optional_lanes": {
    "lane_summary": {
      "support_claims": {
        "posture": "core_plus_bridge",
        "shipped_optional_lanes": ["release-package-bridge"],
        "narrowed_claims": ["extension_sideload_unshipped"]
      }
    },
    "assets": [
      {
        "lane": "release-package-bridge",
        "kind": "optional_lane_asset",
        "label": "bridge-bundle-linux",
        "path": "roger-reviewer-2026.04.01-bridge-linux.tar.gz",
        "sha256": "def456",
        "bytes": 111
      }
    ]
  },
  "checksums": {
    "path": "SHA256SUMS",
    "entries": 2
  },
  "publish_gate": {
    "publish_allowed": true
  },
  "warnings": [],
  "failures": []
}
EOF

cat >"$checksums" <<'EOF'
abc123  roger-reviewer-2026.04.01-core-x86_64-unknown-linux-gnu.tar.gz
def456  roger-reviewer-2026.04.01-bridge-linux.tar.gz
EOF

cat >"$signing" <<'EOF'
# Signing Status

Unsigned targets (placeholder surfaced explicitly in verify-assets lane):
- `x86_64-unknown-linux-gnu` (unsigned_placeholder)
EOF

bash scripts/release/build_release_notes.sh \
  --manifest "$manifest" \
  --checksums "$checksums" \
  --signing-notes "$signing" \
  --output "$output" \
  --publish-mode draft \
  --verify-run-id 123456789 \
  --verify-run-url https://github.com/example/repo/actions/runs/123456789

grep -q "Roger Reviewer v2026.04.01" "$output"
grep -q 'Publish mode: `draft`' "$output"
grep -q 'Verified workflow run: `123456789`' "$output"
grep -q 'Verified workflow URL: `https://github.com/example/repo/actions/runs/123456789`' "$output"
grep -q 'Publish gate: `true`' "$output"
grep -q 'Posture: `core_plus_bridge`' "$output"
grep -q 'Attached checksum manifest: `SHA256SUMS`' "$output"
grep -q 'extension_sideload_unshipped' "$output"
grep -q 'release-package-bridge/bridge-bundle-linux' "$output"
grep -q 'unsigned_placeholder' "$output"

echo "test_build_release_notes: PASS"
