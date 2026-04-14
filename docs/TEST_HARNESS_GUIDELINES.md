# Test Harness Guidelines

This document turns Roger Reviewer's testing posture into an implementation-
facing contract. It is the canonical harness-design companion to
[`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md)
and
[`RELEASE_AND_TEST_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/RELEASE_AND_TEST_MATRIX.md).
[`TESTING.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/TESTING.md) is the
operator-facing entrypoint; this document remains the implementation-facing
support contract.

Use this document when:

- defining new test suites
- deciding whether a behavior belongs in `unit`, `integration`, or `e2e`
- adding fixtures, canned provider outputs, or browser or bridge transcripts
- reviewing execution-policy placement
- deciding whether a proposed new automated E2E is justified

This is not a generic testing tutorial. It is Roger's specific test-harness
contract for `0.1.x`.

Current repo truth:

- the implementation still carries historical suite-family and metadata labels
  such as `prop_*`, `accept_*`, and `smoke_*`
- until the harness metadata is simplified, treat those labels as sub-kinds
  inside the three-lane model: `prop_*` under `unit`, `accept_*` under
  `integration`, and `smoke_*` as operator or release evidence rather than a
  fourth lane

## Goals

- make Roger's validation posture executable rather than aspirational
- keep the one-blessed-path discipline real
- prevent accidental test-suite drift into expensive, redundant multi-boundary
  scenarios
- force explicit fixture and artifact ownership before implementation begins
- keep provider, browser, bridge, and approval claims tied to named suites
- bind release-critical support claims to named invariant ids and proof outputs

## Core Rules

- push confidence down the stack first
- prefer deterministic fixtures and doubles over ambient real environments
- keep exactly three validation lanes: `unit`, `integration`, and `e2e`
- keep parameterized and property-style suites inside the `unit` lane
- keep one blessed automated happy-path E2E in `0.1.x`
- require at least one real boundary test for each major external surface
- make degraded modes explicit in tests instead of silently omitting them
- every release-critical support claim should map to one or more invariant ids
  from
  [`VALIDATION_INVARIANT_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/VALIDATION_INVARIANT_MATRIX.md)
- allow Roger-owned doubles only when they model real contract edges and real
  failure modes; success-only fantasy doubles are not acceptable
- preserve failure artifacts when they materially reduce diagnosis time
- do not let browser or provider parity claims outrun the suites that defend
  them

## Invariant Ownership And Bead Translation

Roger uses three lanes, but it should reason about critical truths through
invariants.

Rules:

