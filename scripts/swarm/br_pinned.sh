#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
REQUIRED_VERSION="${BR_REQUIRED_VERSION:-0.1.40}"
PINNED_DIR="${BR_PINNED_DIR:-${HOME}/.local/bin}"
PINNED_BIN="${BR_PINNED_BIN:-${PINNED_DIR}/br-${REQUIRED_VERSION}.pinned}"
DEFAULT_LINK="${BR_DEFAULT_LINK:-${PINNED_DIR}/br}"
LEGACY_BACKUP_BIN="${BR_LEGACY_BACKUP_BIN:-${PINNED_DIR}/br-${REQUIRED_VERSION}.queuebug.bak}"
DEFAULT_LOCK_TIMEOUT_MS="${RR_BR_LOCK_TIMEOUT_MS:-60000}"
WRITE_LOCK_TIMEOUT_SEC="${RR_BR_WRITE_LOCK_TIMEOUT_SEC:-120}"
WRITE_LOCK_POLL_SEC="${RR_BR_WRITE_LOCK_POLL_SEC:-0.2}"
SERIALIZE_WRITES="${RR_BR_SERIALIZE_WRITES:-1}"
AUTO_REPAIR_AFTER_WRITE="${RR_BR_AUTO_REPAIR_AFTER_WRITE:-1}"
TRUST_CHECK="${SCRIPT_DIR}/check_beads_trust.sh"
SAFE_REBUILD="${SCRIPT_DIR}/rebuild_beads_db_safe.sh"
EXACT_ID_FALLBACK="${SCRIPT_DIR}/exact_id_status_fallback.py"
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
  local current_target=""
  local temp_link=""

  mkdir -p "$(dirname "${DEFAULT_LINK}")"
  if [[ -L "${DEFAULT_LINK}" ]]; then
    current_target="$(readlink "${DEFAULT_LINK}" 2>/dev/null || true)"
    if [[ "${current_target}" == "${PINNED_BIN}" ]]; then
      return 0
    fi
  fi
  if [[ -e "${DEFAULT_LINK}" && ! -L "${DEFAULT_LINK}" ]]; then
    rm -f "${DEFAULT_LINK}"
  fi
  temp_link="${DEFAULT_LINK}.tmp.$$"
  rm -f "${temp_link}"
  ln -s "${PINNED_BIN}" "${temp_link}"
  mv -f "${temp_link}" "${DEFAULT_LINK}"
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

contains_arg() {
  local needle="$1"
  shift
  local arg
  for arg in "$@"; do
    if [[ "${arg}" == "${needle}" ]] || [[ "${arg}" == "${needle}="* ]]; then
      return 0
    fi
  done
  return 1
}

