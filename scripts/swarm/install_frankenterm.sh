#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
DEFAULT_SOURCE_DIR="${HOME}/Documents/dev/dicklesworthstone/frankenterm"

SOURCE_DIR="${DEFAULT_SOURCE_DIR}"
GIT_URL="https://github.com/Dicklesworthstone/frankenterm.git"
FORCE_INSTALL=1

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Install Frankenterm (\`ft\`) and run local workspace smoke checks.

Options:
  --source-dir PATH    Local Frankenterm checkout (default: ${DEFAULT_SOURCE_DIR})
  --git-url URL        Git fallback when local checkout is unavailable
  --no-force           Skip \`--force\` for cargo install
  -h, --help           Show this help
EOF
}

require_command() {
  local cmd="$1"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Missing required command: $cmd" >&2
    exit 1
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-dir)
      SOURCE_DIR="$2"
      shift 2
      ;;
    --git-url)
      GIT_URL="$2"
      shift 2
      ;;
    --no-force)
      FORCE_INSTALL=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

require_command cargo

install_args=(--locked)
if (( FORCE_INSTALL == 1 )); then
  install_args+=(--force)
fi

if (( FORCE_INSTALL == 0 )) && command -v ft >/dev/null 2>&1; then
  echo "Skipping install (--no-force): $(ft --version)"
elif [[ -d "${SOURCE_DIR}/crates/frankenterm" ]]; then
  echo "Installing ft from local checkout: ${SOURCE_DIR}"
  (
    cd "${SOURCE_DIR}"
    cargo install --path "crates/frankenterm" "${install_args[@]}"
  )
else
  echo "Local Frankenterm checkout not found; installing from git with nightly: ${GIT_URL}"
  cargo +nightly install --git "${GIT_URL}" ft "${install_args[@]}"
fi

require_command ft
echo "Installed: $(ft --version)"

echo "Running ft doctor/status smoke checks in ${PROJECT_ROOT}"
(
  cd "${PROJECT_ROOT}"
  ft doctor --json >/tmp/rr_ft_doctor_install.json
  ft status --format json >/tmp/rr_ft_status_install.json
)

echo "Frankenterm install complete."
echo "Validation artifacts:"
echo "  /tmp/rr_ft_doctor_install.json"
echo "  /tmp/rr_ft_status_install.json"
