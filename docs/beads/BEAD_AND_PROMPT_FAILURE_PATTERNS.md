# Bead And Prompt Failure Patterns

This document captures a focused retrospective on why prior Roger swarm work
sometimes landed partial slices, closed too early, or drifted away from the
real product gap even when the contributing agents were behaving competently.

It exists to improve future bead shaping, worker-launch prompts, and
post-compaction re-anchoring.

Primary audience:

- agents shaping beads
- agents writing launcher prompts or marching orders
- humans reviewing whether the current frontier is truthfully split

Primary source basis:

- historical swarm prompts and worker histories inspected with CASS on
  2026-04-09
- current worker prompt/doctrine docs under `docs/swarm/`
- current execution-governance rules in
  `docs/EXECUTION_GOVERNANCE_AND_REPO_BOUNDARY.md`
- current execution-truth, provider-hardening, and support-claim rules in
  `docs/PLAN_FOR_ROGER_REVIEWER.md`

## Why This Exists

The main problem was not "agents are weak." The main problem was an interaction
between:

- prompts that over-rewarded literal bead closure
- beads that were sometimes underspecified or under-split
- queue truth and overlap issues that made narrow execution unsafe
- ambient repo wording that sometimes normalized stronger support claims than
  the live product truth justified

That combination produced a predictable failure mode: an agent would complete a
locally sensible slice, record validation honestly, and still leave the real
product promise only partially defended.

## Core Failure Pattern

The historical bad loop was roughly:

1. skim authority docs only enough to start quickly
2. claim a bead
3. implement exactly to the listed acceptance criteria
4. run a local validation layer
5. close and sync

That loop maximizes queue throughput when the beads are already excellent.
It fails when a bead is undersplit, a support claim spans multiple surfaces, or
the real product truth lives outside the narrow bead boundary.

## Prompt Patterns That Led Agents Astray

### 1. Narrow closure bias

Historical prompt wording repeatedly told workers to execute the bead exactly to
its acceptance criteria and no further.

That sounds disciplined, but in practice it trained agents to stop at the bead
edge even when they had already discovered:

- a missing child bead
- a support-claim mismatch
- a queue dependency inconsistency
- a docs or command-surface contradiction that made the closeout misleading

This wording should be treated as dangerous unless it is paired with an equally
strong rule that truthful closeout beats literal closure.

### 2. Startup context was too thin

Historical prompts often told workers to skim the canonical plan only enough to
confirm general direction.

That was efficient, but it made workers more likely to miss repo-wide truths
such as:

- local-core-first implementation order
- the difference between planned provider support and earned provider support
- the requirement that approval/posting flows be productized before support is
  claimed
- the difference between adapter presence and live user-facing support

If the repo is in a truth-sensitive implementation phase, startup prompts must
re-anchor on both `AGENTS.md` and the canonical plan.

### 3. Launcher authority drift

Some historical launcher prompts effectively assigned starting beads.

That pushed workers to trust launcher text more than live queue truth, which is
exactly backward for this repo. A launcher may give context, but the worker must
still confirm the live frontier with `br ready` and `br show <id>`.

### 4. Queue velocity was over-optimized

The prompts were tuned to keep agents moving. That was good in one sense, but it
also made them less likely to stop and say:

- this bead is too large
- this bead mixes multiple proof obligations
- this support claim crosses more surfaces than the bead admits
- this queue edge is wrong

Speed is useful only when the current frontier is truthful.

### 5. Post-compaction drift was real

Even a good worker can degrade after compaction or a long interruption.
Operational memory collapses toward the most recent narrow task fragment unless
the relaunch prompt explicitly re-establishes:

- authority order
- current project phase
- the canonical plan shape
- the repo's current truthfulness rules
- live queue truth

This needs to be deliberate, not optional.

## Bead Quality Patterns That Also Caused Failures

### 1. Beads treated as work buckets instead of proof units

Bad beads bundled too much of the following into one container:

- multiple ownership areas
- multiple support claims
- multiple validation layers
- multiple user-visible surfaces

Those beads invited agents to satisfy the easiest visible sub-part and stop.