contains_help_arg() {
  contains_arg "--help" "$@" || contains_arg "-h" "$@"
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

should_auto_verify_workspace() {
  if (( AUTO_REPAIR_AFTER_WRITE != 1 )); then
    return 1
  fi
  if contains_help_arg "$@"; then
    return 1
  fi
  if ! [[ ${#@} -gt 0 ]] || ! is_mutating_command "${1:-}"; then
    return 1
  fi
  if contains_arg "--db" "$@" || contains_arg "--no-db" "$@"; then
    return 1
  fi
  if contains_arg "--no-auto-flush" "$@"; then
    return 1
  fi
  if [[ ! -x "${TRUST_CHECK}" || ! -x "${SAFE_REBUILD}" ]]; then
    return 1
  fi
  return 0
}

should_flush_canonical_jsonl_after_write() {
  if contains_help_arg "$@"; then
    return 1
  fi
  if ! [[ ${#@} -gt 0 ]] || ! is_mutating_command "${1:-}"; then
    return 1
  fi
  if [[ "${1:-}" == "sync" ]]; then
    return 1
  fi
  if contains_arg "--db" "$@" || contains_arg "--no-db" "$@" || contains_arg "--no-auto-flush" "$@"; then
    return 1
  fi
  return 0
}

FALLBACK_SUBCOMMAND=""
FALLBACK_ID=""
FALLBACK_STATUS=""
FALLBACK_REASON=""
FALLBACK_SESSION=""
FALLBACK_JSON=0

reset_exact_id_fallback_spec() {
  FALLBACK_SUBCOMMAND=""
  FALLBACK_ID=""
  FALLBACK_STATUS=""
  FALLBACK_REASON=""
  FALLBACK_SESSION=""
  FALLBACK_JSON=0
}

parse_exact_id_fallback_spec() {
  reset_exact_id_fallback_spec

  local subcommand="${1:-}"
  shift || true

  case "${subcommand}" in
    update|close|reopen)
      ;;
    *)
      return 1
      ;;
  esac

  local pending=""
  local -a ids=()
  local arg=""
  while [[ $# -gt 0 ]]; do
    arg="$1"
    shift

    if [[ -n "${pending}" ]]; then
      case "${pending}" in
        status)
          FALLBACK_STATUS="${arg}"
          ;;
        reason)
          FALLBACK_REASON="${arg}"
          ;;
        session)
          FALLBACK_SESSION="${arg}"
          ;;
        *)
          return 1
          ;;
      esac
      pending=""
      continue
    fi

    case "${arg}" in
      --json|--robot)
        FALLBACK_JSON=1
        ;;
      --no-daemon|--no-auto-flush|--no-auto-import|--allow-stale|--no-color|--quiet|-q|--verbose|-v|-vv|-vvv)
        ;;
      -s|--status)
        [[ "${subcommand}" == "update" ]] || return 1
        pending="status"
        ;;
      --status=*)
        [[ "${subcommand}" == "update" ]] || return 1
        FALLBACK_STATUS="${arg#*=}"
        ;;
      -r|--reason)
        [[ "${subcommand}" == "close" ]] || return 1
        pending="reason"
        ;;
      --reason=*)
        [[ "${subcommand}" == "close" ]] || return 1
        FALLBACK_REASON="${arg#*=}"
        ;;
      --session)
        pending="session"
        ;;
      --session=*)
        FALLBACK_SESSION="${arg#*=}"
        ;;
      --actor|--actor=*|--lock-timeout|--lock-timeout=*)
        ;;
      --db|--db=*|--no-db|--force|--suggest-next|--claim)
        return 1
        ;;
      --title|--title=*|--description|--description=*|--body|--body=*|--design|--design=*|--acceptance-criteria|--acceptance-criteria=*|--acceptance|--acceptance=*|--notes|--notes=*|--priority|--priority=*|--type|--type=*|--assignee|--assignee=*|--owner|--owner=*|--due|--due=*|--defer|--defer=*|--estimate|--estimate=*|--add-label|--add-label=*|--remove-label|--remove-label=*|--set-labels|--set-labels=*|--parent|--parent=*|--external-ref|--external-ref=*)
        return 1
        ;;
      --)
        while [[ $# -gt 0 ]]; do
          ids+=("$1")
          shift
        done
        ;;
      -*)
        return 1
        ;;
      *)
        ids+=("${arg}")
        ;;
    esac
  done

  [[ -z "${pending}" ]] || return 1
  [[ ${#ids[@]} -eq 1 ]] || return 1

  case "${subcommand}" in
    update)
      [[ -n "${FALLBACK_STATUS}" ]] || return 1
      ;;
    close|reopen)
      ;;
  esac

  FALLBACK_SUBCOMMAND="${subcommand}"
  FALLBACK_ID="${ids[0]}"
  return 0
}

exact_id_fallback_matches_failure() {
  local output="${1:-}"
  [[ -n "${FALLBACK_ID}" ]] || return 1
  [[ "${output}" == *"Ambiguous ID '${FALLBACK_ID}'"* ]] || [[ "${output}" == *"Issue not found: ${FALLBACK_ID}"* ]]
}

run_exact_id_fallback() {
  local beads_root="$1"
  [[ -n "${beads_root}" ]] || die "missing beads root for exact-id fallback"
  [[ -x "${EXACT_ID_FALLBACK}" ]] || die "missing exact-id fallback helper at ${EXACT_ID_FALLBACK}"

  local -a helper_args=(
    "${EXACT_ID_FALLBACK}"
    "--beads-dir" "${beads_root}"
    "--command" "${FALLBACK_SUBCOMMAND}"
    "--id" "${FALLBACK_ID}"
  )

  if [[ -n "${FALLBACK_STATUS}" ]]; then
    helper_args+=(--status "${FALLBACK_STATUS}")
  fi
  if [[ -n "${FALLBACK_REASON}" ]]; then
    helper_args+=(--reason "${FALLBACK_REASON}")
  fi
  if [[ -n "${FALLBACK_SESSION}" ]]; then
    helper_args+=(--session "${FALLBACK_SESSION}")
  fi
  if (( FALLBACK_JSON == 1 )); then
    helper_args+=(--json)
  fi

  echo "br_pinned: underlying br exact-id mutation failed for ${FALLBACK_ID}; applying canonical JSONL fallback" >&2
  "${helper_args[@]}"
  "${SAFE_REBUILD}" --beads-dir "${beads_root}" --install --br "${PINNED_BIN}" >&2
}

