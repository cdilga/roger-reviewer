#!/usr/bin/env bash
set -euo pipefail

VERSION="${VERSION:-0.1.40}"
IMAGE="${IMAGE:-debian:trixie-slim}"
PLATFORM="${PLATFORM:-linux/arm64}"
ARCHIVE_URL="${ARCHIVE_URL:-https://github.com/Dicklesworthstone/beads_rust/releases/download/v${VERSION}/br-v${VERSION}-linux_arm64.tar.gz}"

docker run --rm --platform "${PLATFORM}" "${IMAGE}" bash -lc "
  set -euo pipefail
  export DEBIAN_FRONTEND=noninteractive
  apt-get update >/dev/null
  apt-get install -y curl ca-certificates sqlite3 tar >/dev/null
  curl -fsSL -o /tmp/br.tar.gz '${ARCHIVE_URL}'
  mkdir -p /tmp/brdir
  tar -xzf /tmp/br.tar.gz -C /tmp/brdir
  chmod +x /tmp/brdir/br

  tmp=\$(mktemp -d)
  cd \"\$tmp\"
  /tmp/brdir/br init >/dev/null
  /tmp/brdir/br create tiny --type task >/dev/null

  mkdir -p second/.beads
  cp .beads/issues.jsonl second/.beads/issues.jsonl
  cp .beads/config.yaml second/.beads/config.yaml

  cd second
  /tmp/brdir/br init >/dev/null
  /tmp/brdir/br sync --import-only --rebuild >/tmp/sync.out 2>/tmp/sync.err

  echo workspace=\$tmp/second
  echo version=\$(/tmp/brdir/br --version)
  echo sync_stdout_start
  cat /tmp/sync.out
  echo sync_stdout_end
  echo sync_stderr_start
  cat /tmp/sync.err
  echo sync_stderr_end
  echo integrity_check:
  sqlite3 .beads/beads.db 'PRAGMA integrity_check;'
"
