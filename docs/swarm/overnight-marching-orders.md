First read `AGENTS.md` carefully, then read `README.md`, then read the canonical plan in `docs/PLAN_FOR_ROGER_REVIEWER.md` so you understand the project authority order, current planning-stage status, and the `0.1.0` architecture direction.

This repo is still in planning and bead-polishing. Do not start product implementation. Work only on planning, architecture reconciliation, bead polishing, readiness-review prep, repo discovery, and related documentation tasks that are actually allowed by `AGENTS.md`.

Register with MCP Agent Mail immediately, introduce yourself to the other active agents, and keep checking your inbox. Use Agent Mail file reservations before editing any docs or repo files. Acknowledge messages that require it, and do not drift into communication-only loops without moving work forward.

Use `br ready` as the source of truth for what is truly unblocked. Use `bv --robot-triage` or `bv --robot-next` only to rank or understand the queue, then verify the bead with `br show <id>` before claiming it. If `bv` points at something blocked, trust `br ready` and choose a different bead.

Do not treat any launcher text as a bead assignment. You must choose work yourself from the live backlog.

When you pick work:

1. Claim it with `br update <id> --status in_progress`.
2. Reserve the files you expect to touch through Agent Mail.
3. Announce the bead you are taking and the files you reserved.
4. Execute the bead exactly to its acceptance criteria and no further.
5. If you change bead state or notes, run `br sync --flush-only`.

Be meticulous about authority. `AGENTS.md` beats historical critique docs, and `docs/PLAN_FOR_ROGER_REVIEWER.md` beats bead-seed drift unless the user explicitly asks for a plan update. If you hit ambiguity, document it in the bead or send Agent Mail instead of guessing.

Re-read `AGENTS.md` after every compaction or long interruption so the operating rules stay fresh. The durable state lives in beads and Agent Mail, so use them continuously.

If you are running in a headless swarm cycle, work until you reach a durable checkpoint, then stop cleanly. A durable checkpoint means one of:

1. you made meaningful progress on a claimed bead and left the bead and mail state accurate
2. you completed and closed a bead and synced beads
3. you hit a real blocker and documented it in the bead and Agent Mail

Do not stop at a shallow status note if useful work is still available, and do not wait for human babysitting between beads when `br ready` still shows valid unblocked work.
