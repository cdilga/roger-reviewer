#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

mkdir -p "$tmpdir/apps/extension/src/background" "$tmpdir/apps/extension/src/content"

cat >"$tmpdir/apps/extension/manifest.template.json" <<'JSON'
{
  "manifest_version": 3,
  "name": "Roger Reviewer",
  "version": "0.1.0",
  "permissions": ["nativeMessaging", "tabs"],
  "background": { "service_worker": "src/background/main.js" },
  "content_scripts": [
    {
      "matches": ["https://github.com/*"],
      "js": ["src/content/main.js"]
    }
  ]
}
JSON

cat >"$tmpdir/apps/extension/src/background/main.js" <<'JS'
const supportedActions = ['start_review', 'resume_review', 'show_findings'];
module.exports = { supportedActions };
JS

cat >"$tmpdir/apps/extension/src/content/main.js" <<'JS'
const supportedActions = ['start_review', 'resume_review', 'show_findings'];
module.exports = { supportedActions };
JS

(cd "$tmpdir" && "$repo_root/scripts/extension/validate_manifest.sh")

cat >"$tmpdir/apps/extension/src/content/main.js" <<'JS'
const supportedActions = ['start_review', 'resume_review'];
module.exports = { supportedActions };
JS

if (cd "$tmpdir" && "$repo_root/scripts/extension/validate_manifest.sh") >"$tmpdir/failure.out" 2>"$tmpdir/failure.err"; then
  echo "expected validator to reject a missing supported action" >&2
  exit 1
fi

if ! rg -q "content script missing action mapping: show_findings" "$tmpdir/failure.err"; then
  echo "validator failed for an unexpected reason" >&2
  cat "$tmpdir/failure.err" >&2
  exit 1
fi
