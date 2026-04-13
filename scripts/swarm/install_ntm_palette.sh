#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd "${SCRIPT_DIR}/../.." && pwd)
SOURCE_FILE="${PROJECT_ROOT}/docs/swarm/command_palette.md"
TARGET_DIR="${HOME}/.config/ntm"
TARGET_FILE="${TARGET_DIR}/command_palette.md"
MODE="symlink"

usage() {
  cat <<EOF
Usage: $(basename "$0") [--copy|--symlink] [--show]

Install the repo-local NTM command palette into ~/.config/ntm/command_palette.md.

Options:
  --copy       Copy the palette file instead of symlinking it
  --symlink    Symlink the palette file (default)
  --show       Print source and target paths, then exit
  -h, --help   Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --copy)
      MODE="copy"
      shift
      ;;
    --symlink)
      MODE="symlink"
      shift
      ;;
    --show)
      printf 'source=%s\ntarget=%s\n' "$SOURCE_FILE" "$TARGET_FILE"
      exit 0
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

if [[ ! -f "$SOURCE_FILE" ]]; then
  echo "Source palette file not found: $SOURCE_FILE" >&2
  exit 1
fi

mkdir -p "$TARGET_DIR"

case "$MODE" in
  symlink)
    ln -sfn "$SOURCE_FILE" "$TARGET_FILE"
    echo "Symlinked NTM palette: $TARGET_FILE -> $SOURCE_FILE"
    ;;
  copy)
    cp "$SOURCE_FILE" "$TARGET_FILE"
    echo "Copied NTM palette to: $TARGET_FILE"
    ;;
esac
