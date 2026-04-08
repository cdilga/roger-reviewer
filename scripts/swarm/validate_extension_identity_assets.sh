#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

manifest_path="apps/extension/manifest.template.json"
identity_note="docs/extension-visual-identity.md"

if [[ ! -f "${identity_note}" ]]; then
  echo "missing identity note: ${identity_note}" >&2
  exit 1
fi

if ! rg -q "Chosen: Signal Ribbon mark" "${identity_note}"; then
  echo "identity note does not record chosen direction" >&2
  exit 1
fi

node <<'NODE'
const fs = require('node:fs');
const path = require('node:path');

const manifestPath = path.join(process.cwd(), 'apps/extension/manifest.template.json');
const raw = fs.readFileSync(manifestPath, 'utf8');
const manifest = JSON.parse(raw);

const iconPaths = new Set();

if (manifest.icons && typeof manifest.icons === 'object') {
  for (const value of Object.values(manifest.icons)) {
    if (typeof value === 'string' && value.length > 0) {
      iconPaths.add(value);
    }
  }
}

if (manifest.action?.default_icon && typeof manifest.action.default_icon === 'object') {
  for (const value of Object.values(manifest.action.default_icon)) {
    if (typeof value === 'string' && value.length > 0) {
      iconPaths.add(value);
    }
  }
}

if (iconPaths.size === 0) {
  console.error('manifest has no icon entries');
  process.exit(1);
}

const missing = [];
for (const iconPath of iconPaths) {
  const absolute = path.join(process.cwd(), 'apps/extension', iconPath);
  if (!fs.existsSync(absolute)) {
    missing.push(iconPath);
  }
}

if (missing.length > 0) {
  console.error(`missing manifest icon files: ${missing.join(', ')}`);
  process.exit(1);
}

console.log(`validated ${iconPaths.size} manifest icon paths`);
NODE

echo "PASS: extension identity assets are wired and present"
