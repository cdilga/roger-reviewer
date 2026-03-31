#!/usr/bin/env bash
set -euo pipefail

manifest_path="${1:-apps/extension/manifest.template.json}"
background_path="apps/extension/src/background/main.js"
content_path="apps/extension/src/content/main.js"

if [[ ! -f "$manifest_path" ]]; then
  echo "missing manifest: $manifest_path" >&2
  exit 1
fi
if [[ ! -f "$background_path" ]]; then
  echo "missing background script: $background_path" >&2
  exit 1
fi
if [[ ! -f "$content_path" ]]; then
  echo "missing content script: $content_path" >&2
  exit 1
fi

python3 - "$manifest_path" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, 'r', encoding='utf-8') as fh:
    data = json.load(fh)

required_top = [
    'manifest_version',
    'name',
    'version',
    'permissions',
    'background',
    'content_scripts',
]

missing = [k for k in required_top if k not in data]
if missing:
    raise SystemExit(f"manifest missing keys: {', '.join(missing)}")

if data['manifest_version'] != 3:
    raise SystemExit('manifest_version must be 3')

perms = set(data.get('permissions', []))
for key in ['nativeMessaging', 'tabs']:
    if key not in perms:
        raise SystemExit(f"manifest permissions missing {key}")

content_scripts = data.get('content_scripts', [])
if not content_scripts:
    raise SystemExit('content_scripts must not be empty')

print('manifest validation ok')
PY

required_actions=(start_review resume_review show_findings refresh_review)
for action in "${required_actions[@]}"; do
  if ! rg -q "$action" "$background_path"; then
    echo "background script missing action mapping: $action" >&2
    exit 1
  fi
  if ! rg -q "$action" "$content_path"; then
    echo "content script missing action mapping: $action" >&2
    exit 1
  fi
done

echo "extension action mapping ok"
