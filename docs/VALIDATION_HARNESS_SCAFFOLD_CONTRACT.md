# Validation Harness Scaffold Contract

This document closes `rr-025.1`.

It defines the shared validation harness scaffold for Roger Reviewer `0.1.0`:
suite naming conventions, common directory layout, metadata envelope schema,
helper boundaries, and failure-artifact preservation rules.

Authority:

- [`AGENTS.md`](/Users/cdilga/Documents/dev/roger-reviewer/AGENTS.md)
- [`docs/TEST_HARNESS_GUIDELINES.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/TEST_HARNESS_GUIDELINES.md)
- [`docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md)
- [`docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md)

This document narrows those contracts into the concrete layout that
`rr-025.2` (fixture corpus and manifest) and `rr-025.3` (CI wiring) can
implement without inventing their own schemas.

---

## Governing Rules

- All validation lives under one canonical layout. No per-suite ad hoc paths.
- Suite naming derives from the family prefix table in
  [`VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md).
- Every suite must attach a metadata envelope. The schema is fixed here.
- Helpers that are shared across suite families live in `tests/support/`.
  Suite-local helpers stay inside the suite crate or module.
- Failure artifacts are preserved by default for acceptance, E2E, and
  bridge or provider integration suites. Other suites may opt in.

---

## Directory Layout

Roger's test artifacts, fixtures, and suite code follow this tree:

```text
tests/
  support/                  # shared helpers, doubles, and envelope utilities
    harness/                # suite runner and metadata-envelope helpers
    doubles/                # reusable doubles (GitHub adapter, TUI runtime, provider output)
    fixtures/               # fixture loader and manifest reader
  fixtures/                 # owned fixture corpus (managed by rr-025.2)
    <fixture-family>/       # one directory per fixture family
      MANIFEST.toml         # fixture manifest (see Fixture Manifest below)
      <fixture-files>

target/test-artifacts/      # runtime artifact output tree (gitignored)
  unit/
  property/
  integration/
  acceptance/
  e2e/
  release-smoke/
  failures/                 # preserved failure artifacts from any tier
```

Suite code lives in the standard Rust workspace layout
(`crates/<package>/tests/<suite_prefix>_*.rs` or `crates/<package>/tests/`
subdirectories). The `tests/` directory at the repo root owns the shared
support layer and the canonical fixture corpus only.

---

## Suite Naming Conventions

Suite file and module names must use the prefix families defined in
[`VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md):

| Prefix | Purpose |
|--------|---------|
| `unit_` | pure domain, schema, reducer, and serializer logic |
| `prop_` | state-machine and rule-matrix coverage |
| `int_storage_` | storage, migrations, artifact budgets |
| `int_harness_` | harness adapter boundaries with doubles |
| `int_cli_` | launch resolution, session finder, robot outputs |
| `int_tui_` | TUI controller, findings workflow, approval surfaces |
| `int_bridge_` | Native Messaging envelopes, launch-only mode |
| `int_github_` | draft invalidation, payload rendering, partial post |
| `int_search_` | prior-review lookup, lexical-only degrade |
| `accept_opencode_` | OpenCode provider-claim acceptance |
| `accept_bounded_provider_` | bounded live-CLI provider-claim acceptance (`codex`, `claude`, `gemini`; later `copilot`) |
| `e2e_` | full multi-boundary happy path (one blessed E2E only) |
| `smoke_` | manual or release-lane smoke |

No other prefix families may be introduced without updating
`VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md` first.

---

## Suite Metadata Envelope

Every automated suite must attach a metadata envelope so CI tiers, the E2E
budget guard, and validation matrix tracing can inspect it without parsing
test names or free-form comments.

### Envelope Fields

```toml
# Embedded in each suite file via a constant or test attribute,
# or emitted to a sidecar JSON at suite start.

[suite]
id = "<prefix>_<suite_name>"         # e.g. "int_harness_opencode_resume"
family = "<prefix>"                   # must match one of the prefix families above
flow_ids = ["F01", "F01.1"]          # flow families this suite defends
fixture_families = [                  # fixture families consumed
  "fixture_resumebundle_stale_locator"
]
support_tier = "opencode_tier_b"     # or "gemini_tier_a", "native_messaging_v1", etc.
degraded = false                      # true if this suite intentionally tests a degraded mode
bounded = false                       # true if this suite is launch-only or partial parity
tier = "integration"                  # unit | property | integration | acceptance | e2e | smoke
preserve_failure_artifacts = true     # must be true for acceptance, e2e, bridge/provider int
```

### Rules

- `id` must be unique across all suites in the Roger workspace.
- `flow_ids` must map to IDs defined in
  [`REVIEW_FLOW_MATRIX.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/REVIEW_FLOW_MATRIX.md).
