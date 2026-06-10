#!/usr/bin/env bash
set -euo pipefail

BR_BIN="${BR_BIN:-$HOME/.local/bin/br}"

if [[ ! -x "$BR_BIN" ]]; then
  echo "Missing executable BR_BIN: $BR_BIN" >&2
  exit 2
fi

tmp=$(mktemp -d /tmp/br-repro-rebuild.XXXXXX)
trap 'rm -rf "$tmp"' EXIT

cd "$tmp"
"$BR_BIN" init >/dev/null
"$BR_BIN" create "tiny" --type task >/dev/null

mkdir -p second/.beads
cp .beads/issues.jsonl second/.beads/issues.jsonl
cp .beads/config.yaml second/.beads/config.yaml

cd second
"$BR_BIN" init >/dev/null
"$BR_BIN" sync --import-only --rebuild >/tmp/br-rebuild-sync.out 2>/tmp/br-rebuild-sync.err

echo "workspace=$tmp/second"
echo "version=$("$BR_BIN" --version)"
echo "sync_stdout_start"
cat /tmp/br-rebuild-sync.out
echo "sync_stdout_end"
echo "sync_stderr_start"
cat /tmp/br-rebuild-sync.err
echo "sync_stderr_end"
echo "integrity_check:"
sqlite3 .beads/beads.db 'PRAGMA integrity_check;'
