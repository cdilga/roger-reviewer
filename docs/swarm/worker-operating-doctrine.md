This is the long-form worker operating doctrine for swarm runs.
The concise worker startup prompt is `docs/swarm/overnight-marching-orders.md`.

First read `AGENTS.md` carefully, then read `README.md`. Before claiming your
first bead, re-anchor on the canonical plan in `docs/PLAN_FOR_ROGER_REVIEWER.md`
enough to refresh the project authority order, current implementation-stage
status, local-core-first `0.1.0` architecture direction, and support-claim
truthfulness model. Do not burn the whole first turn on a full line-by-line
plan read while ready work is waiting, but do not treat the plan as optional
context either. After you choose a bead, read the relevant plan sections and
`br show <id>` in full.

Read `docs/beads/BEAD_AND_PROMPT_FAILURE_PATTERNS.md` when shaping beads,
writing launcher prompts, or recovering from a run that previously produced
partial or misleading closeouts.

The implementation gate has passed. You may now claim and execute implementation beads that are actually allowed by `AGENTS.md`, the canonical plan, and the current user instruction. Do not self-block on outdated planning-only assumptions.

Register with MCP Agent Mail immediately, introduce yourself to the other active agents, and keep checking your inbox. Use Agent Mail file reservations before editing any docs or repo files. Acknowledge messages that require it, and do not drift into communication-only loops without moving work forward.

Frankenterm (`ft`) is the default observer path for swarm sessions. If `ft` is missing, install it with `./scripts/swarm/install_frankenterm.sh` unless the run is intentionally degraded (`--no-ft`). Keep limits explicit in status notes: `ft` only sees panes discoverable through WezTerm CLI; tmux-internal panes not surfaced by WezTerm stay outside `ft` visibility.

Use `br ready` as the source of truth for what is truly unblocked. Use `bv --robot-triage` or `bv --robot-next` only to rank or understand the queue, then verify the bead with `br show <id>` before claiming it. If `bv` points at something blocked, trust `br ready` and choose a different bead.

If `br` reports `database is busy`, that is lock contention, not "no work".
Back off briefly and retry before deciding the queue is empty.
For scripted or bulk mutation paths (`create`/`update`/`close`/`sync`), prefer
`./scripts/swarm/br_pinned.sh ...` over raw `br ...`; the wrapper serializes
mutating calls behind a repo-local advisory lock and injects a longer
`--lock-timeout` unless one is already set.
If standard `br` reads or claims still fail after a few retries, switch to the
direct fallback path for queue truth and claiming:

1. `br ready --no-daemon`
2. `br show <id> --no-daemon`
3. `br update <id> --status in_progress --no-daemon`

Use the first clean `--no-daemon` result as authoritative rather than parking
on a busy DB. Announce in Agent Mail when you had to fall back so other workers
know the queue view came from the direct path.
For scripted queue-inspection reads (`ready/list/show`), prefer `--no-auto-import --no-auto-flush` so read paths do not trigger hidden write-side repair under contention.
If the busy error is specifically a snapshot conflict (`SQLITE_BUSY_SNAPSHOT`
or `snapshot conflict on pages ...`), a long-lived reader such as `bv` may be
holding a stale snapshot after checkpoint/repair work. In that case:

1. use `br ready --no-db`, `br show <id> --no-db`, or `br list --status open --no-db`
   only for read-only queue inspection
2. restart the stale reader (`bv` or other long-lived DB observers) before
   trusting DB-backed `br` reads again
3. do not use `--no-db` for claiming, closing, syncing, or any other mutation
   path; move back to DB-backed `br update/close/sync` only after the stale
   reader is gone

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

CI-sensitive closeout categories require remote evidence in closeout notes:

1. labels include `ci` or `github-actions` (remote CI validation category)
2. labels include `release` or `publish` (release/publication truth category)

For those categories, closeout evidence must include:

1. GitHub Actions run URL (`https://github.com/<owner>/<repo>/actions/runs/<id>`)
2. run outcome (`success|failure|cancelled|skipped|timed_out|neutral|action_required`)

Use the helper before closeout:

```bash
scripts/swarm/check_ci_closeout_evidence.sh --bead <id> --run-url <url> --outcome <outcome>
```

Local-only evidence is sufficient only for beads that are not in the
CI-sensitive categories above. For local-only closeout on those beads, record a
clear reason and run:

```bash
scripts/swarm/check_ci_closeout_evidence.sh --bead <id> --local-only-reason "<reason>"
```

