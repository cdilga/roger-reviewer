# Test Execution Tiers And E2E Budget

This document turns the canonical plan's validation posture into an
implementation-facing support contract for Roger `0.1.0`.

It narrows, but does not override, the canonical plan in
[`docs/PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md)
and the release-facing validation matrix in
[`docs/RELEASE_AND_TEST_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/RELEASE_AND_TEST_MATRIX.md).

## Purpose

Roger should not drift into an expensive test story before the product exists.
The `0.1.0` validation contract needs four explicit execution tiers, one
blessed automated end-to-end test, and a small machine-readable budget file
that makes E2E growth visible instead of ambient.

Current repo truth as of 2026-04-07:

- `tests/suites/e2e_core_review_happy_path.toml` is the suite-metadata contract
  for the blessed E2E.
- `packages/cli/tests/e2e_core_review_happy_path.rs` is now the executable
  functional implementation for `e2e_core_review_happy_path`.
- Do not claim functional automated E2E coverage from lane policy or metadata
  alone; the suite must also be actually run in the relevant lane.
- The metadata file reserves the suite id and budget slot, but the E2E exists
  because the executable suite landed, not because the metadata exists.

## Test Tiers

### Tier 1: Fast local

Run on ordinary local development loops and before small planning-to-implementation
changes are merged into implementation branches.

Required posture:

- target seconds-to-low-minutes runtime on a developer machine
- cover pure domain logic, schema rules, serialization, prompt normalization,
  state transitions, and small adapter contracts
- prefer unit, parameterized, and narrow fixture-backed integration tests
- avoid network access, real browser launch, real provider launch, and
  heavyweight install flows

Examples:

- domain schema and fingerprint stability tests
- prompt repair and malformed-output normalization cases
- storage migration and artifact-addressing tests
- CLI parsing and robot-output shape tests with doubles

### Tier 2: PR validation

Run on every PR or equivalent merge gate.

Required posture:

- cover the full Tier 1 set plus the high-value integration families the plan
  already commits to
- stay deterministic and parallelizable in CI
- use Roger-owned doubles or canned corpora for GitHub mutation, provider repair
  cases, and bridge-envelope contract coverage

Required families:

- storage and migration integration tests
- prompt pipeline plus malformed or partial provider-output corpora
- harness-adapter tests with doubles and bounded acceptance hooks
- CLI session-binding and robot-output tests
- TUI controller tests against fake runtime services
- Native Messaging envelope and host-mode contract tests
- Native Messaging host-runtime round-trip tests against the registered `rr`
  binary
- GitHub adapter tests with Roger-owned doubles
- search and rebuild tests against seeded local fixtures

### Tier 3: Gated or nightly validation

Run on a gated branch, release candidate, nightly cadence, or another explicit
high-signal lane. This is the first tier allowed to include heavyweight
cross-boundary validation.

Required posture:

- include the one blessed automated E2E once implemented
- include provider acceptance suites and bridge/install smoke that are too slow
  or environment-sensitive for every PR
- keep the lane small enough that failures are actionable rather than noisy

Required contents for `0.1.0`:

- `E2E-01` core review happy path as an executable suite that is actually run;
  metadata or budget registration alone does not satisfy this
- OpenCode acceptance suite
- Gemini bounded acceptance suite
- browser bridge smoke for the serious v1 bridge path
- `SMOKE-BRIDGE-CHROME-01` for Chrome PR-page launch smoke
- `SMOKE-BRIDGE-BRAVE-01` for Brave PR-page launch smoke
- `SMOKE-BRIDGE-EDGE-01` for the Edge browser-launch edge scenario with
  fixture-backed transcript ownership
- same-PR routing or worktree smoke only where lower-level integration coverage
  leaves a real gap

### Tier 4: Release validation

Run for tagged releases or release candidates that may become a shipped build.

Required posture:

- validate artifact integrity and packaging claims in addition to product
  behavior
- cover the blessed manual smoke matrix from the release/test matrix
- fail closed on checksum, manifest, or missing-artifact mismatches

Required contents:

- artifact checksum verification
- per-target packaging smoke for shipped artifact classes
- manual smoke on the blessed OS or browser paths
- confirmation that the shipped support claims still match what was actually
  built

## Automated E2E Budget

Roger `0.1.0` carries one blessed automated heavyweight E2E:

- `E2E-01`: core review happy path

Current implementation status:

- implemented as `packages/cli/tests/e2e_core_review_happy_path.rs`
- registered in budget and suite metadata
- still requires a real run before any specific lane can claim that coverage

That test protects a product-defining promise across several boundaries:

- session-aware launch or resume
- real supported-provider execution on the blessed path
- findings normalization into Roger state
- local draft review or approval flow
- GitHub posting through a Roger-owned double
- persisted audit chain

Everything else should default downward into a cheaper tier unless a lower-cost
test shape leaves a meaningful product-risk gap.

Execution rule:

- do not close an implementation bead for `E2E-01` by narrowing docs,
  registering metadata, or wiring lane policy alone
- if docs need correction while the suite is still missing, that is a separate
  docs-truthfulness task and the implementation bead remains open

The suite was exercised locally on 2026-04-07 with:

```sh
cargo test -p roger-cli --test e2e_core_review_happy_path -- --nocapture
```

