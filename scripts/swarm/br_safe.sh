#!/usr/bin/env bash
set -euo pipefail

SELF_PATH="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "${BASH_SOURCE[0]}")"
SCRIPT_DIR="$(cd "$(dirname "${SELF_PATH}")" && pwd)"
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
TRUST_CHECK="${SCRIPT_DIR}/check_beads_trust.sh"
SAFE_REBUILD="${SCRIPT_DIR}/rebuild_beads_db_safe.sh"
EXACT_ID_FALLBACK="${SCRIPT_DIR}/exact_id_status_fallback.py"
DEFAULT_BEADS_DIR="${PROJECT_ROOT}/.beads"
CANONICAL_BR_BIN="${HOME}/.local/bin/br-main.current"

resolve_br_bin() {
  if [[ -n "${RR_BR_BIN:-}" ]]; then
    printf '%s\n' "${RR_BR_BIN}"
    return 0
  fi
  if [[ -x "${CANONICAL_BR_BIN}" ]]; then
    printf '%s\n' "${CANONICAL_BR_BIN}"
    return 0
  fi
  command -v br 2>/dev/null || true
}

BR_BIN="$(resolve_br_bin)"
DEFAULT_LOCK_TIMEOUT_MS="${RR_BR_LOCK_TIMEOUT_MS:-60000}"
WRITE_LOCK_TIMEOUT_SEC="${RR_BR_WRITE_LOCK_TIMEOUT_SEC:-120}"
WRITE_LOCK_POLL_SEC="${RR_BR_WRITE_LOCK_POLL_SEC:-0.2}"
SERIALIZE_WRITES="${RR_BR_SERIALIZE_WRITES:-1}"
AUTO_REPAIR_AFTER_WRITE="${RR_BR_AUTO_REPAIR_AFTER_WRITE:-1}"

LOCK_HELD=0
LOCK_DIR=""

usage() {
  cat <<'EOF'
Usage: br_safe.sh [--print-path|--verify|--version|<br args...>]

Single guarded front door for Roger bead operations:
- verifies native SQLite workspace trust before DB-backed work
- serializes mutating commands behind a repo-local advisory lock
- flushes canonical JSONL after successful writes
- validates and auto-repairs from canonical JSONL after corrupting writes
- falls back to --no-db reads for common queue inspection commands when trust is degraded

When trust is degraded:
- read-only queue inspection is allowed for: ready, list, show, blocked
- exact single-issue status mutations may recover through canonical JSONL + validated DB rebuild
- other mutating commands are refused; use rebuild_beads_db_safe.sh first
EOF
}

die() {
  echo "br_safe: $*" >&2
  exit 2
}

