# Roger Reviewer Testing Doctrine

Status: operator-facing testing doctrine and entrypoint to the validation
contract set.

This document is the operator-facing entrypoint for Roger's testing posture.
It summarizes how Roger turns support claims into runnable proof, and it points
to the implementation-facing contracts that define suites, fixtures, artifacts,
execution policies in detail. It also treats Roger's user-facing persona and
flow artifacts as first-class inputs to validation design rather than as loose
planning notes.

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
4. `docs/PERSONA_JOURNEYS_AND_CHAOS_RECOVERY.md`
5. `docs/REVIEW_FLOW_MATRIX.md`
6. `docs/VALIDATION_INVARIANT_MATRIX.md`
7. `docs/TEST_HARNESS_GUIDELINES.md`
8. `docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`
9. `docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md`
10. `docs/VALIDATION_CI_TIERS_AND_ENTRYPOINTS.md`
11. `docs/RELEASE_AND_TEST_MATRIX.md`
12. `docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md`

If these conflict, the canonical plan and `AGENTS.md` win.

## Core Philosophy

- keep exactly three validation lanes: `unit`, `integration`, `e2e`
- push confidence down the stack first
- keep the heavyweight E2E catalog small, explicit, and capped at six major
  product journeys in `0.1.x`
- treat persona scenario ids and flow families as first-class proof-shaping
  artifacts, not as optional narrative garnish
- use invariant ownership to defend critical truths
- prefer deterministic fixtures and Roger-owned doubles over ambient
  environments
- forbid success-only fantasy doubles
- preserve replayable failure artifacts when diagnosis materially benefits
- keep release promises narrower than the widest imaginable test matrix

## Heavyweight E2E Posture

Roger no longer treats one giant E2E as if it proves the whole product.
Instead, `0.1.x` carries a six-slot budget for the main contract-shaped
journeys Roger actually claims:

- `E2E-01` core review happy path
- `E2E-02` cross-surface review continuity with recall
- `E2E-03` TUI-first review with memory-assisted triage
- `E2E-04` refresh and draft reconciliation after new commits
- `E2E-05` browser setup and first PR-page launch
- `E2E-06` bare-harness dropout and return continuity

Rules:

- these are separate proof units for diagnosability, ownership, and
  parallelism, not ingredients for one omnibus suite
- only journeys that have executable suites and real runs count as functional
  coverage
- budget approval alone is not proof
- anything beyond these six major journeys must justify why a cheaper unit,
  integration, acceptance, or release-smoke shape is insufficient

Current repo truth:

- `E2E-01` is the first implemented executable heavyweight E2E
- `E2E-02` through `E2E-06` are budget-approved scenario slots until their
  suites land and run
- the machine-readable budget now carries persona ids, flow ids, invariant ids,
  current executable suite ids, current cheaper-suite owners, and follow-on
  bead ids so this mapping is derivable without re-reading every matrix doc

Current ownership map:

| E2E | Persona anchors | Invariant anchors | Executable proof today | Current cheaper proof owners | Missing executable owner |
|-----|-----------------|-------------------|------------------------|------------------------------|--------------------------|
| `E2E-01` | `PJ-03A`, `PJ-05A` | `INV-HARNESS-002`, `INV-POST-001` | `e2e_core_review_happy_path` | `int_harness_opencode_resume`, `int_github_outbound_audit`, `int_github_posting_safety_recovery` still own malformed/degraded/post-recovery truth | none |
| `E2E-02` | `PJ-02A`, `PJ-02D`, `PJ-04A`, `PJ-04B` | `INV-SESSION-002`, `INV-CONTEXT-001`, `INV-SEARCH-003`, `INV-SEARCH-004` | none | `int_cli_session_aware`, `accept_opencode_resume`, `int_search_prior_review_lookup` | `rr-6iah.1` |
| `E2E-03` | `PJ-03A`, `PJ-03C`, `PJ-04A` | `INV-TUI-001`, `INV-TUI-002`, `INV-SEARCH-003`, `INV-SEARCH-004` | none | `int_search_prior_review_lookup`, `int_cli_session_aware` | `rr-6iah.2` |
| `E2E-04` | `PJ-02D`, `PJ-04D`, `PJ-05B` | `INV-POST-002`, `INV-POST-003`, `INV-HARNESS-003` | none | `prop_refresh_identity_lifecycle`, `int_github_posting_safety_recovery` | `rr-6iah.3` |
| `E2E-05` | `PJ-01A`, `PJ-01B`, `PJ-01C` | `INV-BRIDGE-001`, `INV-BRIDGE-002`, `INV-SESSION-001` | none | `smoke_browser_launch_chrome`, `smoke_browser_launch_brave`, `smoke_browser_launch_edge`, `int_bridge_launch_only_no_status` | `rr-6iah.4` |
| `E2E-06` | `PJ-03C`, `PJ-04A`, `PJ-04B` | `INV-SESSION-002`, `INV-CONTEXT-001` | none | `accept_opencode_dropout_return`, `accept_opencode_resume`, `smoke_opencode_continuity`, `int_storage_opencode_dropout_return` | `rr-6iah.5` |

Official browser-lane posture for `E2E-05`:

- the executable automation owner should be a deterministic extension-loaded
  Chromium harness that proves setup, identity discovery, host registration,
  truthful doctor output, and first PR-page launch without depending on real
  GitHub mutation
