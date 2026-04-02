#!/usr/bin/env bash
set -euo pipefail

BR_BIN="${BR_BIN:-/tmp/br-target-v0134/release/br}"

if [[ ! -x "$BR_BIN" ]]; then
  echo "Missing executable BR_BIN: $BR_BIN" >&2
  exit 2
fi

tmp=$(mktemp -d /tmp/br-repro-readmut.XXXXXX)
cd "$tmp"

"$BR_BIN" init >/dev/null
id1=$("$BR_BIN" create 'alpha' --silent)
id2=$("$BR_BIN" create 'beta' --silent)
"$BR_BIN" dep add "$id1" "$id2" >/dev/null

echo "workspace=$tmp"
echo "ids=$id1,$id2"
echo "before:"
sqlite3 .beads/beads.db "select key,value from metadata where key='blocked_cache_state';"
echo "ready:"
"$BR_BIN" ready
echo "after:"
sqlite3 .beads/beads.db "select key,value from metadata where key='blocked_cache_state';"
