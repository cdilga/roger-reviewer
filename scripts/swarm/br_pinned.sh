#!/usr/bin/env bash
set -euo pipefail

REQUIRED_VERSION="${BR_REQUIRED_VERSION:-0.1.40}"
PINNED_DIR="${BR_PINNED_DIR:-${HOME}/.local/bin}"
PINNED_BIN="${BR_PINNED_BIN:-${PINNED_DIR}/br-${REQUIRED_VERSION}.pinned}"
DEFAULT_LINK="${BR_DEFAULT_LINK:-${PINNED_DIR}/br}"
LEGACY_BACKUP_BIN="${BR_LEGACY_BACKUP_BIN:-${PINNED_DIR}/br-${REQUIRED_VERSION}.queuebug.bak}"
DEFAULT_LOCK_TIMEOUT_MS="${RR_BR_LOCK_TIMEOUT_MS:-60000}"
WRITE_LOCK_TIMEOUT_SEC="${RR_BR_WRITE_LOCK_TIMEOUT_SEC:-120}"
WRITE_LOCK_POLL_SEC="${RR_BR_WRITE_LOCK_POLL_SEC:-0.2}"
SERIALIZE_WRITES="${RR_BR_SERIALIZE_WRITES:-1}"
LOCK_HELD=0
LOCK_DIR=""

usage() {
  cat <<USAGE
Usage: $(basename "$0") [--print-path|--verify|--version|--run <args...>|<br args...>]

Ensures the pinned beads_rust binary is available, verifies it matches br ${REQUIRED_VERSION},
restores ${DEFAULT_LINK} to point at that pinned location, and serializes
mutating br commands behind a repo-local advisory lock by default.
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

contains_lock_timeout_arg() {
  local arg
  for arg in "$@"; do
    if [[ "${arg}" == "--lock-timeout" ]] || [[ "${arg}" == --lock-timeout=* ]]; then
      return 0
    fi
  done
  return 1
}

db_root_from_args() {
  local prev_was_db=0
  local arg db_path
  for arg in "$@"; do
    if (( prev_was_db == 1 )); then
      db_path="${arg}"
      printf '%s\n' "$(cd "$(dirname "${db_path}")" && pwd)"
      return 0
    fi
    case "${arg}" in
      --db)
        prev_was_db=1
        ;;
      --db=*)
        db_path="${arg#--db=}"
        printf '%s\n' "$(cd "$(dirname "${db_path}")" && pwd)"
        return 0
        ;;
      *)
        prev_was_db=0
        ;;
    esac
  done
  return 1
}

find_beads_root() {
  local dir="${PWD}"
  while true; do
    if [[ -d "${dir}/.beads" ]]; then
      printf '%s\n' "${dir}/.beads"
      return 0
    fi
    if [[ "${dir}" == "/" ]]; then
      return 1
    fi
    dir="$(dirname "${dir}")"
  done
}

lock_root_for_args() {
  local db_root
  if db_root="$(db_root_from_args "$@")"; then
    printf '%s\n' "${db_root}"
    return 0
  fi
  find_beads_root
}

is_mutating_command() {
  local subcommand="${1:-}"
  case "${subcommand}" in
    init|create|update|close|reopen|delete|archive|sync|import|defer|undefer|dep|duplicate|merge|edit)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

release_write_lock() {
  if (( LOCK_HELD == 1 )) && [[ -n "${LOCK_DIR}" ]]; then
    rmdir "${LOCK_DIR}" 2>/dev/null || true
  fi
}

acquire_write_lock() {
  local root_path="$1"
  local start_ts elapsed
  LOCK_DIR="${root_path}/.rr_br_write_lock"
  start_ts="${SECONDS}"

  while ! mkdir "${LOCK_DIR}" 2>/dev/null; do
    elapsed=$((SECONDS - start_ts))
    if (( elapsed >= WRITE_LOCK_TIMEOUT_SEC )); then
      die "timed out waiting ${WRITE_LOCK_TIMEOUT_SEC}s for br write lock at ${LOCK_DIR}"
    fi
    sleep "${WRITE_LOCK_POLL_SEC}"
  done

  LOCK_HELD=1
  trap release_write_lock EXIT INT TERM
}

run_br_command() {
  local -a cmd_args=("$@")
  local root_path status

  if (( SERIALIZE_WRITES == 1 )) && [[ ${#cmd_args[@]} -gt 0 ]] && is_mutating_command "${cmd_args[0]}"; then
    if root_path="$(lock_root_for_args "${cmd_args[@]}")"; then
      acquire_write_lock "${root_path}"
    fi
    if ! contains_lock_timeout_arg "${cmd_args[@]}"; then
      cmd_args+=(--lock-timeout "${DEFAULT_LOCK_TIMEOUT_MS}")
    fi
  fi

  "${PINNED_BIN}" "${cmd_args[@]}"
  status=$?
  release_write_lock
  trap - EXIT INT TERM
  exit "${status}"
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
      run_br_command "$@"
      ;;
    *)
      run_br_command "$@"
      ;;
  esac
}

main "$@"
