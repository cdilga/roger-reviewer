#!/usr/bin/env bash
# Live panel-interaction E2E: launch a real browser preloaded with the unpacked
# Roger extension, then drive REAL clicks via CDP and assert OBSERVABLE outcomes
# (mark renders, each launch button settles with VISIBLE feedback, launch still
# settles after the MV3 worker idles, the "i" control toggles).
#
# This is the harness layer that catches what contract/jsdom tests cannot: the
# status relay (rr-s8wx) and launch buttons (one-shot sendNativeMessage twin)
# both shipped past green stubbed suites and only broke against Edge's real
# module service worker. It is a real-browser + native-host test (like the
# Docker install/update E2E), so it runs manually/locally, not in hermetic CI.
#
# Usage:
#   scripts/extension/test_panel_interaction_e2e.sh [--browser edge|chrome|brave]
#       [--pr-url <url>] [--skip-idle] [--no-reset]
#
# Prereqs: the target browser installed; on macOS arm the dedicated profile is
# isolated from your real browser. Edge/Brave honor --load-extension; branded
# Chrome 137+ needs a one-time manual "Load unpacked".
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
browser="edge"
pr_url="https://github.com/cdilga/roger-reviewer/pull/2"
skip_idle=""
reset="--reset-profile"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --browser) browser="${2:-}"; shift 2 ;;
    --pr-url) pr_url="${2:-}"; shift 2 ;;
    --skip-idle) skip_idle="1"; shift ;;
    --no-reset) reset=""; shift ;;
    -h|--help) grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "error: unknown argument: $1" >&2; exit 2 ;;
  esac
done

testing_dir="${ROOT_DIR}/apps/extension/testing"

# Bootstrap puppeteer-core (CDP client; no browser download) if absent.
if [[ ! -d "${testing_dir}/node_modules/puppeteer-core" ]]; then
  echo "==> installing puppeteer-core (CDP client) into apps/extension/testing"
  (cd "${testing_dir}" && npm install --no-save puppeteer-core@22 >/dev/null 2>&1) || {
    echo "error: failed to install puppeteer-core; install it manually in ${testing_dir}" >&2
    exit 3
  }
fi

# Pack the current source so the loaded extension reflects the working tree, then
# launch the browser preloaded with it at the PR url (with CDP enabled — the
# launcher already passes --remote-debugging-port).
echo "==> packing extension from current source"
"${ROOT_DIR}/target/debug/rr" bridge pack-extension --robot >/dev/null 2>&1 \
  || "${ROOT_DIR}/target/release/rr" bridge pack-extension --robot >/dev/null 2>&1 \
  || { echo "error: could not pack extension (build rr first)" >&2; exit 3; }

echo "==> launching ${browser} preloaded with the unpacked extension at ${pr_url}"
# shellcheck disable=SC2086
bash "${ROOT_DIR}/scripts/extension/prepare_browser_test_env.sh" \
  --browser "${browser}" ${reset} --start-url "${pr_url}" --require-doctor-pass >/dev/null 2>&1

profile_root="${HOME}/.roger/bridge/browser-profiles/${browser}"
port_file="${profile_root}/DevToolsActivePort"

# Wait for the CDP endpoint to come up.
for _ in $(seq 1 20); do
  [[ -f "${port_file}" ]] && break
  sleep 1
done
[[ -f "${port_file}" ]] || { echo "error: ${browser} did not expose a CDP port (${port_file} missing)" >&2; exit 3; }
sleep 5  # let the page load + content script inject + status relay settle

echo "==> driving real clicks via CDP and asserting observable outcomes"
RR_CDP_PORT_FILE="${port_file}" RR_PR_URL="${pr_url}" RR_SKIP_IDLE="${skip_idle}" \
  node "${testing_dir}/panel_interaction_e2e.cjs"
status=$?

echo
if [[ $status -eq 0 ]]; then
  echo "test_panel_interaction_e2e: PASS (${browser})"
else
  echo "test_panel_interaction_e2e: FAIL (${browser}) exit=${status}"
fi
exit $status