ensure_br_bin() {
  [[ -n "${BR_BIN}" ]] || die "unable to find 'br' on PATH; install it first"
  [[ -x "${BR_BIN}" ]] || die "br is not executable: ${BR_BIN}"
  local resolved_bin=""
  resolved_bin="$(python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "${BR_BIN}")"
  [[ "${resolved_bin}" != "${SELF_PATH}" ]] || die "resolved br binary points back to br_safe.sh; set RR_BR_BIN or repair ~/.local/bin/br"
  "${BR_BIN}" --version >/dev/null 2>&1 || die "unable to execute br at ${BR_BIN}"
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

contains_lock_timeout_arg() {
  contains_arg "--lock-timeout" "$@"
}

extract_issue_id_arg() {
  shift
  local arg
  for arg in "$@"; do
    if [[ "${arg}" == --* ]]; then
      continue
    fi
    printf '%s\n' "${arg}"
    return 0
  done
  return 1
}

extract_option_value() {
  local option="$1"
  shift
  local prev_was_option=0
  local arg
  for arg in "$@"; do
    if (( prev_was_option == 1 )); then
      printf '%s\n' "${arg}"
      return 0
    fi
    if [[ "${arg}" == "${option}" ]]; then
      prev_was_option=1
      continue
    fi
    if [[ "${arg}" == "${option}="* ]]; then
      printf '%s\n' "${arg#${option}=}"
      return 0
    fi
  done
  return 1
}

find_beads_dir() {
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

lock_root_for_args() {
  local db_root
  if db_root="$(db_root_from_args "$@")"; then
    printf '%s\n' "${db_root}"
    return 0
  fi
  find_beads_dir
}

is_mutating_command() {
  local subcommand="${1:-}"
  case "${subcommand}" in
    init|create|update|close|reopen|delete|archive|sync|import|defer|undefer|dep|duplicate|merge|edit|label|comment|comments)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

supports_no_db_fallback() {
  local subcommand="${1:-}"
  case "${subcommand}" in
    ready|list|show|blocked)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

is_read_inspection_command() {
  local subcommand="${1:-}"
  case "${subcommand}" in
    ready|list|show|blocked|graph|audit|orphans|info|stats|where)
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
  if contains_arg "--db" "$@" || contains_arg "--no-db" "$@" || contains_arg "--no-auto-flush" "$@"; then
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

canonical_issue_exists() {
  local issue_id="$1"
  local beads_dir=""
  local jsonl_path=""

  beads_dir="$(find_beads_dir || true)"
  if [[ -z "${beads_dir}" ]]; then
    beads_dir="${DEFAULT_BEADS_DIR}"
  fi
  jsonl_path="${beads_dir}/issues.jsonl"

  [[ -f "${jsonl_path}" ]] || return 1
  command -v jq >/dev/null 2>&1 || return 1
  jq -e --arg issue_id "${issue_id}" 'select(.id == $issue_id)' "${jsonl_path}" >/dev/null
}

canonical_issue_has_child_collision() {
  local issue_id="$1"
  local beads_dir=""
  local jsonl_path=""

  beads_dir="$(find_beads_dir || true)"
  if [[ -z "${beads_dir}" ]]; then
    beads_dir="${DEFAULT_BEADS_DIR}"
  fi
  jsonl_path="${beads_dir}/issues.jsonl"

  [[ -f "${jsonl_path}" ]] || return 1
  command -v jq >/dev/null 2>&1 || return 1
  jq -e --arg issue_id "${issue_id}" 'select(.id != $issue_id and (.id | startswith($issue_id + ".")))' "${jsonl_path}" >/dev/null
}

apply_issue_status_fallback() {
  local issue_id="$1"
  local requested_status="$2"
  local close_reason="${3:-}"
  local beads_dir=""
  local jsonl_path=""
  local tmp_jsonl=""
  local updated_at=""

  beads_dir="$(find_beads_dir || true)"
  if [[ -z "${beads_dir}" ]]; then
    beads_dir="${DEFAULT_BEADS_DIR}"
  fi
  jsonl_path="${beads_dir}/issues.jsonl"

  [[ -f "${jsonl_path}" ]] || die "missing canonical JSONL: ${jsonl_path}"
  command -v jq >/dev/null 2>&1 || die "jq is required for canonical issue fallback"

  updated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  tmp_jsonl="$(mktemp "${beads_dir}/issues.jsonl.fallback.XXXXXX")"

  if [[ "${requested_status}" == "closed" ]]; then
    jq -c \
      --arg issue_id "${issue_id}" \
      --arg requested_status "${requested_status}" \
      --arg updated_at "${updated_at}" \
      --arg close_reason "${close_reason}" \
      '
      if .id == $issue_id then
        .status = $requested_status
        | .updated_at = $updated_at
        | .closed_at = $updated_at
        | .close_reason = $close_reason
      else
        .
      end
      ' "${jsonl_path}" > "${tmp_jsonl}"
  else
    jq -c \
      --arg issue_id "${issue_id}" \
      --arg requested_status "${requested_status}" \
      --arg updated_at "${updated_at}" \
      '
      if .id == $issue_id then
        .status = $requested_status
        | .updated_at = $updated_at
        | del(.closed_at, .close_reason)
      else
        .
      end
      ' "${jsonl_path}" > "${tmp_jsonl}"
  fi

  mv "${tmp_jsonl}" "${jsonl_path}"
  "${SAFE_REBUILD}" --install --br "${BR_BIN}"
}

extract_requested_status() {
  local subcommand="$1"
  shift

  case "${subcommand}" in
    close)
      printf '%s\n' "closed"
      return 0
      ;;
    reopen)
      printf '%s\n' "open"
      return 0
      ;;
    update)
      extract_option_value "--status" "$@" || return 1
      ;;
    *)
      return 1
      ;;
  esac
}

attempt_canonical_issue_fallback() {
  local subcommand="$1"
  shift
  local -a command_args=("${subcommand}" "$@")
  local issue_id=""
  local requested_status=""
  local close_reason=""

  issue_id="$(extract_issue_id_arg "${command_args[@]}" || true)"
  requested_status="$(extract_requested_status "${subcommand}" "${command_args[@]}" || true)"
  close_reason="$(extract_option_value "--reason" "${command_args[@]}" || true)"

  [[ -n "${issue_id}" ]] || return 1
  [[ -n "${requested_status}" ]] || return 1
  canonical_issue_exists "${issue_id}" || return 1

  if [[ "${requested_status}" == "closed" && -z "${close_reason}" ]]; then
    return 1
  fi

  apply_issue_status_fallback "${issue_id}" "${requested_status}" "${close_reason}"
  echo "br_safe: recovered ${subcommand} for canonical issue ${issue_id} via JSONL fallback + validated DB rebuild" >&2
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
        status) FALLBACK_STATUS="${arg}" ;;
        reason) FALLBACK_REASON="${arg}" ;;
        session) FALLBACK_SESSION="${arg}" ;;
        *) return 1 ;;
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

  echo "br_safe: underlying br exact-id mutation failed for ${FALLBACK_ID}; applying canonical JSONL fallback" >&2
  "${helper_args[@]}"
  "${SAFE_REBUILD}" --beads-dir "${beads_root}" --install --br "${BR_BIN}" >&2
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
    echo "br_safe: post-mutation trust check deferred (${trust_reason})" >&2
    return 0
  fi

  echo "br_safe: post-mutation trust check failed (${trust_reason}); rebuilding DB from canonical JSONL" >&2
  "${SAFE_REBUILD}" --beads-dir "${beads_root}" --install --br "${BR_BIN}" >&2
}