- `fixture_families` must map to families defined in
  [`VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md).
- `degraded = true` and `bounded = true` must be explicit for any suite
  testing non-parity or launch-only behavior; the suite must not silently
  pass for full-feature behavior.
- `preserve_failure_artifacts = true` is mandatory for: all `accept_*`
  suites, the one blessed `e2e_` suite, and all `int_bridge_*` and
  `int_harness_*` suites.

---

## Helper Boundaries

### `tests/support/harness/`

Owns: suite runner bootstrap, metadata envelope emission, artifact-tree
initialization, and budget-guard integration.

Must not own: fixture data, provider-specific doubles, TUI runtime doubles.

### `tests/support/doubles/`

Owns: reusable adapter doubles for GitHub, TUI runtime services, bridge
Native Messaging, and provider output emission.

Rules:
- Each double must be clearly labeled as a Roger-owned test double.
- Doubles must not silently succeed for mutation paths; approval and posting
  doubles must require an explicit `expect_approval` or `expect_post` call.
- Do not share doubles with production code.

### `tests/support/fixtures/`

Owns: fixture manifest reader, fixture file loader, and fixture family
validation helpers.

Must not own: fixture data (that lives in `tests/fixtures/<family>/`).

### Suite-Local Helpers

Helpers used by only one suite family stay inside that suite's crate or
module. Do not promote a helper to `tests/support/` until it is used by two
or more distinct suite families.

---

## Fixture Manifest

Each fixture family directory must contain a `MANIFEST.toml`:

```toml
[fixture]
family = "fixture_resumebundle_stale_locator"
description = "Stale SessionLocator plus valid ResumeBundle reseed path"

[[fixture.allowed_consumers]]
suite_family = "accept_opencode_"
notes = "Primary consumer for locator-reopen and stale-locator reseed cases"

[[fixture.allowed_consumers]]
suite_family = "accept_bounded_provider_"
notes = "Bounded-provider reseed cases only"

[[fixture.allowed_consumers]]
suite_family = "int_harness_"
notes = "Adapter boundary tests"

[fixture.degraded_conditions]
stale_locator = "SessionLocator intentionally points to a deleted or moved session"
```

Rules:
- `family` must match one of the fixture families in
  `VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`.
- `allowed_consumers` must list the suite family prefixes permitted to
  consume this fixture. Cross-family sharing must be explicit, not
  discovered at runtime.
- `degraded_conditions` must document any intentionally broken or partial
  state encoded in the fixture.

---

## Failure Artifact Preservation Rules

Roger's test harness writes failures to `target/test-artifacts/failures/`.

Required behavior:

- Acceptance suites (`accept_*`): always preserve on failure, structured
  output, raw provider output, `ResumeBundle` snapshot, and continuity
  quality report.
- E2E suite (`e2e_core_review_happy_path`): always preserve on failure,
  full session trace, CLI output, approval-chain snapshot, and GitHub
  adapter response.
- Bridge integration suites (`int_bridge_*`): always preserve on failure,
  Native Messaging envelope, host manifest state, and bridge error payloads.
- Harness integration suites (`int_harness_*`): always preserve on failure,
  session transcript excerpt and locator state.
- Other suites: optional; flag with `preserve_failure_artifacts = true` in
  the metadata envelope if the suite touches a boundary that materially
  reduces diagnosis time when preserved.

Structure under `target/test-artifacts/failures/`:

```text
failures/
  <suite_id>/
    <timestamp>_<test_name>/
      metadata.json       # envelope fields plus failure summary
      <artifact-files>    # named by artifact class
```

Artifacts must use the names from
[`TEST_HARNESS_GUIDELINES.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/TEST_HARNESS_GUIDELINES.md)
§ Required artifact classes.

---

## Acceptance Summary For `rr-025.1`

This document now fixes:

- the canonical directory layout for suites, fixtures, and artifacts
- the suite naming prefix families and their binding to the validation matrix
- the metadata envelope schema and its required fields
- helper boundary rules and promotion policy
- the fixture manifest format and allowed-consumer contract
- failure artifact preservation rules and directory structure

That is sufficient for `rr-025.2` to materialize the first fixture corpus
and for `rr-025.3` to wire CI tiers and artifact retention without
reinventing these contracts.
