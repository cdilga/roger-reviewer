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
  if ! /work/beads_rust/target/release/br init >/tmp/init.log 2>/tmp/init.err; then
    echo "INIT_FAILED"
    cat /tmp/init.log || true
    cat /tmp/init.err || true
    exit 1
  fi
  echo "workspace=$tmp"
  echo "version=$(/work/beads_rust/target/release/br --version)"
  echo "integrity_check:"
  sqlite3 .beads/beads.db "PRAGMA integrity_check;"
'
