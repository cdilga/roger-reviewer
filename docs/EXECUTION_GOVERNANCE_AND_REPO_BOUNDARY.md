# Execution Governance And Repo Boundary

This document tightens how Roger moves from plan to delivered product. The
architecture plan is not the main gap anymore. The main gap is execution
governance: how beads are split, how they are closed, how support claims are
earned, and what belongs in this repo versus the operator's external toolkit.

## Why This Exists

Roger's planning docs are strong on architecture, constraints, and validation
philosophy. The delivery system has been weaker:

- beads have sometimes been treated as work buckets instead of proof units
- support claims have drifted ahead of live command surfaces or live validation
- swarm/operator tooling has taken up too much repo attention relative to
  product code
- repo state, bead state, and current docs have not always been reconciled
  tightly enough

This contract exists to correct those failure modes.

## 1. Beads Must Be Proof Units

A bead is one independently provable slice of product progress.

Good leaf beads usually have:

- one clear ownership area
- one acceptance boundary
- one primary validation story
- minimal overlap with sibling leaves

Bad leaf beads usually contain:

- multiple unrelated file clusters
- multiple distinct support claims
- multiple validation layers
- enough ambiguity that two agents would need constant negotiation

Rules:

- split oversized beads before or during implementation
- keep parent beads mostly as integration checkpoints
- when `br ready` is thin but adjacent safe work is obvious, shape the graph
  instead of declaring the repo done

## 2. Closure Requires Evidence

Code landing is not enough to close a bead.

Every implementation closeout should record:

1. which acceptance criteria were satisfied
2. which validation actually ran
3. the real outcome
4. any residual gap, degraded mode, or deferred edge

Do not close a bead when:

- behavior was inferred from code instead of exercised
- no validation ran for implementation work
- docs-only, metadata-only, or lane-wiring-only edits were used to stand in for
  missing implementation or missing runnable validation
- obvious remaining sub-work exists but is untracked
- the close reason would overstate support, coverage, or completeness

If the bead is not honestly closeable:

- keep it open
- add a note
- create child beads for the remaining separable work
- if documentation needs correction before implementation lands, track that as
  a separate docs-truthfulness slice rather than using it to satisfy the
  implementation bead

## 3. Support Claims Must Be Earned

Support claims are product commitments.

They must be backed by:

- live command or UI surface
- matching docs
- matching validation

Rules:

- planned capability is not shipped capability
- adapter coverage is not the same thing as live user-facing support
- bounded or degraded support must be labeled as bounded or degraded
- setup/install claims must match a fresh-user path that was actually exercised
- E2E claims require executable suites that exist and were run
- suite metadata, budget slots, CI lane references, and contract docs are not
  themselves implementation evidence

When live probing contradicts docs or beads, reality wins.

## 4. Repo Boundary Must Stay Clean

This repo should primarily contain:

- Roger product code
- Roger product tests and fixtures
- Roger release/build scripts
- Roger policy and implementation docs
- minimal repo-local config required to point external tools at Roger

This repo should usually not contain:

- personal swarm control planes
- personal Agent Mail dashboards
- machine-local observer/bootstrap helpers
- ad hoc operator repro tooling that is not part of Roger's shipped surface

Preferred model:

- move operator tooling into an external ops toolkit
- leave a small repo-local config or thin wrapper only when needed
- keep AGENTS/product docs focused on Roger, not on a personal operating system

## Practical Operating Rules

Before starting work:

- read `AGENTS.md`
- read the relevant bead
- confirm current repo truth in code, tests, and live command surface

Before closing work:

- check acceptance criteria one by one
- run the named validation
- update support docs if the claim changed
- sync beads truthfully

When the graph is the bottleneck:

- create missing child beads
- add missing dependency edges
- prefer widening truthful parallel leaves over crowding a broad parent

## Comparison Standard

High-quality Dicklesworthstone repos tend to share a few traits:

- strong truthfulness about current state versus future ambition
- explicit guarantees and explicit non-goals
- clear "problem / solution / current API truth" framing
- strong emphasis on determinism, correctness, and bounded behavior

Roger should match that standard in its agent guidance:

- concise
- direct
- explicit about current truth
- explicit about what is not yet true
- strict about proof before claims
