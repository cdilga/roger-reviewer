#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

echo "[smoke] extension entry UX test guard"
node --test \
  apps/extension/src/background.test.js \
  apps/extension/src/background.launch.test.js \
  apps/extension/src/content/main.test.js \
  apps/extension/src/popup/main.test.js

echo "[smoke] verify supported-browser launch suite ids are present"
for suite in \
  tests/suites/smoke_browser_launch_chrome.toml \
  tests/suites/smoke_browser_launch_brave.toml \
  tests/suites/smoke_browser_launch_edge.toml
do
  if [[ ! -f "${suite}" ]]; then
    echo "missing required smoke suite metadata: ${suite}" >&2
    exit 1
  fi
done

cat <<'EOF'
[smoke] caveat
This script validates Roger entry UX at content/popup test seams.
It does not execute a live browser DOM probe; release/manual lanes must still run
supported-browser live smoke when support wording or seam selectors change.
EOF
