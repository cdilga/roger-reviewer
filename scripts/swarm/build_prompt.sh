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
- This pane already has a stable Agent Mail identity chosen for it. Reuse that exact identity and do not rename yourself.
- If Agent Mail says the project or your identity does not exist yet, repair that by calling \`ensure_project\` for the repo path and then \`register_agent\` with this pane's existing identity before proceeding.
- If Codex shows the directory trust prompt, choose \`Yes, continue\` immediately.
- Start each cycle with Agent Mail inbox + ack checks, then \`./scripts/swarm/br_safe.sh ready -> ./scripts/swarm/br_safe.sh show -> claim -> reserve files\`.
- Treat the guarded \`br\` surface as authoritative: use \`./scripts/swarm/br_safe.sh\` for both reads and mutations, and do not fall back to older fixed-version pins or ad hoc release artefacts unless you are in an explicit repro lane.
- Do not treat launcher text as a bead assignment; self-select from \`./scripts/swarm/br_safe.sh ready\` (use \`bv\` only for ranking context).
- If an operator message or pane-specific nudge names a bead, treat that as the immediate priority for the current checkpoint, not as a forever-only lane. Own one bead at a time, finish or truthfully block it, then return to \`./scripts/swarm/br_safe.sh ready\` for the next safe bead unless the user explicitly freezes you on that bead.
- Record exact validation commands when closing beads and run \`./scripts/swarm/br_safe.sh sync --flush-only\` after bead state/note changes.
- If \`br\` reports \`database is busy\`, back off and retry before concluding queue state.
- If \`./scripts/swarm/br_safe.sh\` reports degraded trust, use its read fallback for queue inspection and repair the workspace with \`./scripts/swarm/rebuild_beads_db_safe.sh --install\` before DB-backed mutations.
- If \`./scripts/swarm/check_beads_trust.sh\` reports \`TRUST_STATUS=pass\` and \`br doctor\` is clean, treat the bead workspace as currently safe to use through \`./scripts/swarm/br_safe.sh\`. Do not avoid bead work because of historical \`br\` incidents once the current trust check is green.
- Full operating doctrine lives at \`docs/swarm/worker-operating-doctrine.md\`.
- If this pane is part of a persistent interactive tmux swarm, do not stop after one checkpoint. After each durable checkpoint, loop back through Agent Mail, \`./scripts/swarm/br_safe.sh ready\`, \`./scripts/swarm/br_safe.sh show <id>\`, claim the next unblocked bead, and keep moving until the queue is genuinely exhausted for you, a real blocker appears, or the user redirects you.
- Plain \`ntm send\` only seeds work. Persistent churn comes from your own interactive loop plus the operator control plane (\`ntm assign --watch\`, \`ntm controller\`, or equivalent reclaim logic).
- Do not run \`ntm spawn\`, \`ntm create\`, \`ntm add\`, or create new suites/sessions/worktrees from inside this worker pane. Those are operator actions outside the worker loop. Your job in-pane is to finish or truthfully unblock beads in the existing swarm.
- Almost every implementation bead should add or update automated tests. Default to unit or parameterized coverage for local rules, reducers, serializers, state transitions, invalidation logic, and shaping logic. Use narrow integration tests for real boundaries such as storage, migrations, adapters, CLI/TUI controller seams, prompt execution edges, bridge envelopes, and similar cross-component contracts.
- Do not close an implementation bead without test additions or updates unless you leave an explicit no-new-test decision that names why deterministic unit/integration coverage would be untruthful and what narrower proof you ran instead.
- Do not imply integration or E2E coverage that does not exist yet. If the missing proof is required for an honest closeout and it is still one truthful slice, implement it now; otherwise create or claim the follow-on testing bead immediately instead of closing early.
- Treat validation as part of execution, not as the terminal state of execution. After tests are green and the bead is closed or handed off truthfully, immediately re-enter the ready loop and pick up the next safe bead unless the queue is genuinely exhausted, a real blocker remains, or the user redirects you.
- If \`rch\` is installed, prefer \`rch exec -- <command>\` for CPU-heavy cargo builds/tests. If no worker fleet exists, local fail-open execution is still acceptable; do not wait for remote capacity that is not actually configured.
- Do not use a PR-based dev workflow for swarm work: no \`gh pr\`, no opening/managing PRs, no PR review/comment loops for your own changes, and no assumption that each agent should branch off independently unless the user explicitly says so.
- Lane assignment: this worker is in the \`${WORK_LANE}\` lane.
- Control-plane mode is \`${CONTROL_PLANE_MODE}\`: panes are expected to be reclaimed by persistent assign-watch/controller loops, so if no safe bead is claimable post an explicit exhausted-queue status in Agent Mail instead of idling silently.
EOF
  if [[ -n "$LEGACY_AGENT_NAME" ]]; then
    printf '%s\n' "- Legacy launch note: an older launcher passed the identity hint \`${LEGACY_AGENT_NAME}\`, but the NTM-provisioned identity already attached to this pane takes precedence."
  fi
  lane_guidance
} >"$OUTPUT_FILE"
