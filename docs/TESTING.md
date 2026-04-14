# Roger Reviewer Testing Doctrine

Status: operator-facing testing doctrine and entrypoint to the validation
contract set.

This document is the operator-facing entrypoint for Roger's testing posture.
It summarizes how Roger turns support claims into runnable proof, and it points
to the implementation-facing contracts that define suites, fixtures, artifacts,
and execution policies in detail.

This file does not replace the detailed validation contracts under `docs/`.
It tells humans and agents how those contracts fit together.

## Why This Exists

Roger is a mutation-sensitive local review product. "The tests are green" is
not enough. Users need something that works on the blessed paths, fails closed
on dangerous paths, and degrades honestly when Roger cannot safely continue.

Roger therefore treats testing as a truth-maintenance system:

- support claims must map to named proofs
- approval-sensitive and posting-sensitive failures must fail closed
- degraded modes must be visible and intentionally tested
- critical truths must be owned explicitly rather than implied by broad
  scenario coverage

Roger does not promise bug-free software. It does promise that the blessed
support claims should be narrow, explicit, evidence-backed, and replayable.

## Authority

Use the testing docs in this order:

1. `AGENTS.md`
2. `docs/PLAN_FOR_ROGER_REVIEWER.md`
3. this document
4. `docs/VALIDATION_INVARIANT_MATRIX.md`
5. `docs/TEST_HARNESS_GUIDELINES.md`
6. `docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`
7. `docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md`
8. `docs/VALIDATION_CI_TIERS_AND_ENTRYPOINTS.md`
9. `docs/RELEASE_AND_TEST_MATRIX.md`
10. `docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md`

If these conflict, the canonical plan and `AGENTS.md` win.

## Core Philosophy

- keep exactly three validation lanes: `unit`, `integration`, `e2e`
- push confidence down the stack first
- keep one blessed automated heavyweight E2E in `0.1.x`
- use invariant ownership to defend critical truths
- prefer deterministic fixtures and Roger-owned doubles over ambient
  environments
- forbid success-only fantasy doubles
- preserve replayable failure artifacts when diagnosis materially benefits
- keep release promises narrower than the widest imaginable test matrix

## Current Tooling Posture

Roger's doctrine is now stronger than its tooling standardization, and that is
intentional.

Current pinned posture:

- executable validation should continue to route through the canonical repo
  commands and execution policies documented in the validation docs
- suite metadata, fixtures, and artifact roots are the authoritative proof
  plumbing today
- release-critical claims should depend on named suites, artifacts, and proof
  manifests rather than on raw line-coverage percentages alone