When you pick work:

1. Claim it with `br update <id> --status in_progress`.
2. Reserve the files you expect to touch through Agent Mail.
3. Announce the bead you are taking and the files you reserved.
4. Finish the bead truthfully. Satisfy the acceptance criteria, but do not stop
   mechanically if an honest closeout also requires a missing child bead,
   dependency correction, support-claim correction, or adjacent clearly-bounded
   follow-on work. Complete that work if it remains one truthful slice;
   otherwise bead it immediately and leave explicit notes.
5. Run the validation required by that bead's contract before closing it.
6. Record the exact validation command or suite result in the bead close reason
   or notes. Do not imply broader coverage than what actually ran.
7. If you change bead state or notes, run `br sync --flush-only`.

## Remote CI failure ownership protocol

When a GitHub Actions run fails during swarm execution, treat it as local
backlog intake work, not ambient noise:

1. first reporter wins ownership: the first worker who sees the failure claims
   one local bead for it (existing bead if present, otherwise one new repair
   bead) and posts an Agent Mail claim message
2. claim message must include: run id, run URL, workflow path/name, ref or tag,
   and the local bead id that owns remediation
3. no duplicate repair beads: if an open/in-progress bead already references the
   same run id + workflow identity, update that bead and reply in-thread instead
   of creating another bead
4. routing is explicit: send the claim/update to active workers via Agent Mail
   topic `ci-failure` so everyone sees owner + current status
5. if no owner claim exists within 15 minutes of first detection, any worker may
   claim and announce; do not leave failing runs unowned

## Fresh-Eyes Finding Intake Workflow (Testing-Only)

Use this path for onboarding and first-use rehearsals (`README.md`,
`docs/DEV_MACHINE_ONBOARDING.md`, and related devx probes).

1. Raise a repair bead immediately when a finding is reproducible and affects:
   - a documented onboarding step,
   - command correctness, or
   - queue/safety behavior.
2. Before creating a new repair bead, dedupe against open/in-progress beads by
   command/path + failure symptom. Reuse the existing bead when equivalent.
3. Record one explicit test-follow-up decision for each raised finding:
   - add a lower-layer test in the same repair bead when the failure is
     deterministic and repo-local, or
   - record a no-new-test decision (with reason + exact manual/int command) when
     the failure is external/environment-dependent and not truthfully unit/int-testable.
4. Post the finding claim/update in Agent Mail with topic `fresh-eyes` and
   include the rehearsal source doc/step plus linked bead id.

Minimum rehearsal evidence before closing a fresh-eyes workflow bead:

1. at least one linked repair bead id from the rehearsal, and
2. one linked test-follow-up decision (test added or explicit no-test decision).

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

For swarm execution workflow, do not use pull requests as the unit of agent work. This run is bead-first and repo-local:

1. do not create or manage GitHub PRs for your own implementation work
2. do not use `gh pr`, PR review flows, or PR-thread coordination as your normal working loop
3. do not assume a branch-per-agent workflow
4. work in the checked-out repo/worktree, keep changes local and explicit, and use beads + Agent Mail as the coordination surface
5. if a bead or closeout note mentions "open a PR", reinterpret that as a stale workflow assumption unless the current user instruction explicitly asks for PR handling

Roger the product may review GitHub PRs, but the swarm building Roger should not default to a PR-based delivery workflow.

If you need CPU-heavy cargo builds or tests and `rch` is available, prefer `rch exec -- <command>`. If `rch` is installed locally without a worker fleet, it may fail open to local execution; do not sit idle waiting for remote capacity that is not actually configured.

Re-read `AGENTS.md` after every compaction or long interruption so the
operating rules stay fresh. Reopen the canonical plan sections relevant to your
active bead before continuing, then re-check live queue truth with `br ready`
instead of resuming from memory alone. The durable state lives in beads and
Agent Mail, so use them continuously.

If you are in a persistent interactive tmux swarm session, do not stop after a single checkpoint. After each useful checkpoint, immediately:

1. re-check Agent Mail and acknowledge anything important
2. re-run `br ready`
3. verify the next candidate with `br show <id>`
4. claim the next unblocked bead you can usefully advance
5. if no bead is ready but the next slice is obvious, create or split the bead needed to continue safely

Only stop when the live queue is genuinely exhausted for you, you hit a real blocker that prevents more progress, or the user explicitly redirects you.

If you are running in a headless one-shot cycle instead, then stop cleanly after a durable checkpoint and let the outer launcher invoke you again.
