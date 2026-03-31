#!/usr/bin/env bash
set -euo pipefail

PORT="${AGENT_MAIL_WATCH_PORT:-8781}"
AGENTS="${AGENT_MAIL_WATCH_AGENTS:-BlueLake,GreenCastle}"
PROJECT="${AGENT_MAIL_WATCH_PROJECT:-/Users/cdilga/Documents/dev/roger-reviewer}"

exec python3 scripts/agent_mail_watch.py \
  --bind 127.0.0.1 \
  --port "$PORT" \
  --project-key "$PROJECT" \
  --agents "$AGENTS" \
  "$@"
