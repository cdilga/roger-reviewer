#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_KIT_ROOT="$(cd "${PROJECT_ROOT}/.." && pwd)/agent-swarm-kit"

export SWARM_PROJECT_ROOT="${PROJECT_ROOT}"
export SWARM_PROJECT_CONFIG="${PROJECT_ROOT}/.swarm-kit/project.env"
export SWARM_KIT_ROOT="${SWARM_KIT_ROOT:-${DEFAULT_KIT_ROOT}}"

if [[ ! -d "${SWARM_KIT_ROOT}" ]]; then
  echo "agent-swarm-kit repo not found at ${SWARM_KIT_ROOT}" >&2
  exit 1
fi

if [[ ! -f "${SWARM_PROJECT_CONFIG}" ]]; then
  echo "Missing swarm project config: ${SWARM_PROJECT_CONFIG}" >&2
  exit 1
fi
