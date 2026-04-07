#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck disable=SC1091
source "${SCRIPT_DIR}/agent_swarm_kit_env.sh"
CI_FAILURE_WATCH_HELPER="${SCRIPT_DIR}/ensure_ci_failure_watch.sh"

"${SWARM_KIT_ROOT}/bin/status.sh" "$@"

echo
if [[ -x "${CI_FAILURE_WATCH_HELPER}" ]]; then
  "${CI_FAILURE_WATCH_HELPER}" --status || true
fi
