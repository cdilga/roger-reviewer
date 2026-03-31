---
name: Roger Extreme Software Optimization
description: Use when doing serious Roger Reviewer performance work such as startup latency, retrieval and reranking, indexing, storage refresh, TUI responsiveness, replay, normalization, or repair hot paths. Enforces baseline-first measurement, profile-first hotspot identification, one-lever diffs, equivalence oracles, isomorphism proof sketches, and re-measurement before claiming wins.
---

# Roger Extreme Software Optimization

This is a project skill for Claude Code. It packages Roger's proof-backed performance workflow into a loadable skill.

For the canonical repo contract version used by Codex and other harnesses, see `docs/skills/ROGER_EXTREME_SOFTWARE_OPTIMIZATION.md`.

## Use this skill when

Apply this skill for high-value performance work such as:
- review-session startup latency
- prompt-pass throughput
- retrieval or reranking latency
- TUI responsiveness
- indexing or storage refresh cost
- replay, normalization, or repair hot paths

Do not use it for speculative micro-optimizations or cleanup with no measured problem.

## Core doctrine

- No performance claim without measurement.
- No optimization proposal before profiling.
- No meaningful optimization without an equivalence oracle.
- No output-changing speedup disguised as basically the same.

## Required workflow

1. Baseline first.
2. Profile before proposing.
3. Write the hotspot statement.
4. Pull one lever only.
5. Define the equivalence oracle.
6. Write the isomorphism proof sketch.
7. Re-measure after the change.
8. Keep rollback obvious.

## Baseline checklist

At minimum capture:
- exact workload
- exact command or invocation path
- p50, p95, and p99 latency where applicable
- throughput where applicable
- peak memory or another explicit memory metric
- relevant environment details

If the workload is only a proxy, say so explicitly.

## Good optimization levers

Examples that often fit this skill well:
- move repeated pure work out of an inner loop
- avoid hydrating fields not required by the chosen output schema
- reuse prepared statements
- add bounded caching around a verified pure function
- use deterministic parallelization with a stable merge proof
- stream work to reduce peak RSS while preserving ordering invariants

## High-risk levers

These require extra scrutiny or explicit gating:
- approximate search under an exactness label
- floating-point reordering that may drift scores or rank order
- concurrency that changes stable ordering
- cache layers that can leak stale results across scopes
- benchmark-only wins that do not help real Roger workflows

## Reporting template

When this skill is active, structure your report like this:
- Baseline
- Evidence
- Change
- Oracle
- Proof sketch
- Results
- Decision

## Roger-specific constraints

Performance work must preserve:
- review-safe behavior
- durable findings identity
- approval gating semantics
- scope boundaries
- replayability and auditability
- honest degraded-mode behavior

A speedup that weakens any of those without explicit product approval is not a real win.

## Anti-patterns

Do not:
- propose optimizations before measuring
- optimize the thing that merely looks suspicious
- bundle multiple independent levers in one diff
- hide output changes inside a perf patch
- call a change successful because it feels cleaner

## Minimal success condition

A reviewer should be able to answer:
- what exact workload was measured?
- what was the real hotspot?
- what single lever changed?
- how do we know outputs did not change?
- what was the measured win?
- how do we roll it back if needed?

If those answers are missing, this skill was not applied correctly.
