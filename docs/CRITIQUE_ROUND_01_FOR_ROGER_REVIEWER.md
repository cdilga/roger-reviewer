# Critique Round 01 for Roger Reviewer

This is the first explicit critique-and-integration artifact after the initial
plan draft.

## Highest-Value Revisions

### 1. Add explicit finding identity rules

Problem:

The first draft described refresh behavior, but it did not define how findings
remain stable across reruns. Without this, refresh would likely produce noisy
duplication and broken approval lineage.

Revision:

- add `FindingFingerprint`
- separate triage state from outbound posting state
- require refresh flows to classify findings as carried forward, superseded,
  resolved, or stale

### 2. Add integration contracts

Problem:

The first draft named several adapters, but it did not define the boundaries
that matter most: OpenCode session linkage, browser-to-local launch, and
outbound posting.

Revision:

- define those three contracts explicitly in the canonical plan
- keep them small and testable
- make them early spike candidates

### 3. Add architecture risk spikes before implementation spread

Problem:

The original rollout was good, but it still let package-level implementation
start before the riskiest unknowns had been reduced.

Revision:

- add a `Phase 0.5` risk-spike stage
- treat the browser bridge, OpenCode boundary, and artifact storage split as
  front-loaded validation problems

### 4. Clarify v1 extension expectations

Problem:

The original plan could be read as promising a strong live status indicator in
the extension without proving the required infrastructure.

Revision:

- explicitly prioritize launch/resume over live status fidelity in v1
- allow the extension status surface to degrade gracefully instead of forcing a
  hidden daemon

## Integrated Outcome

These revisions have been merged into:

- [`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md)
- [`BEAD_SEED_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/BEAD_SEED_FOR_ROGER_REVIEWER.md)

## Remaining Concerns for Round 02

- exact FrankenTUI runtime and packaging implications
- the specific OpenCode integration boundary available in practice
- what `FPs` and `SA` mean in the brain dump
- whether named-instance state sharing should be reduced further for v1
