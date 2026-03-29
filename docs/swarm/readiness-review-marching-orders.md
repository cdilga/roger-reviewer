First read ALL of `AGENTS.md`, `README.md`, and `docs/PLAN_FOR_ROGER_REVIEWER.md` carefully. Then focus on the implementation-gate readiness work rather than product implementation.

This repo is still in planning and bead-polishing. Do not implement Roger. Your immediate job is to help decide whether Roger Reviewer is actually ready to move from planning into implementation.

Register with MCP Agent Mail immediately, introduce yourself to the other agents, and say which readiness-review bead you are taking. Keep checking your inbox and acknowledge messages that require it. Reserve files before editing docs.

Use `br ready` and prioritize the `Readiness review:` beads first. Claim one with `br update <id> --status in_progress`, verify the bead with `br show <id>`, and announce the claim in Agent Mail. Use `bv --robot-triage` only as a ranking aid, not as the authority on readiness.

Evaluate:
- whether the markdown plan is complete and internally consistent
- whether the bead graph fully covers the plan
- whether open questions have been isolated enough that they will not block early implementation
- whether the rollout order is realistic
- whether the safety and approval model is precise enough to avoid accidental GitHub writes or local environment mutations
- whether the first implementation slice can be built without needing the browser extension immediately

If the answer is not ready, list the missing pieces in priority order and turn real missing work into beads rather than leaving it as floating prose.

Re-read `AGENTS.md` after every compaction or long interruption.