### Supported-Browser Launch Classification

`SMOKE-BRIDGE-CHROME-01`, `SMOKE-BRIDGE-BRAVE-01`, and
`SMOKE-BRIDGE-EDGE-01` are named suite ids for supported-browser launch smoke.

Bridge smoke rule:

- a supported-browser bridge smoke is not complete unless it proves both:
  1. registration/install truth (`rr extension doctor` or equivalent), and
  2. host-runtime truth (the registered `rr` binary responds to a Native
     Messaging request without hanging)
They are explicitly smoke/acceptance lane guards, not heavyweight E2Es.

Chrome/Brave-specific execution is required when:

- Native Messaging host registration logic changes
- launch-intent payload or bridge envelope handling changes
- extension packaging/release lane changes could affect Chrome/Brave launch
  behavior
- Chrome or Brave launch support is being newly claimed or re-claimed in
  release notes

Shared-source coverage without fresh Chrome/Brave runs is sufficient only when:

- the change is docs-only or limited to shared UI styling with no launch/bridge
  envelope changes
- `int_bridge_*` suites remain green and the latest passing browser smoke
  artifacts remain representative

`SMOKE-BRIDGE-EDGE-01` is the named suite id for the Edge browser-launch edge
scenario. It is explicitly a smoke/acceptance lane guard, not a second
heavyweight E2E.

Edge-specific execution is required when:

- Native Messaging host registration logic changes
- launch-intent payload or bridge envelope handling changes
- extension packaging/release lane changes could affect Edge launch behavior
- Windows Edge support is being newly claimed or re-claimed in release notes

Shared-source coverage without a fresh Edge run is sufficient only when:

- the change is docs-only or limited to shared UI styling with no launch/bridge
  envelope changes
- `int_bridge_*` suites remain green and the latest passing
  `SMOKE-BRIDGE-EDGE-01` artifact remains representative

## Budget File

The machine-readable budget for automated heavyweight E2Es lives in
[`docs/AUTOMATED_E2E_BUDGET.json`](/Users/cdilga/Documents/dev/roger-reviewer/docs/AUTOMATED_E2E_BUDGET.json).

Contract:

- `blessed_automated_e2e_budget` is the current allowed baseline
- `blessed_e2e_ids` lists approved heavyweight E2Es by stable id
- `warning_mode` controls the initial feedback phase
- `future_ci_mode` defines the stricter gate Roger should enable once the
  workflow is proven
- each new heavyweight E2E must add a justification entry instead of quietly
  raising the count

## Growth Rules

When a change increases the automated heavyweight E2E count above the budget
baseline, Roger should require an explicit written defense.

Minimum defense questions:

- what product-defining promise does this E2E protect?
- why can that promise not be defended by a unit, parameterized, or narrow
  integration test?
- which lower-cost alternatives were considered and rejected?
- what real boundary combination would remain unprotected without this E2E?

Acceptable justification examples:

- the behavior spans provider execution, Roger persistence, approval flow, and
  a bridge or packaging boundary that lower-level doubles cannot exercise
- the failure mode is only visible when several real envelopes interact and the
  integration family cannot reproduce it faithfully

Unacceptable justification examples:

- "the E2E was faster to write"
- "the unit-test seam was annoying"
- "we wanted extra confidence" without naming the irreducible boundary

## Warning And Failure Policy

Roger should enforce the budget in two stages.

### Stage 1: warning

Local validation and CI emit a visible warning when the budget count rises above
the recorded baseline or when a new heavyweight E2E lacks a justification
record.

The warning should be direct:

- it should ask whether the author is taking the lazy path to another expensive
  E2E
- it should point to the cheaper preferred alternatives
- it should name the exact test ids that triggered the warning

### Stage 2: failure

Once the warning workflow is proven, CI should fail either of these cases:

- the heavyweight E2E count exceeds `blessed_automated_e2e_count` without a
  matching approved justification entry
- a new heavyweight E2E id appears without budget-file updates

## Test Classification Rules

Use these rules before promoting any test into the heavyweight E2E lane.

- If a behavior can be defended with pure inputs and outputs, keep it in Tier 1.
- If a behavior crosses one or two Roger-owned boundaries with good doubles,
  keep it in Tier 2.
- If a behavior needs a real provider, real bridge, or real packaging surface
  but is not a shipped-release claim, place it in Tier 3.
- If a behavior validates shipped artifacts, support claims, or manual release
  promises, place it in Tier 4.

## `0.1.0` Mapping Summary

| Concern | Default tier | Notes |
| --- | --- | --- |
| Domain logic, findings, schema, prompt repair | Tier 1 | Prefer parameterized tests over fixtures where possible |
| Storage, migrations, CLI robot shapes, TUI controller, adapter contracts | Tier 2 | Deterministic CI coverage |
| Core review happy path, provider acceptance, bridge smoke | Tier 3 | Keep small and intentionally curated |
| Packaging, checksums, install truthfulness, release smoke | Tier 4 | Release-candidate or tagged-release gate |

## Review Rule For Future Beads

Any bead that proposes a new automated E2E should name:

- the candidate test id
- the tier it belongs in
- the cheaper test shapes that were rejected
- the justification record it will add to the budget file if it exceeds the
  current blessed count
