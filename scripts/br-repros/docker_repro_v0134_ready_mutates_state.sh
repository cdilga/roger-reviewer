#!/usr/bin/env bash
set -euo pipefail

IMAGE="${IMAGE:-rust:1.88-bookworm}"

docker run --rm "$IMAGE" bash -lc '
  set -euo pipefail
  CARGO_BIN=/usr/local/cargo/bin/cargo
  export DEBIAN_FRONTEND=noninteractive
  apt-get update >/dev/null
  apt-get install -y git sqlite3 >/dev/null
  git clone --depth 1 https://github.com/Dicklesworthstone/frankensqlite.git /work/frankensqlite >/dev/null 2>&1
  git clone --depth 1 --branch v0.1.34 https://github.com/Dicklesworthstone/beads_rust.git /work/beads_rust >/dev/null 2>&1
  cd /work/beads_rust
  if ! "$CARGO_BIN" build --release --bin br >/tmp/build.log 2>&1; then
    echo "BUILD_FAILED"
    tail -n 120 /tmp/build.log || true
    exit 1
  fi
  tmp=$(mktemp -d)
  cd "$tmp"
  BR=/work/beads_rust/target/release/br
  "$BR" init >/dev/null
  id1=$("$BR" create alpha --silent)
  id2=$("$BR" create beta --silent)
  "$BR" dep add "$id1" "$id2" >/dev/null
  echo "workspace=$tmp"
  echo "version=$("$BR" --version)"
  echo "ids=$id1,$id2"
  echo "before:"
  sqlite3 .beads/beads.db "select key,value from metadata where key='\''blocked_cache_state'\'';"
  echo "ready:"
  "$BR" ready
  echo "after:"
  sqlite3 .beads/beads.db "select key,value from metadata where key='\''blocked_cache_state'\'';"
'
