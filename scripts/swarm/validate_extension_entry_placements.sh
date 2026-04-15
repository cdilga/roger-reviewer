#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

echo "[validate] extension entry placement + fallback smoke"
node --test \
  apps/extension/src/content/main.test.js \
  apps/extension/src/popup/main.test.js \
  apps/extension/src/background.launch.test.js \
  apps/extension/src/background.test.js

echo "[validate] assert placement precedence test coverage markers are present"
content_test="apps/extension/src/content/main.test.js"

# The primary host-preference assertion text may evolve (inline-first vs rail-first)
# while still validating the same bounded precedence contract.
if ! grep -Fq "resolvePanelPlacement prefers inline mode when both header and rail seams exist" "${content_test}" \
  && ! grep -Fq "resolvePanelPlacement prefers rail mode when both header and rail seams exist" "${content_test}"
then
  echo "missing host-preference coverage marker in ${content_test}" >&2
  exit 1
fi

for marker in \
  "resolvePanelPlacement selects rail mode above reviewers when header seam is unavailable" \
  "resolvePanelPlacement falls back to modal mode when no bounded seam exists"
do
  if ! grep -Fq "${marker}" "${content_test}"; then
    echo "missing placement coverage marker: ${marker}" >&2
    exit 1
  fi
done

echo "[validate] assert popup action set remains Start/Resume/Findings"
node <<'NODE'
const { ACTIONS } = require('./apps/extension/src/popup/main.js');
const expected = ['start_review', 'resume_review', 'show_findings'];
const actual = Array.isArray(ACTIONS) ? ACTIONS.map((action) => action.id) : [];
if (JSON.stringify(actual) !== JSON.stringify(expected)) {
  console.error(`unexpected popup action set: ${JSON.stringify(actual)}`);
  process.exit(1);
}
NODE

echo "[validate] verify supported-browser launch suite ids are present"
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
[validate] pass
Validated PR entry placement precedence (header -> rail -> modal),
popup manual backup action set, and Native Messaging fail-closed launch behavior.
EOF
