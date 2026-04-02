#!/usr/bin/env bash
set -euo pipefail

PINNED_VERSION="${RR_BR_PINNED_VERSION:-0.1.34}"
LOCAL_BIN_DIR="${RR_LOCAL_BIN_DIR:-${HOME}/.local/bin}"
DEFAULT_BR_PATH="${RR_BR_DEFAULT_PATH:-${LOCAL_BIN_DIR}/br}"
PINNED_BR_PATH="${RR_BR_PINNED_PATH:-${LOCAL_BIN_DIR}/br-${PINNED_VERSION}.pinned}"
STRICT_TARGET="${RR_BR_STRICT_TARGET:-1}"

REPAIR=1
QUIET=0
PRINT_PATH=0

usage() {
  cat <<EOF
Usage: $(basename "$0") [options]

Ensure the default br path resolves to the vetted $PINNED_VERSION build.

Options:
  --repair       Repair pathing when needed (default)
  --no-repair    Validate only; do not rewrite files/symlinks
  --print-path   Print resolved default br path only
  --quiet        Suppress informational output
  -h, --help     Show this help
EOF
}

log() {
  if (( QUIET == 0 )); then
    printf '%s\n' "$*"
  fi
}

fail() {
  printf 'resolve_br.sh: %s\n' "$*" >&2
  exit 1
}

version_of_bin() {
  local bin_path="$1"
  local version

  if [[ ! -x "$bin_path" ]]; then
    return 1
  fi

  version="$("$bin_path" --version 2>/dev/null | awk 'NR==1 { print $2 }')"
  [[ -n "$version" ]] || return 1
  printf '%s\n' "$version"
}

is_pinned_version_bin() {
  local bin_path="$1"
  local version

  version="$(version_of_bin "$bin_path")" || return 1
  [[ "$version" == "$PINNED_VERSION" ]]
}

ensure_not_backup_target() {
  local link_path="$1"
  local link_target

  if [[ -L "$link_path" ]]; then
    link_target="$(readlink "$link_path")"
    if [[ "$link_target" == *.bak ]]; then
      fail "default br link points at backup binary: $link_target"
    fi
  fi
}

add_candidate() {
  local candidate="$1"
  local existing

  [[ -n "$candidate" ]] || return 0
  if (( ${#CANDIDATES[@]} > 0 )); then
    for existing in "${CANDIDATES[@]}"; do
      if [[ "$existing" == "$candidate" ]]; then
        return 0
      fi
    done
  fi
  CANDIDATES+=("$candidate")
}

find_pinned_source() {
  local candidate
  for candidate in "${CANDIDATES[@]}"; do
    if is_pinned_version_bin "$candidate"; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done
  return 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repair)
      REPAIR=1
      shift
      ;;
    --no-repair)
      REPAIR=0
      shift
      ;;
    --print-path)
      PRINT_PATH=1
      shift
      ;;
    --quiet)
      QUIET=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

mkdir -p "$LOCAL_BIN_DIR"

declare -a CANDIDATES=()
add_candidate "$PINNED_BR_PATH"
add_candidate "${LOCAL_BIN_DIR}/br-${PINNED_VERSION}.localfix"
add_candidate "${LOCAL_BIN_DIR}/br-${PINNED_VERSION}.queuebug.bak"
add_candidate "${LOCAL_BIN_DIR}/br-${PINNED_VERSION}.bak"
add_candidate "${LOCAL_BIN_DIR}/br.${PINNED_VERSION}.bak"
add_candidate "${HOME}/.cargo/bin/br"
if command -v br >/dev/null 2>&1; then
  add_candidate "$(command -v br)"
fi

if (( REPAIR == 1 )); then
  if ! is_pinned_version_bin "$PINNED_BR_PATH"; then
    if (( STRICT_TARGET == 1 )); then
      fail "expected vetted br target missing or wrong version: $PINNED_BR_PATH (set RR_BR_STRICT_TARGET=0 to allow fallback copy)"
    fi
    source_path="$(find_pinned_source)" || fail "could not find a usable br $PINNED_VERSION candidate"
    cp "$source_path" "$PINNED_BR_PATH"
    chmod 755 "$PINNED_BR_PATH"
  fi

  ln -sfn "$PINNED_BR_PATH" "$DEFAULT_BR_PATH"
fi

is_pinned_version_bin "$DEFAULT_BR_PATH" || fail "default br path is not pinned to $PINNED_VERSION: $DEFAULT_BR_PATH"
ensure_not_backup_target "$DEFAULT_BR_PATH"

if (( PRINT_PATH == 1 )); then
  printf '%s\n' "$DEFAULT_BR_PATH"
  exit 0
fi

log "Pinned br path: $DEFAULT_BR_PATH"
log "Pinned br version: $(version_of_bin "$DEFAULT_BR_PATH")"
log "Pinned br target: $PINNED_BR_PATH"
