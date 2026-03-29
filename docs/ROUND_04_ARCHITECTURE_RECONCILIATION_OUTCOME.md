# Round 04 Architecture Reconciliation Outcome

Status: completed Round 04 closeout artifact. This document records the
reconciliation outcome; the canonical spec remains
[`AGENTS.md`](/Users/cdilga/Documents/dev/roger-reviewer/AGENTS.md) plus
[`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md).

Date: 2026-03-29

## Purpose

Round 04 existed to finish architecture reconciliation after the critique and
ADR work from Rounds 01 through 03. Its job was not to restart ideation. Its
job was to:

- align the canonical docs with the accepted ADR set
- remove stale ambiguity that kept broad questions sounding bigger than they
  really were
- turn ambient uncertainty into bounded follow-up work
- unblock the next planning phase: bead sync, bead polishing, and readiness
  review

## Source Set Reconciled

- [`AGENTS.md`](/Users/cdilga/Documents/dev/roger-reviewer/AGENTS.md)
- [`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md)
- [`ROUND_04_ARCHITECTURE_RECONCILIATION_BRIEF.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/ROUND_04_ARCHITECTURE_RECONCILIATION_BRIEF.md)
- [`DATA_MODEL_AND_STORAGE_CONTRACT.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/DATA_MODEL_AND_STORAGE_CONTRACT.md)
- [`RELEASE_AND_TEST_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/RELEASE_AND_TEST_MATRIX.md)
- [`REVIEW_FLOW_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/REVIEW_FLOW_MATRIX.md)
- [`docs/adr/README.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/adr/README.md)
- accepted ADRs `001` through `008`

## What Round 04 Closed

### 1. Broad architecture direction is reconciled

Roger is now clearly a Rust-first, local-first, daemonless-in-steady-state
system with:

- in-process TUI plus app-core in `0.1.x`
- one canonical Roger store per profile by default
- Native Messaging as the serious v1 browser bridge
- OpenCode as the primary harness and Gemini as the bounded secondary harness

These are no longer open ideology debates.

### 2. Config topology and prompt ingress are no longer ambient questions

Round 04 closed the missing owner-policy defaults:

- `repo` is the default scope
- `project` is an explicit Roger-managed allowlist overlay
- future `org` overlays are opt-in only
- web-path prompt ingress stays bounded to preset selection plus a short
  explicit objective and related launch selectors
- per-review overrides may not silently relax approval, mutation, or trust
  policy
- canonical-by-default doc classes are conservative and explicit

### 3. Support posture is now truthful beta plus explicit ambition

Roger should make truthful beta claims today while still committing to a broader
eventual provider/browser/OS support track. Current docs should distinguish:

- blessed and acceptance-tested paths
- bounded or partial paths
- future support-track targets

This closes the gap between "ultra-minimal beta" and "we still intend to push
far beyond the initial matrix."

### 4. Dependency posture is explicitly agent-tier

Round 04 locks a stronger engineering stance:

- every significant dependency must be justified in writing
- agents are expected to challenge convenience dependencies during review
- thin Roger-owned adapters and bespoke glue are often preferable to broad
  dependency trees
- alien-tier capability is a reason to demand stricter architecture, not an
  excuse for looser architecture

Roger should act like a swarm capable of manufacturing better artefacts,
stronger critiques, sharper transfer audits, and leaner ownership boundaries
than a normal human-default workflow would sustain.

## What Still Remains Open After Round 04

These are still real questions, but they are now bounded implementation-shaping
questions rather than unresolved product philosophy:

- default queue limits, cancellation rules, and wake cadence inside the
  accepted in-process runtime
- exact release/devops automation for binaries plus extension artefacts
- named-instance/worktree preflight specifics and UX details
- attention-event mirroring across surfaces
- first-class `rr --robot` command surface and stable output schemas
- semantic packaging details, merged-outcome storage shape, and TOON viability

These should be handled by bead refinement, scoped follow-up decisions, and
readiness review rather than another broad architecture rewrite.

## Bead-Level Outcome

- `rr-012` should close: the architecture spikes were run, the ADRs exist, and
  the formerly implicit assumptions are now explicit in canonical docs.
- `rr-015` remains open, but it is now a bounded harness/session contract task
  rather than a vague architecture-spike proxy.
- `rr-021` remains open, but the bridge family and dependency posture are now
  settled enough that the remaining work is package, adapter, and validation
  execution.
- `rr-023` remains open, but the one-store-per-profile baseline and
  no-DB-copy-sync default are already settled.

## Exit Criteria Assessment

Round 04 is considered complete because:

- the canonical docs and accepted ADRs now tell the same broad story
- the previously open config and prompt-ingress policy has been frozen
- stale ambiguity has been reduced to bounded follow-up questions
- the planning set is ready to move to live bead sync, bead polishing, and then
  readiness review

## Next Step

The next planning step is not another broad critique round.

The next sequence is:

1. sync the live bead graph to the current
   [`BEAD_SEED_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/BEAD_SEED_FOR_ROGER_REVIEWER.md)
2. run bead polishing on the live graph
3. run and record a readiness review
4. begin implementation only after that gate passes