- supported-browser `Chrome`/`Brave`/`Edge` runs remain named smoke or
  operator-stability evidence for public support claims
- a live sacrificial-PR rehearsal is still desirable, but it belongs in a
  later operator-stability or release-candidate lane rather than in the base
  deterministic E2E closeout

The detailed scenario contract lives in
[`docs/RELEASE_AND_TEST_MATRIX.md`](docs/RELEASE_AND_TEST_MATRIX.md), and the
machine-readable budget source of truth lives in
[`docs/AUTOMATED_E2E_BUDGET.json`](docs/AUTOMATED_E2E_BUDGET.json).

Entry, browser-resume, and local-first ownership for `PJ-01` through `PJ-03`
now has a dedicated guard:

- `cargo run -q -p roger-validation -- guard-persona-ownership tests/suites .beads/issues.jsonl`
- the guard checks that these non-recovery scenario cuts still point at both
  live suite metadata and explicit bead owners, so browser setup, browser
  resume, and terminal-first continuity truth do not drift back into prose-only
  matrices
- `guard-persona-recovery` remains the recovery-only guard for `PJ-04` through
  `PJ-06`

Recovery ownership for `PJ-04` through `PJ-06` now has a dedicated guard:

- `cargo run -q -p roger-validation -- guard-persona-recovery tests/suites .beads/issues.jsonl`
- the guard checks that recovery-heavy scenario cuts still point at both live
  suite metadata and explicit bead owners, so crash/restart/bootstrap truth does
  not drift back into prose-only matrices

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
- the repo-pinned Rust compiler channel is `nightly` via
  [`rust-toolchain.toml`](../rust-toolchain.toml); the workspace language
  edition remains `2024`
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

## User-Facing Journey Artifacts

Roger's testing doctrine does not begin with suites. It begins with the user
story Roger is claiming to support.

Two artifacts are first-class here:

- [`docs/PERSONA_JOURNEYS_AND_CHAOS_RECOVERY.md`](docs/PERSONA_JOURNEYS_AND_CHAOS_RECOVERY.md)
  for user-language persona families and stable scenario ids such as `PJ-03A`,
  `PJ-05D`, or `PJ-06C`
- [`docs/REVIEW_FLOW_MATRIX.md`](docs/REVIEW_FLOW_MATRIX.md) for surface and
  flow-family mapping such as `F01`, `F02.1`, or `F07`

Rules:

- a major testable claim should usually be traceable to at least one persona
  scenario id and one flow family
- crash, restart, corruption, stale-session, invalidation, and recovery
  behavior are part of the product journey, not test afterthoughts
- if a failure branch matters enough to affect trust, it deserves an explicit
  scenario id rather than being buried in a generic "error handling" bucket
- future E2E candidates should be described first in persona or flow language,
  then translated into suites and fixtures

Examples:

- `PJ-03A` plus `F01` is a clean local-first review journey
- `PJ-05D` plus `F07` and `F08` is a crash-after-approval or crash-during-post
  journey
- `PJ-06C` plus `F08` is a fail-closed local-damage or corruption journey

If Roger cannot explain a proposed test in this user-facing language, the test
is probably too implementation-shaped to anchor a support claim cleanly.

## Proof Ladder

Every critical product claim should be traceable through this ladder:

1. support claim or user-visible promise
2. persona scenario id and/or flow family
3. one or more invariant ids
4. owning bead or beads
5. owning suite families
6. required fixture families or corpora
7. execution policy where the proof must run
8. artifact outputs and proof manifests
9. release or docs wording that becomes safe

If any rung is missing, the claim is not yet fully defended.

## Bead Translation Contract

Testing posture must translate cleanly into backlog work. Every implementation
bead that changes behavior should do one of the following:

- cite one or more existing invariant ids from
  `docs/VALIDATION_INVARIANT_MATRIX.md`
- or create a new invariant row in that document as part of the same change

Each such bead should also name:

- the concrete user-visible or operator-visible promise being defended
- the relevant persona scenario id and flow family when the change affects a
  user journey or recovery story
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
- crash, restart, stale-session, and corruption recovery stories that are
  explicit rather than accidental
- support claims that match current evidence, not aspiration
- regressions that are diagnosable from artifacts rather than from guesswork

## Detailed Contracts

- [`docs/PERSONA_JOURNEYS_AND_CHAOS_RECOVERY.md`](docs/PERSONA_JOURNEYS_AND_CHAOS_RECOVERY.md)
- [`docs/REVIEW_FLOW_MATRIX.md`](docs/REVIEW_FLOW_MATRIX.md)
- [`docs/VALIDATION_INVARIANT_MATRIX.md`](docs/VALIDATION_INVARIANT_MATRIX.md)
- [`docs/TEST_HARNESS_GUIDELINES.md`](docs/TEST_HARNESS_GUIDELINES.md)
- [`docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`](docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md)
- [`docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md`](docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md)
- [`docs/VALIDATION_CI_TIERS_AND_ENTRYPOINTS.md`](docs/VALIDATION_CI_TIERS_AND_ENTRYPOINTS.md)
- [`docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md`](docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md)
- [`docs/RELEASE_AND_TEST_MATRIX.md`](docs/RELEASE_AND_TEST_MATRIX.md)
- [`docs/UPDATE_RELEASE_AND_TESTED_UPGRADE_CONTRACT.md`](docs/UPDATE_RELEASE_AND_TESTED_UPGRADE_CONTRACT.md)
