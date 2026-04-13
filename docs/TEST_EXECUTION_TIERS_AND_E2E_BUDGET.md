# Test Execution Tiers And E2E Budget

This document turns the canonical plan's validation posture into an
implementation-facing support contract for Roger `0.1.0`.

It narrows, but does not override, the canonical plan in
[`docs/PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md)
and the release-facing validation matrix in
[`docs/RELEASE_AND_TEST_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/RELEASE_AND_TEST_MATRIX.md).

## Purpose

Roger should not drift into an expensive test story before the product exists.
The `0.1.0` validation contract needs three validation lanes, a small set of
execution policies that invoke those lanes, one blessed automated end-to-end
test, and a machine-readable budget file that makes E2E growth visible instead
of ambient.

Current repo truth as of 2026-04-07:

- `tests/suites/e2e_core_review_happy_path.toml` is the suite-metadata contract
  for the blessed E2E.
- `packages/cli/tests/e2e_core_review_happy_path.rs` is now the executable
  functional implementation for `e2e_core_review_happy_path`.
- Do not claim functional automated E2E coverage from lane policy or metadata
  alone; the suite must also be actually run in the relevant lane.
- The metadata file reserves the suite id and budget slot, but the E2E exists
  because the executable suite landed, not because the metadata exists.
- Historical metadata still includes labels such as `prop_*`, `accept_*`, and
  `smoke_*`; treat those as sub-kinds inside the three-lane model until the
  harness metadata is simplified.

## Validation Lanes

Roger recognizes only three validation lanes:

- `unit`
- `integration`
- `e2e`

### Lane 1: Unit

This lane covers ordinary local development loops and should be the dominant
source of confidence.

Required posture:

- target seconds-to-low-minutes runtime on a developer machine
- cover pure domain logic, schema rules, serialization, prompt normalization,
  state transitions, and small adapter contracts
- include parameterized and property-style suites directly in `unit`
- avoid network access, real browser launch, real provider launch, and
  heavyweight install flows

Examples:

- domain schema and fingerprint stability tests
- prompt repair and malformed-output normalization cases
- storage migration and artifact-addressing tests
- CLI parsing and robot-output shape tests with doubles

### Lane 2: Integration

This lane covers deterministic boundary tests and provider truthfulness without
promoting every expensive flow into E2E.

Required posture:

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
- provider-acceptance coverage
- transaction and crash-recovery coverage
- install/setup and bridge truthfulness checks unless the defended promise is a
  full product journey

### Lane 3: End-to-End

This lane is reserved for a very small number of product-defining journeys that
cross several real Roger surfaces.

Required posture:

- include the one blessed automated E2E once implemented
- admit additional E2Es only when lower-layer coverage leaves a meaningful gap
- keep the lane small enough that failures are actionable rather than noisy

Required contents for `0.1.0`:

- `E2E-01` core review happy path as an executable suite that is actually run;
  metadata or budget registration alone does not satisfy this
- any future E2E only after explicit justification and budget approval

## Execution Policies

Execution policies decide when and how the three lanes run. They are not lanes.

### `local-bead`

Run before committing a bead.

Required posture:

- target a small truthful slice of `unit` and, when needed, `integration`
- prefer the cheapest proof that defends the bead honestly
- keep the local loop fast enough to be habitual
- record the exact command and resulting artifact or proof-manifest path when
  the bead defends a release-critical invariant

### CI reproduction

Run on pull requests or equivalent merge gates.

Required posture:

- rerun deterministic `unit` and `integration` coverage outside the author's
  machine
- avoid requiring licensed environments or brittle real-surface dependencies by
  default
- keep proof outputs machine-discoverable enough that a reviewer can trace a
  support claim to a suite result without scraping free-form notes

### Operator stability

Run on demand or on a schedule when Roger needs extra confidence in brittle or
licensed environments.

Required posture:

- may include selected `integration` suites that touch real providers, browsers,
  install flows, or host-runtime behavior
- may include the small curated `e2e` set when the environment is available
- should not become a dumping ground for work that belongs in lower lanes

### `release-candidate`

Run for tagged releases or release candidates that may become a shipped build.

Required posture:

- artifact checksum verification
- per-target packaging smoke for shipped artifact classes
- manual smoke on the blessed OS or browser paths
- confirmation that the shipped support claims still match what was actually
  built
- consumption of the current proof manifests or equivalent suite summaries for
  release-critical invariants before widening release wording

## Automated E2E Budget

Roger `0.1.0` carries one blessed automated heavyweight E2E:

- `E2E-01`: core review happy path

Current implementation status:

- implemented as `packages/cli/tests/e2e_core_review_happy_path.rs`
- registered in budget and suite metadata
- still requires a real run before any execution policy can claim that coverage

That test protects a product-defining promise across several boundaries:

- session-aware launch or resume
- real supported-provider execution on the blessed path
- findings normalization into Roger state
- local draft review or approval flow
- GitHub posting through a Roger-owned double
- persisted audit chain

Everything else should default downward into a cheaper tier unless a lower-cost
test shape leaves a meaningful product-risk gap.

If a future E2E defends a memory-assisted journey, it must assert the live
memory contract explicitly: truthful retrieval mode, correct scope bucket,
preserved provenance, and explicit degraded lexical-only fallback when semantic
retrieval is unavailable.

Everything else should default downward into a cheaper lane unless a lower-cost
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

These are explicitly integration plus smoke guards, not heavyweight E2Es.

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
scenario. It is explicitly an integration plus smoke guard, not a second
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
- `cataloged_candidate_e2e_ids`, when present, lists pre-shaped future E2Es
  that do not count toward the budget until they are promoted into
  `blessed_e2e_ids`
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

- If a behavior can be defended with pure inputs and outputs, keep it in the
  `unit` lane.
- If a behavior crosses one or two Roger-owned boundaries with good doubles, or
  proves a bounded real-surface contract, keep it in the `integration` lane.
- If a behavior validates a product-defining journey across several real Roger
  surfaces, consider `e2e`.
- If a behavior validates shipped artifacts, support claims, or manual release
  promises, keep it out of a new lane and model it as `release-candidate`
  evidence.

## `0.1.0` Mapping Summary

| Concern | Default lane or gate | Notes |
| --- | --- | --- |
| Domain logic, findings, schema, prompt repair | `unit` | Prefer parameterized tests over fixtures where possible |
| Storage, migrations, CLI robot shapes, TUI controller, adapter contracts | `integration` | Deterministic CI coverage |
| Core review happy path | `e2e` | Keep intentionally small and curated |
| Provider acceptance, bridge smoke, memory/search contract truth | `integration` plus operator stability where needed | Do not widen into E2E casually |
| Packaging, checksums, install truthfulness, release smoke | `release-candidate` gate | Operator-facing release evidence |

## Review Rule For Future Beads

Any bead that proposes a new automated E2E should name:

- the candidate test id
- the lane it belongs in
- the cheaper test shapes that were rejected
- the justification record it will add to the budget file if it exceeds the
  current blessed count