- new implementation work that changes a user-visible or operator-visible
  promise should cite one or more invariant ids from
  [`VALIDATION_INVARIANT_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/VALIDATION_INVARIANT_MATRIX.md)
  or add a new row there
- a bead should not claim a support boundary without also naming the suite
  families, fixture families, and proof outputs that defend the relevant
  invariants
- if a behavior cannot yet produce mechanically discoverable evidence, the docs
  should describe the gap plainly rather than widening the support claim

## Coverage Gap Escalation

Low coverage is not automatically a problem bead. It becomes a Roger problem
when it weakens a real promise or leaves a critical invariant effectively
unowned.

Agents should create or split a testing bead when they notice:

- suspiciously thin coverage around a release-critical invariant
- nominal-path coverage without matching degraded, invalidation, or recovery
  coverage for the same promise
- a suite family that appears to own a support claim in prose but does not
  produce believable proof artifacts in practice
- an underspecified test request whose faithful implementation depends on Roger's
  UX, support, or failure-handling vision

Plan-reread exception:

- ordinary implementation work may often proceed from a bead plus the relevant
  support docs
- testing or validation work must reread the canonical plan, `docs/TESTING.md`, and
  the relevant validation contracts when the bead is underspecified relative to
  the UX or support claim it is supposed to defend

The goal is not to make every agent reread everything. The goal is to prevent
agents from guessing when the test is supposed to encode product truth.

## Validation Lanes

### 1. Unit

Purpose:
- defend local domain rules, state transitions, and pure shaping logic,
  including property and parameterized coverage

Required coverage:
- `ReviewSession`, `ReviewRun`, `Finding`, `FindingState`, `OutboundDraft`,
  `OutboundDraftBatch`, and approval invalidation reducers
- `StructuredFindingsPack` parsing, normalization, salvage, and repair
  classification
- `ResumeBundle` construction, trimming, and continuity-state projection
- config layering and launch-profile resolution
- search scope filtering and degraded lexical-only fallbacks
- TUI presenter or reducer state without a live terminal
- GitHub outbound payload rendering
- bridge-envelope and command-result serialization

Rules:
- no live provider process
- no live browser
- no real GitHub network
- use compact parameter tables when a rule matrix exists

### 2. Integration

Purpose:
- defend a boundary between Roger-owned components or between Roger and one
  external adapter contract

Required coverage:
- storage and migration
- prompt pipeline plus canned provider-output corpora
- harness adapters with doubles and resumability fixtures
- CLI session binding and robot-output stability
- TUI controller tests with fake runtime services
- Native Messaging envelope handling and host-mode failure paths
- Native Messaging host-runtime execution against the actual registered `rr`
  binary, not just in-process bridge helper calls
- GitHub adapter behavior with Roger-owned doubles
- multi-instance and worktree routing
- index rebuild and artifact lookup
- provider-acceptance suites proving truthful launch, resume, reseed, bounded
  dropout, and published provider limits
- transaction and crash-recovery suites for launch binding, artifact writes,
  return/rebind, retries after partial failure, and stale-event rejection
- search and memory contract coverage, including repo-first lookup, explicit
  overlays, provenance buckets, candidate-versus-promoted distinctions, and
  degraded lexical-only fallback

Required unit-lane matrices that should not be promoted into integration unless
the boundary itself is under test:
- config-layer precedence
- finding triage and outbound-state transitions
- draft invalidation causes
- provider capability tiers
- refresh reconciliation outcomes
- instance and worktree isolation rules
- robot-output shape variants
- suggestion rendering edge cases

Rules:
- target one meaningful boundary per suite
- avoid mixing browser, provider, GitHub, and approval semantics into one test
  unless that is the specific boundary under test
- this lane may touch real provider or bridge boundaries where needed, but it
  should still stay narrower than a full product journey

### 3. End-to-End

Purpose:
- prove Roger's defining local review loop works across the critical
  multi-boundary path

The blessed `0.1.x` E2E is:
- `E2E-01 Core review happy path`

It must include:
- CLI launch
- session create or resume on the blessed provider path
- valid structured findings intake
- local draft materialization
- explicit local approval step
- post through a GitHub adapter double
- durable audit persistence

It must not expand casually into:
- browser launch
- extension readback
- multi-instance routing
- malformed findings
- partial post recovery
- provider-bounded degraded modes
- most provider truthfulness checks
- most search and memory contract checks

Those belong in lower-cost suites unless a later explicit justification says
otherwise.

If an E2E claims to defend a memory-assisted journey, it must assert the live
memory contract explicitly: truthful retrieval mode, correct scope bucket,
preserved provenance, and degraded lexical-only fallback when semantic retrieval
is unavailable.

The prescriptive E2E catalog, including unblessed candidate journeys, lives in
[`RELEASE_AND_TEST_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/RELEASE_AND_TEST_MATRIX.md).
Only entries carried in
[`AUTOMATED_E2E_BUDGET.json`](/Users/cdilga/Documents/dev/roger-reviewer/docs/AUTOMATED_E2E_BUDGET.json)
count as blessed heavyweight E2Es.

## Execution Policies And Release Evidence

Roger recognizes only three validation lanes. Everything else is an execution
policy or release gate.

Recommended execution policies:

- `local-bead`: smallest truthful `unit` or `integration` slice that should run
  before committing a bead
- CI reproduction: deterministic reruns of the relevant `unit` and
  `integration` coverage
