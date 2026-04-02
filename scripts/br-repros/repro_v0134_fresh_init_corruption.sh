#!/usr/bin/env bash
set -euo pipefail

BR_BIN="${BR_BIN:-/tmp/br-target-v0134/release/br}"

if [[ ! -x "$BR_BIN" ]]; then
  echo "Missing executable BR_BIN: $BR_BIN" >&2
  exit 2
fi

tmp=$(mktemp -d /tmp/br-repro-v0134-init.XXXXXX)
cd "$tmp"

echo "workspace=$tmp"
"$BR_BIN" init
echo "integrity_check:"
sqlite3 .beads/beads.db 'PRAGMA integrity_check;'

