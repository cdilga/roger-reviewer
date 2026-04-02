#!/usr/bin/env bash
set -euo pipefail

IMAGE="${IMAGE:-rust:1.88-bookworm}"
ITERATIONS="${ITERATIONS:-120}"

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
  echo "workspace=$tmp"
  echo "version=$("$BR" --version)"
  echo "iterations='"$ITERATIONS"'"
  for i in $(seq 1 '"$ITERATIONS"'); do
    parent_id=$("$BR" create "probe-parent-$i" --type task --silent)
    child_id=$("$BR" create "probe-child-$i" --type task --silent)
    "$BR" update "$child_id" --notes "probe-created-notes-$i" >/dev/null
    "$BR" dep add "$child_id" "$parent_id" --type parent-child >/dev/null
    "$BR" ready >/dev/null 2>&1 || true
    "$BR" show "$child_id" >/dev/null 2>&1 || true
    "$BR" update "$child_id" --notes "probe-notes-$i" >/dev/null
    "$BR" update "$child_id" --acceptance-criteria "probe-acceptance-$i" >/dev/null
    if ! out=$("$BR" update "$parent_id" --notes "probe-parent-notes-$i" 2>&1); then
      echo "failure_iteration=$i"
      echo "$out"
      exit 12
    fi
    if [ $((i % 25)) -eq 0 ]; then
      echo "progress=$i"
    fi
  done
  echo "fk_failure_detected=0"
  sqlite3 .beads/beads.db "PRAGMA foreign_key_check;"
'
