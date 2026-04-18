#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
BR_PINNED="${SCRIPT_DIR}/br_pinned.sh"
TRUST_CHECK="${SCRIPT_DIR}/check_beads_trust.sh"
SAFE_REBUILD="${SCRIPT_DIR}/rebuild_beads_db_safe.sh"
DEFAULT_BEADS_DIR="${PROJECT_ROOT}/.beads"

usage() {
  cat <<'EOF'
Usage: br_safe.sh <br args...>

Guarded front door for common beads operations:
- verifies native SQLite workspace trust before DB-backed work
- routes mutating commands through br_pinned.sh
- falls back to --no-db reads for common queue inspection commands when trust is degraded

When trust is degraded:
- read-only queue inspection is allowed for: ready, list, show, blocked
- exact single-issue status or close mutations may recover through canonical JSONL + validated DB rebuild
- other mutating commands are refused; use rebuild_beads_db_safe.sh first
EOF
}

die() {
  echo "br_safe: $*" >&2
  exit 2
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

contains_arg() {
  local needle="$1"
  shift
  local arg
  for arg in "$@"; do
    if [[ "$arg" == "$needle" ]] || [[ "$arg" == "$needle="* ]]; then
      return 0
    fi
  done
  return 1
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
  "${SAFE_REBUILD}" --install
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
  "${BR_PINNED}" "${command_args[@]}" >"${stdout_file}" 2>"${stderr_file}"
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
  if [[ $# -eq 0 ]]; then
    usage
    exit 2
  fi

  if [[ "$1" == "-h" || "$1" == "--help" ]]; then
    usage
    exit 0
  fi

  local subcommand="$1"
  local -a args=("$@")
  local trust_output=""
  local trust_reason=""

  if trust_output="$("${TRUST_CHECK}" --skip-br-doctor 2>&1)"; then
    if is_read_inspection_command "$subcommand"; then
      if ! contains_arg "--no-daemon" "${args[@]}"; then
        args+=(--no-daemon)
      fi
      if ! contains_arg "--no-auto-import" "${args[@]}"; then
        args+=(--no-auto-import)
      fi
      if ! contains_arg "--no-auto-flush" "${args[@]}"; then
        args+=(--no-auto-flush)
      fi
      exec "${BR_PINNED}" "${args[@]}"
    fi
    if is_mutating_command "$subcommand"; then
      run_mutation_with_fallback "${args[@]}"
      exit $?
    fi
    exec "${BR_PINNED}" "${args[@]}"
  fi

  trust_reason=$(printf '%s\n' "$trust_output" | awk -F= '/^TRUST_REASON=/{print $2; exit}')
  if [[ -z "$trust_reason" ]]; then
    trust_reason="unknown trust failure"
  fi

  if supports_no_db_fallback "$subcommand"; then
    echo "br_safe: workspace trust degraded (${trust_reason}); using --no-db read fallback" >&2
    if ! contains_arg "--no-db" "${args[@]}"; then
      args+=(--no-db)
    fi
    if ! contains_arg "--no-daemon" "${args[@]}"; then
      args+=(--no-daemon)
    fi
    exec "${BR_PINNED}" "${args[@]}"
  fi

  if is_mutating_command "$subcommand"; then
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
