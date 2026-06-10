#!/usr/bin/env bash
set -euo pipefail

# Keep Roger's browser session isolated from generic harness use.
export BU_NAME="${BU_NAME:-roger-edge}"

# Roger should prefer Edge when the harness auto-discovers a local browser.
export BU_BROWSER_PREFS="${BU_BROWSER_PREFS:-edge,chrome,brave}"

exec browser-harness "$@"