- operator stability: on-demand or scheduled runs of expensive real-surface
  `integration` suites and the few E2Es that need them
- `release-candidate`: explicit operator gate backed by lane evidence plus
  smoke and artifact proof

### Operator And Release Smoke

Purpose:
- defend the surfaces that are too expensive or brittle to over-automate early

Required areas:
- CLI launch into a real local review
- Native Messaging bridge launch
- refresh after new commits
- explicit approval before posting
- OpenCode dropout and return
- one same-PR multi-instance sanity pass

Rules:
- smoke is evidence for operator stability and release-candidate decisions. It
  is not a fourth lane and it does not replace lower-layer automation

## Fixture Contract

Roger should create and own a purpose-built fixture corpus. The harness should
not rely on ad hoc developer repos or hand-assembled temp state.

Required fixture families:

- compact single-repo review fixture
- monorepo review fixture
- same-PR multi-instance fixture
- malformed findings corpora
- partial findings corpora
- raw-only findings corpora
- invalid-anchor and moved-file refresh fixtures
- `ResumeBundle` reopen, reseed, and dropout fixtures
- GitHub draft and posted-action payload fixtures
- Native Messaging request and response transcripts
- browser launch-intent payload fixtures
- migration and artifact-store integrity fixtures

Rules:
- fixtures must be named by purpose, not by author or date
- each fixture must document the suite families allowed to consume it
- fixtures that encode degraded behavior must say what is intentionally broken
- large opaque corpora are discouraged; prefer several small fixtures with one
  job each

## Double and Stub Policy

Roger should distinguish sharply between doubles that are acceptable and real
boundaries that must exist at least once.

Allowed and preferred doubles:
- GitHub posting and thread mutation behavior
- provider output emission when testing normalization or repair
- TUI runtime services
- extension status readback
- index backend and embedding backend triggers where the invariant is Roger's
  orchestration rather than the external model

Must have at least one real boundary path somewhere in the overall suite:
- blessed provider path
- Native Messaging bridge path
- local CLI launch and resume path

Bridge-runtime rule:

- install/setup/doctor checks do not by themselves prove browser launch works
- at least one automated suite must spawn the actual registered host binary,
  feed it a Native Messaging request envelope over stdin, and assert a bounded
  response envelope on stdout
- browser-launch smoke must treat "button click dispatches" and "host manifest
  exists" as insufficient unless a host-runtime round trip is also proven

## Test Artifact Layout

Roger's harness should write artifacts to one predictable tree:

```text
target/test-artifacts/
  unit/
  property/
  integration/
  acceptance/
  e2e/
  release-smoke/
  failures/
```

Required artifact classes:
- normalized structured outputs
- raw provider outputs
- bridge transcripts
- posted-action and approval-chain snapshots
- reducer or controller state snapshots
- fixture provenance metadata
- failure summaries with the owning suite name and flow IDs

Rules:
- preserve artifacts on failure by default in CI
- preserve artifacts on success only for suites explicitly marked as
  investigation-heavy
- prefer structural snapshots over pixel-golden terminal captures

## Suite Naming

Recommended prefixes:

- `unit_*`
- `prop_*`
- `int_*`
- `accept_*`
- `e2e_*`
- `smoke_*`

