#!/usr/bin/env bash
set -euo pipefail

REQUIRED_VERSION="${BR_REQUIRED_VERSION:-0.1.34}"
PINNED_DIR="${BR_PINNED_DIR:-${HOME}/.local/bin}"
PINNED_BIN="${BR_PINNED_BIN:-${PINNED_DIR}/br-${REQUIRED_VERSION}.pinned}"
DEFAULT_LINK="${BR_DEFAULT_LINK:-${PINNED_DIR}/br}"
LEGACY_BACKUP_BIN="${BR_LEGACY_BACKUP_BIN:-${PINNED_DIR}/br-${REQUIRED_VERSION}.queuebug.bak}"

usage() {
  cat <<USAGE
Usage: $(basename "$0") [--print-path|--verify|--version|--run <args...>|<br args...>]

Ensures the pinned beads_rust binary is available, verifies it matches br ${REQUIRED_VERSION},
and restores ${DEFAULT_LINK} to point at that pinned location.
USAGE
}

die() {
  echo "br_pinned: $*" >&2
  exit 1
}

ensure_pinned_binary() {
  mkdir -p "${PINNED_DIR}"

  if [[ ! -x "${PINNED_BIN}" ]]; then
    if [[ -x "${LEGACY_BACKUP_BIN}" ]]; then
      cp "${LEGACY_BACKUP_BIN}" "${PINNED_BIN}"
      chmod +x "${PINNED_BIN}"
    else
      die "missing pinned binary at ${PINNED_BIN} and no legacy backup at ${LEGACY_BACKUP_BIN}"
    fi
  fi

  local detected_version
  detected_version=$("${PINNED_BIN}" --version 2>/dev/null | awk '{print $2}')
  if [[ "${detected_version}" != "${REQUIRED_VERSION}" ]]; then
    die "expected br ${REQUIRED_VERSION} at ${PINNED_BIN}, found '${detected_version:-unknown}'"
  fi
}

restore_default_link() {
  mkdir -p "$(dirname "${DEFAULT_LINK}")"
  ln -sfn "${PINNED_BIN}" "${DEFAULT_LINK}"
}

main() {
  ensure_pinned_binary
  restore_default_link

  if [[ $# -eq 0 ]]; then
    exec "${PINNED_BIN}"
  fi

  case "$1" in
    -h|--help)
      usage
      ;;
    --print-path)
      echo "${PINNED_BIN}"
      ;;
    --verify)
      echo "${PINNED_BIN} (${REQUIRED_VERSION})"
      ;;
    --version)
      exec "${PINNED_BIN}" --version
      ;;
    --run)
      shift
      exec "${PINNED_BIN}" "$@"
      ;;
    *)
      exec "${PINNED_BIN}" "$@"
      ;;
  esac
}

main "$@"
