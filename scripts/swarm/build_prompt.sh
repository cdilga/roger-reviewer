#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 || $# -gt 4 ]]; then
  echo "Usage: $(basename "$0") <base-prompt-file> [legacy-agent-name] <output-file> [implementation|maintenance]" >&2
  exit 1
fi

BASE_PROMPT_FILE="$1"
LEGACY_AGENT_NAME=""
OUTPUT_FILE=""
WORK_LANE="implementation"
CONTROL_PLANE_MODE="${SWARM_CONTROL_MODE:-assign}"

case "$#" in
  2)
    OUTPUT_FILE="$2"
    ;;
  3)
    if [[ "$3" == "implementation" || "$3" == "maintenance" ]]; then
      OUTPUT_FILE="$2"
      WORK_LANE="$3"
    else
      LEGACY_AGENT_NAME="$2"
      OUTPUT_FILE="$3"
    fi
    ;;
  4)
    LEGACY_AGENT_NAME="$2"
    OUTPUT_FILE="$3"
    WORK_LANE="$4"
    ;;
esac

case "$WORK_LANE" in
  implementation|maintenance)
    ;;
  *)
    echo "Invalid lane '$WORK_LANE' (expected implementation or maintenance)." >&2
    exit 1
    ;;
esac

case "$CONTROL_PLANE_MODE" in
  assign|nudge)
    ;;
  *)
    CONTROL_PLANE_MODE="assign"
    ;;
esac

if [[ ! -f "$BASE_PROMPT_FILE" ]]; then
  echo "Prompt file not found: $BASE_PROMPT_FILE" >&2
  exit 1
fi

mkdir -p "$(dirname "$OUTPUT_FILE")"

lane_guidance() {
  case "$WORK_LANE" in
    implementation)
      cat <<'EOF'
- You are in the implementation lane. Do not proactively take bead-health/tracker-repair work as background cleanup.
- If queue trust looks degraded (cache/repair anomalies, DB busy flapping, bead graph hygiene), report it and continue implementation unless that blocker is highest leverage and explicitly claimed.
- Only switch into maintenance work after an explicit claim/assignment to a maintenance bead.
EOF
      ;;
    maintenance)
      cat <<'EOF'
- You are in the maintenance lane. Prioritize bead-health, queue-trust, runbook, and swarm-operability repairs.
- Do not claim product implementation beads unless maintenance work is exhausted or a handoff message explicitly redirects you.
- Keep maintenance edits scoped so implementation workers can proceed in parallel without overlap.
EOF
      ;;
  esac
}

{
  cat "$BASE_PROMPT_FILE"
  cat <<EOF

Persistent swarm identity rules:
- This pane already has a stable Agent Mail identity provisioned by upstream NTM. Reuse that exact identity. Do not register a new one or rename yourself.
- Start each cycle with Agent Mail inbox + ack checks, then \`br ready -> br show -> claim -> reserve files\`.
- Do not treat launcher text as a bead assignment; self-select from \`br ready\` (use \`bv\` only for ranking context).
- Record exact validation commands when closing beads and run \`br sync --flush-only\` after bead state/note changes.
- If \`br\` reports \`database is busy\`, back off and retry before concluding queue state.
- Full operating doctrine lives at \`docs/swarm/worker-operating-doctrine.md\`.
- Lane assignment: this worker is in the \`${WORK_LANE}\` lane.
- Control-plane mode is \`${CONTROL_PLANE_MODE}\`: panes are expected to be reclaimed by persistent assign-watch/controller loops, so if no safe bead is claimable post an explicit exhausted-queue status in Agent Mail instead of idling silently.
EOF
  if [[ -n "$LEGACY_AGENT_NAME" ]]; then
    printf '%s\n' "- Legacy launch note: an older launcher passed the identity hint \`${LEGACY_AGENT_NAME}\`, but the NTM-provisioned identity already attached to this pane takes precedence."
  fi
  lane_guidance
} >"$OUTPUT_FILE"