is_lock_only_trust_failure() {
  local trust_output="$1"
  [[ "${trust_output}" == *"database is busy"* ]] || [[ "${trust_output}" == *"database is locked"* ]]
}

post_mutation_verify_and_repair() {
  local beads_root="$1"
  local trust_output=""
  local trust_reason=""

  if trust_output="$("${TRUST_CHECK}" --beads-dir "${beads_root}" --skip-br-doctor 2>&1)"; then
    return 0
  fi

  trust_reason=$(printf '%s\n' "${trust_output}" | awk -F= '/^TRUST_REASON=/{print $2; exit}')
  if [[ -z "${trust_reason}" ]]; then
    trust_reason="unknown trust failure"
  fi

  if is_lock_only_trust_failure "${trust_output}"; then
    echo "br_pinned: post-mutation trust check deferred (${trust_reason})" >&2
    return 0
  fi

  echo "br_pinned: post-mutation trust check failed (${trust_reason}); rebuilding DB from canonical JSONL" >&2
  "${SAFE_REBUILD}" --beads-dir "${beads_root}" --install --br "${PINNED_BIN}" >&2
}

flush_canonical_jsonl_after_write() {
  local beads_root="$1"

  # Successful br mutations do not always leave issues.jsonl current by the
  # time control returns to this wrapper. Export the canonical JSONL snapshot
  # before we classify DB/JSONL parity as corruption.
  "${PINNED_BIN}" sync --flush-only --db "${beads_root}/beads.db" --no-daemon --no-auto-import --lock-timeout "${DEFAULT_LOCK_TIMEOUT_MS}" >/dev/null
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
  local root_path=""
  local status=0
  local verify_status=0
  local stdout_file=""
  local stderr_file=""
  local failure_output=""

  if [[ ${#cmd_args[@]} -gt 0 ]] && is_mutating_command "${cmd_args[0]}"; then
    if ! root_path="$(lock_root_for_args "${cmd_args[@]}")"; then
      root_path=""
    fi
  fi

  if (( SERIALIZE_WRITES == 1 )) && [[ ${#cmd_args[@]} -gt 0 ]] && is_mutating_command "${cmd_args[0]}"; then
    if [[ -n "${root_path}" ]]; then
      acquire_write_lock "${root_path}"
    fi
    if ! contains_lock_timeout_arg "${cmd_args[@]}"; then
      cmd_args+=(--lock-timeout "${DEFAULT_LOCK_TIMEOUT_MS}")
    fi
  fi

  if [[ -z "${root_path}" ]] && [[ ${#cmd_args[@]} -gt 0 ]] && is_mutating_command "${cmd_args[0]}"; then
    root_path="$(lock_root_for_args "${cmd_args[@]}" || true)"
  fi

  # Some br mutations can fail after partially touching the workspace. Keep
  # control in this wrapper so trust verification and safe rebuild still run.
  if parse_exact_id_fallback_spec "${cmd_args[@]}"; then
    stdout_file="$(mktemp "${TMPDIR:-/tmp}/rr-br-pinned-stdout.XXXXXX")"
    stderr_file="$(mktemp "${TMPDIR:-/tmp}/rr-br-pinned-stderr.XXXXXX")"
    set +e
    "${PINNED_BIN}" "${cmd_args[@]}" >"${stdout_file}" 2>"${stderr_file}"
    status=$?
    set -e

    if (( status != 0 )); then
      failure_output="$(cat "${stdout_file}" "${stderr_file}")"
      if [[ -n "${root_path}" ]] && exact_id_fallback_matches_failure "${failure_output}"; then
        rm -f "${stdout_file}" "${stderr_file}"
        run_exact_id_fallback "${root_path}"
        status=$?
        release_write_lock
        trap - EXIT INT TERM
        exit "${status}"
      fi
    fi

    cat "${stdout_file}"
    cat "${stderr_file}" >&2
    rm -f "${stdout_file}" "${stderr_file}"
  else
    set +e
    "${PINNED_BIN}" "${cmd_args[@]}"
    status=$?
    set -e
  fi

  if (( status == 0 )) && [[ -n "${root_path}" ]] && should_flush_canonical_jsonl_after_write "${cmd_args[@]}"; then
    set +e
    flush_canonical_jsonl_after_write "${root_path}"
    status=$?
    set -e
  fi

  if should_auto_verify_workspace "${cmd_args[@]}"; then
    if root_path="$(lock_root_for_args "${cmd_args[@]}")"; then
      set +e
      post_mutation_verify_and_repair "${root_path}"
      verify_status=$?
      set -e
      if (( status == 0 )) && (( verify_status != 0 )); then
        status=$verify_status
      fi
    fi
  fi
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
