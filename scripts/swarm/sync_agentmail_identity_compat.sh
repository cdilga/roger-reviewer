#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)

PROJECT_PATH="${PROJECT_ROOT}"
declare -a REQUESTED_PANES=()

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Mirror NTM-written pane identity files into the legacy Agent Mail locations
that the current installed \`am\` binary still advertises.

Options:
  --project PATH      Absolute project path (default: ${PROJECT_PATH})
  --pane ID           Restrict sync to one pane id (repeatable, e.g. --pane %0)
  -h, --help          Show this help
EOF
}

require_absolute_path() {
  case "$1" in
    /*) ;;
    *)
      echo "project path must be absolute: $1" >&2
      exit 1
      ;;
  esac
}

hash_prefix() {
  local algo="$1"
  local value="$2"
  local length="$3"
  if command -v shasum >/dev/null 2>&1; then
    printf '%s' "$value" | shasum -a "$algo" | awk '{print substr($1,1,'"${length}"')}'
  elif command -v openssl >/dev/null 2>&1; then
    printf '%s' "$value" | openssl dgst "-sha${algo}" -hex 2>/dev/null | sed 's/^.*= //' | cut -c1-"$length"
  else
    echo "need shasum or openssl to compute hashes" >&2
    exit 1
  fi
}

config_base_dir() {
  if [[ -n "${XDG_CONFIG_HOME:-}" ]]; then
    printf '%s\n' "${XDG_CONFIG_HOME}"
    return
  fi

  case "$(uname -s)" in
    Darwin)
      printf '%s\n' "${HOME}/Library/Application Support"
      ;;
    *)
      printf '%s\n' "${HOME}/.config"
      ;;
  esac
}

sanitize_pane_id() {
  local pane="$1"
  pane="${pane#%}"
  if [[ -z "${pane}" ]]; then
    printf 'unknown\n'
    return
  fi

  local out=""
  local i ch
  for (( i=0; i<${#pane}; i++ )); do
    ch="${pane:i:1}"
    case "${ch}" in
      [[:alnum:]]|[-_])
        out+="${ch}"
        ;;
      :)
        out+="-"
        ;;
      *)
        out+="_"
        ;;
    esac
  done

  if [[ -z "${out}" ]]; then
    printf 'unknown\n'
  else
    printf '%s\n' "${out}"
  fi
}

write_identity_file() {
  local path="$1"
  local agent_name="$2"
  printf '%s\n' "${agent_name}" > "${path}"
  chmod 600 "${path}" 2>/dev/null || true
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --project)
      PROJECT_PATH="$2"
      shift 2
      ;;
    --pane)
      REQUESTED_PANES+=("$2")
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

require_absolute_path "$PROJECT_PATH"

sha256_short=$(hash_prefix 256 "$PROJECT_PATH" 12)
sha1_short=$(hash_prefix 1 "$PROJECT_PATH" 12)
sha1_uid=$(hash_prefix 1 "$PROJECT_PATH" 20)
source_dir="${HOME}/.local/state/agent-mail/identity/${sha256_short}"

if [[ ! -d "${source_dir}" ]]; then
  echo "source identity directory not found: ${source_dir}" >&2
  exit 1
fi

declare -a panes=()
if (( ${#REQUESTED_PANES[@]} > 0 )); then
  panes=("${REQUESTED_PANES[@]}")
else
  while IFS= read -r pane_path; do
    panes+=("$(basename "${pane_path}")")
  done < <(find "${source_dir}" -maxdepth 1 -type f -name '%*' | sort)
fi

if (( ${#panes[@]} == 0 )); then
  echo "no pane identity files found under ${source_dir}" >&2
  exit 1
fi

canonical_config_root="$(config_base_dir)"
config_short_dir="${canonical_config_root}/agent-mail/identity/${sha1_short}"
config_uid_dir="${canonical_config_root}/agent-mail/identity/${sha1_uid}"
compat_config_short_dir="${HOME}/.config/agent-mail/identity/${sha1_short}"
compat_config_uid_dir="${HOME}/.config/agent-mail/identity/${sha1_uid}"
claude_dir="${HOME}/.claude/agent-mail"

mkdir -p "${config_short_dir}" "${config_uid_dir}" "${compat_config_short_dir}" "${compat_config_uid_dir}" "${claude_dir}"
chmod 700 "${canonical_config_root}/agent-mail" "${canonical_config_root}/agent-mail/identity" "${config_short_dir}" "${config_uid_dir}" 2>/dev/null || true
chmod 700 "${HOME}/.config/agent-mail" "${HOME}/.config/agent-mail/identity" "${compat_config_short_dir}" "${compat_config_uid_dir}" 2>/dev/null || true
chmod 700 "${claude_dir}" 2>/dev/null || true

synced_count=0

for pane in "${panes[@]}"; do
  source_file="${source_dir}/${pane}"
  if [[ ! -f "${source_file}" ]]; then
    echo "missing source pane file: ${source_file}" >&2
    exit 1
  fi

  agent_name=$(sed -n '1p' "${source_file}")
  if [[ -z "${agent_name}" ]]; then
    echo "empty agent name in ${source_file}" >&2
    exit 1
  fi

  sanitized_pane=$(sanitize_pane_id "${pane}")

  write_identity_file "${config_short_dir}/${sanitized_pane}" "${agent_name}"
  write_identity_file "${config_uid_dir}/${sanitized_pane}" "${agent_name}"
  write_identity_file "${compat_config_short_dir}/${sanitized_pane}" "${agent_name}"
  write_identity_file "${compat_config_uid_dir}/${sanitized_pane}" "${agent_name}"
  write_identity_file "${claude_dir}/identity.${sanitized_pane}" "${agent_name}"
  write_identity_file "/tmp/agent-mail-name.${sha1_short}.${sanitized_pane}" "${agent_name}"
  write_identity_file "/tmp/agent-mail-name.${sha1_uid}.${sanitized_pane}" "${agent_name}"

  printf '%s\t%s\n' "${pane}" "${agent_name}"
  synced_count=$((synced_count + 1))
done

cat <<EOF
synced=${synced_count}
project=${PROJECT_PATH}
source_dir=${source_dir}
legacy_config_short=${config_short_dir}
legacy_config_uid=${config_uid_dir}
compat_config_short=${compat_config_short_dir}
compat_config_uid=${compat_config_uid_dir}
legacy_claude=${claude_dir}
legacy_tmp_prefix=/tmp/agent-mail-name.${sha1_short}.*
EOF
