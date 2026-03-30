# Roger Extreme Software Optimization

Status: reusable Roger skill.

Purpose:
Use this skill when Roger is doing serious performance work and we want the
Dicklesworthstone-style discipline found explicitly in
`PLAN_FOR_ADVANCED_OPTIMIZATIONS_ROUND_1__GPT.md`.

This skill is adapted to Roger's architecture, but its operating style is
anchored in a real written source rather than a vague aesthetic.

## Core doctrine

No performance claim without measurement.
No optimization proposal before profiling.
No merge of a meaningful optimization without an equivalence oracle.
No output-changing speedup disguised as "basically the same".

## When to use it

Use this skill for hot-path work such as:

- review-session startup latency
- prompt-pass throughput
- retrieval latency
- reranking latency
- finding normalization or repair throughput
- TUI responsiveness
- storage/index refresh cost
- replay or artifact load time

Do not use it for cosmetic cleanup or speculative micro-optimizations.

## Mandatory workflow

### 1. Baseline first

Before proposing any change, record a representative baseline.

At minimum capture:

- exact workload
- exact command or invocation path
- p50, p95, and p99 latency where applicable
- throughput where applicable
- peak memory or another explicit memory metric
- environment details that matter

If the workload is not representative, say so explicitly.

### 2. Profile before proposing

Collect evidence about where time or memory is actually going.

Use the most relevant tools available for the target path, for example:

- CPU profiling
- allocation profiling
- I/O profiling
- trace timing
- benchmark harnesses

Do not optimize the thing that merely looks suspicious.
Optimize the thing the profile identifies.

### 3. Write the hotspot statement

State the bottleneck in one sentence.

Examples:

- stored-field hydration dominates minimal robot output
- repeated normalization allocs dominate long-message canonicalization
- regex/DFA construction dominates repeated wildcard search
- worktree refresh spends most of its time reopening unchanged structures

### 4. Choose one lever only

Each diff should pull one main lever.

Good:

- reuse prepared statement
- cache compiled query object
- avoid hydrating unused fields
- move repeated pure computation out of inner loop
- add bounded memoization around a verified pure function

Bad:

- broad refactor plus caching plus data model rewrite plus concurrency change

Minimal diffs are easier to reason about, prove, and roll back.

### 5. Define the equivalence oracle

Before changing code, state how Roger will prove outputs did not change.

Examples:

- same ordered search hits for same corpus and query
- same ordered findings for same session and inputs
- same normalized artifact bytes for same source material
- same ranking and tie-breaking
- same repair decisions for same evidence ledger

If the optimization changes outputs, it is not isomorphic.
That does not make it forbidden, but it must be explicit and usually gated.

### 6. Write the isomorphism proof sketch

State why the optimization cannot change outputs.

Address at least:

- ordering
- tie-breaking
- floating-point behavior
- randomness or seeds, if any
- cache semantics
- concurrency effects, if any

If you cannot write a credible proof sketch, the change is not ready.

### 7. Re-measure after the change

Measure the same workload again.

Report:

- what improved
- what did not improve
- whether any regressions appeared
- whether the win is material enough to keep

### 8. Keep rollback obvious

Each performance diff should be easy to revert or gate.

Possible rollback patterns:

- narrow revertable commit
- feature flag
- env flag
- alternate code path retained briefly for bisecting

## Roger optimization contract template

```text
Operate under Roger's Extreme Software Optimization skill.

For this optimization task:
1. Define the representative workload.
2. Record baseline metrics first.
3. Profile before proposing changes.
4. State the real hotspot.
5. Propose one optimization lever only.
6. Define the equivalence oracle.
7. Write an isomorphism proof sketch covering ordering, tie-breaking,
   floating-point behavior, randomness, and concurrency where relevant.
8. Implement the smallest plausible diff.
9. Re-measure with the same workload.
10. Report gains, regressions, confidence level, and rollback path.

Do not make unmeasured performance claims.
Do not merge output-changing optimizations under an isomorphic label.
```

## Ranking opportunities

When several hotspots are available, rank them using:

`(Impact × Confidence) / Effort`

Interpretation:

- `Impact`: likely latency, throughput, or memory improvement on real Roger workloads
- `Confidence`: how strongly the profile and architecture suggest the lever will work
- `Effort`: implementation and validation cost, including regression risk

Prefer large, obvious wins over clever but low-signal ones.

## Roger-specific adaptation rules

Roger is review software, not a benchmark toy.
That means performance work must preserve:

- review-safe behavior
- durable findings identity
- approval gating semantics
- scope boundaries
- replayability and auditability
- honest degraded-mode behavior

A "fast" change that weakens any of those without explicit product approval is
not a win.

## Accepted patterns

These usually fit the contract well:

- moving repeated pure work out of an inner loop
- lazy materialization of fields not required by the chosen output schema
- prepared statements or bounded caches when semantics remain identical
- deterministic parallelization with proof-backed stable merge
- streaming to reduce peak RSS when ordering invariants are preserved

## High-risk patterns

These need extra scrutiny or explicit gating:

- approximate search introduced under an exactness label
- floating-point reordering that may drift scores or rank order
- concurrency that changes stable ordering
- cache layers that can leak stale results across scopes
- cross-session reuse that weakens provenance or replay fidelity
- benchmark-only wins that do not help real Roger workflows

## Report shape

Every optimization review should end with:

### Baseline
- workload
- metrics

### Evidence
- profile summary
- hotspot statement

### Change
- one-sentence description
- why it should help

### Oracle
- invariant or golden test

### Proof sketch
- why outputs should remain identical

### Results
- before/after metrics
- regressions if any

### Decision
- keep, revise, gate, or revert

## Anti-patterns

Do not do the following:

- propose optimizations before measuring
- benchmark a synthetic path and generalize recklessly
- hide output changes inside a perf patch
- accept speedups without a correctness story
- bundle multiple independent levers in one diff
- call a change successful because it feels cleaner

## Minimal acceptance test for using this skill

A reviewer should be able to answer:

- what exact workload was measured?
- what was the real hotspot?
- what single lever was changed?
- how do we know outputs did not change?
- what was the measured win?
- how do we roll it back if needed?

If those answers are not present, the optimization did not meet Roger's bar.