flush_canonical_jsonl_after_write() {
  local beads_root="$1"
  "${BR_BIN}" sync --flush-only --db "${beads_root}/beads.db" --no-daemon --no-auto-import --lock-timeout "${DEFAULT_LOCK_TIMEOUT_MS}" >/dev/null
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

run_underlying_br_command() {
  local -a cmd_args=("$@")
  local root_path=""
  local status=0
  local verify_status=0
  local stdout_file=""
  local stderr_file=""
  local failure_output=""

  if [[ ${#cmd_args[@]} -gt 0 ]] && is_mutating_command "${cmd_args[0]}"; then
    root_path="$(lock_root_for_args "${cmd_args[@]}" || true)"
  fi

  if (( SERIALIZE_WRITES == 1 )) && [[ -n "${root_path}" ]] && [[ ${#cmd_args[@]} -gt 0 ]] && is_mutating_command "${cmd_args[0]}"; then
    acquire_write_lock "${root_path}"
    if ! contains_lock_timeout_arg "${cmd_args[@]}"; then
      cmd_args+=(--lock-timeout "${DEFAULT_LOCK_TIMEOUT_MS}")
    fi
  fi

  if parse_exact_id_fallback_spec "${cmd_args[@]}"; then
    stdout_file="$(mktemp "${TMPDIR:-/tmp}/rr-br-safe-stdout.XXXXXX")"
    stderr_file="$(mktemp "${TMPDIR:-/tmp}/rr-br-safe-stderr.XXXXXX")"
    set +e
    "${BR_BIN}" "${cmd_args[@]}" >"${stdout_file}" 2>"${stderr_file}"
    status=$?
    set -e

    if (( status != 0 )); then
      failure_output="$(cat "${stdout_file}" "${stderr_file}")"
      if [[ -n "${root_path}" ]] && [[ -x "${EXACT_ID_FALLBACK}" ]] && exact_id_fallback_matches_failure "${failure_output}"; then
        rm -f "${stdout_file}" "${stderr_file}"
        run_exact_id_fallback "${root_path}"
        status=$?
        release_write_lock
        trap - EXIT INT TERM
        return "${status}"
      fi
    fi

    cat "${stdout_file}"
    cat "${stderr_file}" >&2
    rm -f "${stdout_file}" "${stderr_file}"
  else
    set +e
    "${BR_BIN}" "${cmd_args[@]}"
    status=$?
    set -e
  fi

  if (( status == 0 )) && [[ -n "${root_path}" ]] && should_flush_canonical_jsonl_after_write "${cmd_args[@]}"; then
    set +e
    flush_canonical_jsonl_after_write "${root_path}"
    status=$?
    set -e
  fi

  if should_auto_verify_workspace "${cmd_args[@]}" && [[ -n "${root_path}" ]]; then
    set +e
    post_mutation_verify_and_repair "${root_path}"
    verify_status=$?
    set -e
    if (( status == 0 )) && (( verify_status != 0 )); then
      status=$verify_status
    fi
  fi

  release_write_lock
  trap - EXIT INT TERM
  return "${status}"
}

run_mutation_with_fallback() {
  local subcommand="$1"
  shift
  local -a command_args=("${subcommand}" "$@")
  local issue_id=""
  local requested_status=""
  local close_reason=""
  local stdout_file=""
  local stderr_file=""
  local stdout_text=""
  local stderr_text=""
  local status=0

  issue_id="$(extract_issue_id_arg "${command_args[@]}" || true)"
  if [[ -n "${issue_id}" ]] && canonical_issue_exists "${issue_id}" && canonical_issue_has_child_collision "${issue_id}"; then
    attempt_canonical_issue_fallback "${subcommand}" "$@"
    return $?
  fi

  stdout_file="$(mktemp)"
  stderr_file="$(mktemp)"

  set +e
  run_underlying_br_command "${command_args[@]}" >"${stdout_file}" 2>"${stderr_file}"
  status=$?
  set -e

  stdout_text="$(cat "${stdout_file}")"
  stderr_text="$(cat "${stderr_file}")"
  rm -f "${stdout_file}" "${stderr_file}"

  if [[ "${status}" -eq 0 ]]; then
    [[ -n "${stdout_text}" ]] && printf '%s\n' "${stdout_text}"
    [[ -n "${stderr_text}" ]] && printf '%s\n' "${stderr_text}" >&2
    return 0
  fi

  if contains_arg "--db" "${command_args[@]}" || contains_arg "--no-db" "${command_args[@]}"; then
    [[ -n "${stdout_text}" ]] && printf '%s\n' "${stdout_text}"
    [[ -n "${stderr_text}" ]] && printf '%s\n' "${stderr_text}" >&2
    return "${status}"
  fi

  if [[ -n "${issue_id}" ]]; then
    case "${subcommand}" in
      close)
        requested_status="closed"
        close_reason="$(extract_option_value "--reason" "${command_args[@]}" || true)"
        ;;
      update)
        requested_status="$(extract_option_value "--status" "${command_args[@]}" || true)"
        close_reason="$(extract_option_value "--reason" "${command_args[@]}" || true)"
        ;;
      reopen)
        requested_status="open"
        ;;
    esac
  fi

  if [[ -n "${issue_id}" && -n "${requested_status}" ]] \
    && canonical_issue_exists "${issue_id}" \
    && { [[ "${stderr_text}" == *"Issue not found: ${issue_id}"* ]] || [[ "${stderr_text}" == *"Ambiguous ID '${issue_id}'"* ]]; }; then
    attempt_canonical_issue_fallback "${subcommand}" "$@"
    return $?
  fi

  [[ -n "${stdout_text}" ]] && printf '%s\n' "${stdout_text}"
  [[ -n "${stderr_text}" ]] && printf '%s\n' "${stderr_text}" >&2
  return "${status}"
}

main() {
  ensure_br_bin

  if [[ $# -eq 0 ]]; then
    usage
    exit 2
  fi

  case "${1:-}" in
    -h|--help)
      usage
      exit 0
      ;;
    --print-path)
      printf '%s\n' "${BR_BIN}"
      exit 0
      ;;
    --verify)
      printf '%s (%s)\n' "${BR_BIN}" "$("${BR_BIN}" --version 2>/dev/null | awk '{print $2}')"
      exit 0
      ;;
    --version)
      exec "${BR_BIN}" --version
      ;;
  esac

  local subcommand="$1"
  local -a args=("$@")
  local trust_output=""
  local trust_reason=""

  if trust_output="$("${TRUST_CHECK}" --skip-br-doctor 2>&1)"; then
    if is_read_inspection_command "${subcommand}"; then
      if ! contains_arg "--no-daemon" "${args[@]}"; then
        args+=(--no-daemon)
      fi
      if ! contains_arg "--no-auto-import" "${args[@]}"; then
        args+=(--no-auto-import)
      fi
      if ! contains_arg "--no-auto-flush" "${args[@]}"; then
        args+=(--no-auto-flush)
      fi
      exec "${BR_BIN}" "${args[@]}"
    fi
    if is_mutating_command "${subcommand}"; then
      run_mutation_with_fallback "${args[@]}"
      exit $?
    fi
    exec "${BR_BIN}" "${args[@]}"
  fi

  trust_reason=$(printf '%s\n' "${trust_output}" | awk -F= '/^TRUST_REASON=/{print $2; exit}')
  if [[ -z "${trust_reason}" ]]; then
    trust_reason="unknown trust failure"
  fi

  if supports_no_db_fallback "${subcommand}"; then
    echo "br_safe: workspace trust degraded (${trust_reason}); using --no-db read fallback" >&2
    if ! contains_arg "--no-db" "${args[@]}"; then
      args+=(--no-db)
    fi
    if ! contains_arg "--no-daemon" "${args[@]}"; then
      args+=(--no-daemon)
    fi
    exec "${BR_BIN}" "${args[@]}"
  fi

  if is_mutating_command "${subcommand}"; then
    if attempt_canonical_issue_fallback "${args[@]}"; then
      exit 0
    fi
    cat >&2 <<EOF
br_safe: refusing DB-backed mutation because workspace trust is degraded (${trust_reason})
br_safe: run ${SAFE_REBUILD} --install first, then retry
br_safe: use Agent Mail + file reservations for coordination while the DB is being repaired
EOF
    exit 2
  fi

  cat >&2 <<EOF
br_safe: workspace trust is degraded (${trust_reason})
br_safe: this command has no safe --no-db fallback here
br_safe: use ready/list/show/blocked for read-only inspection or repair with:
  ${SAFE_REBUILD} --install
EOF
  exit 2
}

main "$@"
