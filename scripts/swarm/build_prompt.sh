#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "Usage: $(basename "$0") <base-prompt-file> <agent-name> <output-file>" >&2
  exit 1
fi

BASE_PROMPT_FILE="$1"
AGENT_NAME="$2"
OUTPUT_FILE="$3"

if [[ ! -f "$BASE_PROMPT_FILE" ]]; then
  echo "Prompt file not found: $BASE_PROMPT_FILE" >&2
  exit 1
fi

mkdir -p "$(dirname "$OUTPUT_FILE")"

{
  cat "$BASE_PROMPT_FILE"
  cat <<EOF

Persistent swarm identity rules:
- Your Agent Mail identity for this swarm is exactly \`${AGENT_NAME}\`. Reuse that exact name. Do not invent a new identity.
- Register or refresh that exact Agent Mail name immediately before doing other coordination work.
- Do not treat launcher text as a bead assignment. Self-select from \`br ready\` and \`bv\`.
- If the next safe slice is missing, you may create or split beads yourself, but keep dependency truth and add a validation contract.
- If \`br\` says the database is busy, wait briefly and retry instead of assuming there is no work.
- Continue autonomously across multiple beads until the backlog is exhausted, blocked, or you need human steering.
EOF
} >"$OUTPUT_FILE"