- the Rust command baseline is:
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace --all-targets`
  - targeted replay: `cargo test -p <package> --test <suite> -- --nocapture`
- the Rust tooling baseline is:
  - `proptest` for rule matrices and property-style coverage
  - `insta` for stable structural snapshots, especially TUI and controller state
  - `cargo llvm-cov` for coverage reporting and ratcheting

Rust-specific hardening tools that are approved but not required for every
change:

- `cargo fuzz` for parser, envelope, and structured-artifact surfaces
- `criterion` for performance contracts where the release docs explicitly make a
  latency or throughput promise
- `loom` only for narrow concurrency seams where scheduler interleaving or lock
  ordering must be proven rather than inferred

Not yet standardized:

- one blessed mutation-testing or fuzzing policy for the whole repo
- one global requirement for numeric coverage floors across all subsystems
- always-on use of `criterion`, `loom`, or `cargo fuzz` in ordinary PR gates
- `rch` or any other remote-execution wrapper as part of Roger's canonical
  command surface

Recommended future direction:

- use `cargo llvm-cov` as the coverage-reporting source of truth while keeping
  release truth tied to named invariants, suites, and artifacts rather than to
  percentages alone
- keep policy scans and matrix checks Roger-owned and lightweight rather than
  outsourcing support truth to a generic dashboard

## Invariants

An invariant is a product truth that must remain true across valid states or
state transitions, not just in one scripted path.

Examples:

- approval must bind to the exact draft payload and target
- target drift must invalidate prior approval automatically
- ambiguous session selection must fail closed instead of guessing
- partial findings salvage must preserve valid findings rather than discarding
  the whole pack
- bridge launch must never report fake Roger success
- published release assets must remain self-consistent across installer and
  updater surfaces
- blocked install/update paths must emit truthful recovery guidance for the
  actual operator context
- search must not silently widen scope or erase provenance

Roger's release-critical invariants live in
[`docs/VALIDATION_INVARIANT_MATRIX.md`](docs/VALIDATION_INVARIANT_MATRIX.md).

## Proof Ladder

Every critical product claim should be traceable through this ladder:

1. support claim or user-visible promise
2. one or more invariant ids
3. owning bead or beads
4. owning suite families
5. required fixture families or corpora
6. execution policy where the proof must run
7. artifact outputs and proof manifests
8. release or docs wording that becomes safe

If any rung is missing, the claim is not yet fully defended.

## Bead Translation Contract

Testing posture must translate cleanly into backlog work. Every implementation
bead that changes behavior should do one of the following:

- cite one or more existing invariant ids from
  `docs/VALIDATION_INVARIANT_MATRIX.md`
- or create a new invariant row in that document as part of the same change

Each such bead should also name:

- the concrete user-visible or operator-visible promise being defended
- the primary failure, degraded, invalidation, or recovery cases in scope
- the cheapest truthful proof lane
- the owning suite families
- the fixture families or deterministic corpora required
- the expected proof artifacts or manifests
- the execution policy that must run before the support claim is widened

If a bead touches several unrelated invariants or several independent proof
stories, it should be split unless it is intentionally an integration
checkpoint.

## Autonomous Gap Escalation

Agents are expected to notice test gaps and shape follow-on work instead of
walking past them.

Examples that justify a new testing bead or research bead:

- a release-critical invariant has no obvious owning suite
- a support claim is defended only by happy-path coverage
- a boundary has nominal-path tests but weak degraded, invalidation, or
  recovery coverage
- coverage appears suspiciously thin in a failure-prone area and the current
  bead does not have room to fix it honestly
- a test bead is too underspecified to implement faithfully without revisiting
  the product intent

Important exception:

- agents do not need to reread the full plan for every testing task
- they do need to reread the canonical plan, this doctrine, and the relevant
  support contracts when a test bead is underspecified relative to UX behavior,
  support claims, fail-closed behavior, or recovery expectations

In that case, a small research or plan-clarification bead is preferable to
guessing.

## No Fantasy Doubles

Roger allows doubles. Roger does not allow doubles that only model the happy
path or silently remove production failure modes.

Allowed:

- Roger-owned doubles that implement a real adapter contract
- fake runtime services for TUI controller tests
- canned provider corpora that include malformed, partial, raw-only, and
  degraded outcomes
- deterministic bridge transcripts and install-recovery fixtures

Disallowed:

- doubles that cannot express real invalidation, failure, or degraded states
- doubles that bypass approval or posting safety semantics
- "mock success" paths used as substitutes for real boundary coverage

## Runnable Proof And Derived Evidence

Roger's evidence should be machine-derivable, not comment-derived.

The detailed contract lives in
[`docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md`](docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md),
but the practical rule is:

- suites emit metadata envelopes naming flows, fixtures, and invariant ids
- integration and E2E suites preserve replayable failure artifacts where
  required
- release-critical suite families should publish stable `latest.json` and
  `latest_success.json` manifests once implemented so release and closeout tools
  can resolve current proof mechanically
- bead closeout should record the exact command or entrypoint and the resulting
  artifact or manifest path

If a proof cannot be replayed or mechanically located, it is weaker than Roger
should tolerate for a critical claim.

## What Users Can Expect

If Roger follows this doctrine, users should get:

- blessed workflows that are explicitly named and heavily defended
- failure states that stop safely rather than bluffing through dangerous paths
- degraded modes that are visible and auditable
- support claims that match current evidence, not aspiration
- regressions that are diagnosable from artifacts rather than from guesswork

## Detailed Contracts

- [`docs/VALIDATION_INVARIANT_MATRIX.md`](docs/VALIDATION_INVARIANT_MATRIX.md)
- [`docs/TEST_HARNESS_GUIDELINES.md`](docs/TEST_HARNESS_GUIDELINES.md)
- [`docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`](docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md)
- [`docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md`](docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md)
- [`docs/VALIDATION_CI_TIERS_AND_ENTRYPOINTS.md`](docs/VALIDATION_CI_TIERS_AND_ENTRYPOINTS.md)
- [`docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md`](docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md)
- [`docs/RELEASE_AND_TEST_MATRIX.md`](docs/RELEASE_AND_TEST_MATRIX.md)
- [`docs/UPDATE_RELEASE_AND_TESTED_UPGRADE_CONTRACT.md`](docs/UPDATE_RELEASE_AND_TESTED_UPGRADE_CONTRACT.md)
