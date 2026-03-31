# Swarm Bead Operability Remediation Plan

As of 2026-03-31, cass analysis of recent Roger Reviewer swarm sessions shows a
recurring efficiency problem around bead pickup and queue trust rather than a
pure implementation-capacity problem.

## Scope

This plan is intentionally narrow. It covers the swarm control plane around
beads:

- how workers discover ready work
- how much setup and retry work they perform before starting
- how trustworthy the queue and bead state feel during parallel execution
- how often the swarm burns cycles on graph hygiene rather than Roger delivery

It does not change Roger product architecture or acceptance scope.

## Observed failure modes

1. `br` path and tool-health trust are brittle enough that workers and repo
   automation fall back to ad hoc recovery steps.
2. Queue truth is not trusted enough; workers repeatedly re-check `br ready`,
   compare it with `bv`, and manually audit whether the frontier is real.
3. Lock contention is common enough that retry behavior is baked into the swarm
   launcher and supervisor prompts.
4. Workers keep rediscovering missing leaf beads, misplaced dependencies, or
   prematurely closed beads during execution.
5. The swarm pays a large control-plane tax through repeated AGENTS, marching
   orders, Agent Mail, file reservation, and queue-sanity instructions.
6. Meta-maintenance beads and implementation beads compete in the same operator
   attention loop, which makes the frontier feel noisy and unstable.

## Efficiency losses

- before-work tax: `br ready`, `br show`, inbox check, reservation check,
  queue sanity check
- retry tax: broken `br` path, lock contention, derived-state ambiguity
- replanning tax: creating or splitting missing beads mid-run
- context tax: repeated long-form operational instructions in every worker loop
- integrity tax: graph or doctor anomalies consuming ready slots and human
  intervention

## Remediation workstreams

### 1. Restore baseline `br` trust

Fix the default pinned `br` command path and make swarm preflight fail loudly
when the pinned path, health assumptions, or export assumptions are broken.

### 2. Make the queue cheaper to trust

Add a swarm-oriented preflight or snapshot path that gives operators and workers
one authoritative queue-health check before a large run, instead of forcing each
worker to rediscover the same state independently.

### 3. Reduce repeated control-plane prompt cost

Split swarm instructions into:

- a short worker prompt with only the rules that change behavior immediately
- authoritative linked docs for the longer operational doctrine

The worker loop should not need to re-ingest the full operating manual every
cycle.

### 4. Front-load bead-batch hygiene

Before a large swarm launch, validate that the next parallelizable bead batch is
already split, dependency-clean, and acceptance-clear. Missing leaves should be
discovered in operator preflight, not by six workers in parallel.

### 5. Separate maintenance turbulence from implementation flow

Document and script a clear maintenance lane for bead-health repairs and other
meta-operability tasks so implementation workers do not constantly bounce
between product work and tracker repair.

## Success criteria

This remediation is successful when:

- a fresh swarm operator can determine whether the queue is swarm-ready without
  manual SQLite inspection or ad hoc backup binaries
- workers spend materially less prompt space and iteration time on startup
  coordination
- obvious missing beads are created before the swarm run rather than during it
- queue-empty states are rarer and more trustworthy
- maintenance-path bead issues stop dominating the swarm's first ready choices