### 2. Validation boundary was missing or too vague

When a bead did not say what promise it was defending and what validation layer
proved it, workers defaulted to the cheapest thing they could run, even when
the real claim needed a different layer or an explicit degraded-mode note.

### 3. Dependency truth was incomplete

At least some historical queue states allowed agents to discover "ready" work
whose real prerequisites were not actually finished. That is not an agent
failure. That is a graph-truth failure.

### 4. Overlap between sibling slices

At least some work collided because the bead graph and file ownership boundary
were not tight enough. Two agents can both look reasonable and still create
duplicative or conflicting product-shaping work if the slice boundary is weak.

## What Future Prompts Should Say Instead

Startup prompts should explicitly say:

1. read `AGENTS.md`
2. read the relevant parts of `docs/PLAN_FOR_ROGER_REVIEWER.md`
3. confirm current repo truth in code, tests, and live command surface
4. use `br ready` and `br show <id>` as the live queue source of truth
5. finish the bead truthfully, not mechanically

Post-compaction or long-interruption prompts should explicitly say:

1. re-read `AGENTS.md`
2. reopen the canonical plan sections relevant to the active bead
3. re-check live queue truth before continuing
4. do not trust stale launcher memory or stale local assumptions

Recommended startup wording:

> Read `AGENTS.md` first, then re-anchor on `docs/PLAN_FOR_ROGER_REVIEWER.md`
> before claiming work. Confirm the current implementation-stage rules,
> authority order, local-core-first direction, and support-claim truthfulness
> model. Then use `br ready` and `br show <id>` to choose work from live queue
> truth rather than launcher hints.

Recommended execution wording:

> Finish the bead truthfully. Satisfy the acceptance criteria, but do not stop
> mechanically if an honest closeout also requires a missing child bead,
> dependency correction, support-claim correction, or adjacent clearly-bounded
> follow-on work. Complete that work if it is still one truthful slice; otherwise
> bead it immediately and leave explicit notes.

Recommended post-compaction wording:

> After compaction or any long interruption, re-read `AGENTS.md`, reopen the
> canonical plan sections relevant to your active bead, and re-check `br ready`
> before taking the next action. Do not resume from memory alone.

## Bead-Shaping Rules Derived From These Failures

When shaping or splitting beads:

1. treat each leaf as one proof unit, not one theme bucket
2. keep each leaf to one main support claim and one main validation story
3. split any bead that spans multiple surfaces with separate truth obligations
4. make degraded-mode expectations explicit inside the bead
5. name the validation layer and exact closeout evidence up front
6. add dependency edges when a slice cannot be proven independently
7. create a docs-truthfulness child bead when documentation correction is useful
   but does not itself satisfy the implementation claim

## Closeout Questions Agents Should Ask

Before closing a bead, ask:

1. did I satisfy each acceptance criterion explicitly
2. did I run the validation layer that actually defends the promise
3. is any support claim still stronger than the exercised proof
4. did I discover a missing child bead, missing dependency, or queue lie
5. would another agent have to rediscover an obvious remaining gap because I
   chose to stop at the narrowest possible boundary

If the answer to any of the last three questions is yes, the bead is usually
not ready for a clean close without more shaping or explicit residual notes.

## Specific Anti-Patterns To Avoid

- "Implement exactly to acceptance criteria and no further" without an equal or
  stronger truthful-closeout rule
- prompts that tell agents to skim the plan only enough to start, then never
  re-anchor
- launcher text that feels like an assignment instead of a context hint
- closure notes that report validation honestly but still overstate the user
  promise
- beads that mix implementation, support-claim widening, and docs cleanup
  without naming those as separate proof obligations
- letting compaction reset an agent into narrow local execution mode without a
  fresh authority pass

## Relationship To Existing Governance Docs

This document does not replace:

- `AGENTS.md`
- `docs/PLAN_FOR_ROGER_REVIEWER.md`
- `docs/EXECUTION_GOVERNANCE_AND_REPO_BOUNDARY.md`

It explains a historical execution failure pattern that future prompts and bead
shaping should actively defend against.

If there is a conflict, those documents still win according to the repo
authority order.