Every non-unit suite should declare:
- covered flow IDs from
  [`REVIEW_FLOW_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/REVIEW_FLOW_MATRIX.md)
- required fixture families
- whether the path is blessed, bounded, degraded, launch-only, or manual-only

## Execution Policy Contract

| Policy | Purpose | Allowed lane mix | Must block merges |
|--------|---------|------------------|-------------------|
| `local-bead` | immediate developer feedback before commit | targeted `unit`, targeted `integration` | no |
| CI reproduction | normal pull-request safety and deterministic replay | broad `unit`, targeted `integration` | yes |
| operator stability | slower truth-defending runs in brittle or licensed environments | selected `integration`, selected `e2e` | only when explicitly required |
| `release-candidate` | tagged-release validation and publish gate | current lane evidence plus smoke and artifact verification | yes for release promotion |

Current repo truth as of 2026-04-07:
- `e2e_core_review_happy_path` exists as suite metadata and budget policy.
- executable functional coverage now exists in
  `packages/cli/tests/e2e_core_review_happy_path.rs`.
- do not claim functional automated E2E coverage for any execution policy
  unless the executable suite is actually run there.
- the metadata file reserves an id and documents intended scope, but the E2E
  exists only because the executable test landed.

Rules:
- a suite must live in exactly one default lane, even if historical metadata
  still uses more specific sublabels
- promoting a suite into a more expensive execution policy requires an explicit
  reason in code review or the planning notes
- the browser path is not allowed to become a mandatory PR-tier dependency in
  `0.1.x`

## Automated E2E Budget Guard

Roger's E2E budget is intentionally strict.

The canonical machine-readable budget file is:
- [`AUTOMATED_E2E_BUDGET.json`](/Users/cdilga/Documents/dev/roger-reviewer/docs/AUTOMATED_E2E_BUDGET.json)

Guard rules:
- `0.1.x` allows exactly one blessed automated happy-path E2E by default
- a new automated E2E must include a written justification that explains why a
  unit, parameterized, acceptance, or narrow integration suite would not defend
  the promise more cheaply
- declaring an `e2e_*` suite in metadata or budget files does not mean Roger
  has that E2E; the suite exists only when executable tests land and are run
- local runs should warn when the budget increases without a recorded
  justification
- CI should fail once the warning-only phase is retired

Required justification fields for any extra automated E2E:
- product promise defended
- why lower-layer coverage is insufficient
- boundaries crossed
- estimated maintenance cost
- why the scenario is not better represented as provider acceptance or release
  smoke

## Bead Mapping

This harness contract exists to make the following beads executable rather than
generic:

- `rr-011.7`: tiers and budget guard
- `rr-025`: validation matrix, fixtures, support coverage
- `rr-025.1`: shared validation harness scaffold and artifact layout
- `rr-025.2`: canonical fixture corpus and manifest
- `rr-025.3`: suite metadata, CI-tier entrypoints, and artifact-retention wiring
- `rr-011.1`: provider acceptance
- `rr-011.2`: refresh identity validation
- `rr-011.3`: degraded findings validation
- `rr-011.4`: invalidation, launch-only bridge, partial post recovery
- `rr-011.5`: clarification plus dropout and return
- `rr-011.6`: re-entry and same-PR routing

## Implementation Order

The first implementation-facing harness slice should be:

1. `rr-025`: validation matrix, flow coverage, and support-claim ownership
2. `rr-025.1`: shared harness scaffold and artifact layout
3. `rr-025.2`: canonical fixture corpus and manifest
4. `rr-025.3`: suite metadata, CI-tier entrypoints, and artifact retention wiring
5. unit and parameterized harness helpers inside that shared harness
6. narrow integration harness for storage, prompt normalization, and CLI resume
7. provider acceptance harness for OpenCode and the bounded live-CLI providers
8. keep one blessed automated E2E implemented and runnable
9. release-smoke checklist and artifact verification

Do not resolve step 8 by editing docs, budget files, or suite metadata alone.
That step closes only when the executable E2E implementation lands and is run.

No `rr-011.x` validation suite should start before `rr-025.3` lands. The point
is to make suites inherit one Roger-owned harness instead of each suite
inventing its own runner policy, fixture layout, or artifact behavior.

Do not start by building browser-heavy or provider-matrix-heavy E2Es.

## Review Standard

When reviewing a proposed new suite, ask:

- what exact product promise does this defend
- why is this not a lower-layer test
- which flow IDs does it cover
- which support claim would become dishonest without it
- what fixture family owns its inputs
- what artifacts will a failure leave behind

If those answers are vague, the suite is probably too broad, too expensive, or
still underspecified.
