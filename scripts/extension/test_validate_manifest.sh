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
      "matches": [
        "https://github.com/*/*/pull/*",
        "https://github.com/*/*/pulls*"
      ],
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

# Restore the full content script so subsequent negative cases fail for the
# reason under test, not the action-mapping check.
cat >"$tmpdir/apps/extension/src/content/main.js" <<'JS'
const supportedActions = ['start_review', 'resume_review', 'show_findings'];
module.exports = { supportedActions };
JS

# The PR-listing route match is load-bearing: without it the landed row controls
# are dead code on /pulls pages. A manifest that only matches /pull/* must fail.
cat >"$tmpdir/apps/extension/manifest.template.json" <<'JSON'
{
  "manifest_version": 3,
  "name": "Roger Reviewer",
  "version": "0.1.0",
  "permissions": ["nativeMessaging", "tabs"],
  "background": { "service_worker": "src/background/main.js" },
  "content_scripts": [
    {
      "matches": ["https://github.com/*/*/pull/*"],
      "js": ["src/content/main.js"]
    }
  ]
}
JSON

if (cd "$tmpdir" && "$repo_root/scripts/extension/validate_manifest.sh") >"$tmpdir/listing.out" 2>"$tmpdir/listing.err"; then
  echo "expected validator to reject a manifest missing the PR-listing match" >&2
  exit 1
fi

if ! rg -q "PR-listing route" "$tmpdir/listing.err"; then
  echo "validator failed for an unexpected reason (expected PR-listing route check)" >&2
  cat "$tmpdir/listing.err" >&2
  exit 1
fi

echo "validate_manifest self-test ok"
