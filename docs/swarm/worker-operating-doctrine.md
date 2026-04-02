This is the long-form worker operating doctrine for swarm runs.
The concise worker startup prompt is `docs/swarm/overnight-marching-orders.md`.

First read `AGENTS.md` carefully, then read `README.md`. Before claiming your first bead, skim the canonical plan in `docs/PLAN_FOR_ROGER_REVIEWER.md` only far enough to understand the project authority order, current implementation-stage status, and the `0.1.0` architecture direction. Do not burn the whole first turn on a full line-by-line plan read while ready work is waiting. After you choose a bead, read the relevant plan sections and `br show <id>` in full.

The implementation gate has passed. You may now claim and execute implementation beads that are actually allowed by `AGENTS.md`, the canonical plan, and the current user instruction. Do not self-block on outdated planning-only assumptions.

Register with MCP Agent Mail immediately, introduce yourself to the other active agents, and keep checking your inbox. Use Agent Mail file reservations before editing any docs or repo files. Acknowledge messages that require it, and do not drift into communication-only loops without moving work forward.

Frankenterm (`ft`) is the default observer path for swarm sessions. If `ft` is missing, install it with `./scripts/swarm/install_frankenterm.sh` unless the run is intentionally degraded (`--no-ft`). Keep limits explicit in status notes: `ft` only sees panes discoverable through WezTerm CLI; tmux-internal panes not surfaced by WezTerm stay outside `ft` visibility.

Use `br ready` as the source of truth for what is truly unblocked. Use `bv --robot-triage` or `bv --robot-next` only to rank or understand the queue, then verify the bead with `br show <id>` before claiming it. If `bv` points at something blocked, trust `br ready` and choose a different bead.

If `br` reports `database is busy`, that is lock contention, not "no work".
Back off briefly and retry before deciding the queue is empty.
If standard `br` reads or claims still fail after a few retries, switch to the
direct fallback path for queue truth and claiming:

1. `br ready --no-daemon`
2. `br show <id> --no-daemon`
3. `br update <id> --status in_progress --no-daemon`

Use the first clean `--no-daemon` result as authoritative rather than parking
on a busy DB. Announce in Agent Mail when you had to fall back so other workers
know the queue view came from the direct path.
For scripted queue-inspection reads (`ready/list/show`), prefer `--no-auto-import --no-auto-flush` so read paths do not trigger hidden write-side repair under contention.
For launch preflight and prerequisites, treat transient `br doctor` sqlite lock
signals as retry-class, and treat preserved recovery-artifact warnings, sidecar
warnings, plus the stale blocked-cache recoverable-anomaly line as advisory
unless another fatal `ERROR` is present.

Before launching a large swarm batch, the operator should run
`./scripts/swarm/audit_bead_batch.sh --limit 20 --strict` from repo root.
That audit is the pre-launch pass for missing-leaf discovery, dependency sanity,
and acceptance-clarity checks so workers do not rediscover those issues mid-run.

Do not treat any launcher text as a bead assignment. You must choose work yourself from the live backlog.

You are explicitly allowed to shape the backlog when the next safe slice is missing. If the graph is too narrow, the current bead is too large, or a blocker needs to be isolated, create or update beads yourself instead of waiting for a human. Valid autonomy includes:

1. splitting a large bead into smaller non-overlapping child beads
2. creating a planning or design bead to settle a blocking unknown
3. creating a spike bead to test a risky implementation seam
4. creating a bead whose only purpose is to widen safe parallel work for other agents
5. adding notes or dependency updates when the current graph is missing an important edge

Do this conservatively and truthfully. New beads must be justified by the canonical plan and current repo reality, not invented busywork. When you create or split a bead, announce it in Agent Mail so other agents can pick it up immediately.

When you create or refine an implementation bead, include the validation
contract that will be required to close it. Name the cheapest truthful layer:
`unit`, `prop`, `int`, `accept`, `e2e`, or manual `smoke`, and record the
expected suite or command. Do not close a bead on smoke alone unless smoke is
explicitly the correct layer for that bead.

When you pick work:

1. Claim it with `br update <id> --status in_progress`.
2. Reserve the files you expect to touch through Agent Mail.
3. Announce the bead you are taking and the files you reserved.
4. Execute the bead exactly to its acceptance criteria and no further.
5. Run the validation required by that bead's contract before closing it.
6. Record the exact validation command or suite result in the bead close reason
   or notes. Do not imply broader coverage than what actually ran.
7. If you change bead state or notes, run `br sync --flush-only`.

When `br ready` is empty but useful work still obviously exists, do not stop at "queue empty". Instead:

1. run `./scripts/swarm/audit_bead_batch.sh --limit 20 --strict` and follow its queue-repair playbook
2. inspect the active frontier with `br blocked`, `br show`, and `bv --robot-triage`
3. identify the narrowest safe next slice or missing contract
4. create, split, or update the relevant bead if that is the honest next step
5. claim that new or clarified bead yourself, or announce it for another agent to claim

Your job is not just to consume beads mechanically. Your job is to keep the machine moving while preserving safety, truthfulness, and dependency discipline.

Be meticulous about authority. `AGENTS.md` beats historical critique docs, and `docs/PLAN_FOR_ROGER_REVIEWER.md` beats bead-seed drift unless the user explicitly asks for a plan update. If you hit ambiguity, document it in the bead or send Agent Mail instead of guessing.

Implementation is explicitly allowed now.

Important distinction: you are building Roger Reviewer itself, not acting as a Roger-internal review agent. Treat the repo's approval/posting constraints as product requirements the software must preserve, not as a ban on implementing those areas.

You may implement GitHub-adjacent, approval, posting, and mutation-sensitive flows where the backlog and plan call for them. The real restriction is on live external actions during this swarm run: do not actually post to GitHub or mutate external/dev/test environments unless the user explicitly authorizes that action.

If you need CPU-heavy cargo builds or tests and `rch` is available, prefer `rch exec -- <command>`. If `rch` is installed locally without a worker fleet, it may fail open to local execution; do not sit idle waiting for remote capacity that is not actually configured.

Re-read `AGENTS.md` after every compaction or long interruption so the operating rules stay fresh. The durable state lives in beads and Agent Mail, so use them continuously.

If you are in a persistent interactive tmux swarm session, do not stop after a single checkpoint. After each useful checkpoint, immediately:

1. re-check Agent Mail and acknowledge anything important
2. re-run `br ready`
3. verify the next candidate with `br show <id>`
4. claim the next unblocked bead you can usefully advance
5. if no bead is ready but the next slice is obvious, create or split the bead needed to continue safely

Only stop when the live queue is genuinely exhausted for you, you hit a real blocker that prevents more progress, or the user explicitly redirects you.

If you are running in a headless one-shot cycle instead, then stop cleanly after a durable checkpoint and let the outer launcher invoke you again.
