# Validation CI Execution Policies and Entrypoints

This document closes `rr-025.3`.

It connects Roger's validation harness scaffold to the execution policies that
invoke Roger's three validation lanes, fixes the suite metadata registration
contract, and wires artifact retention and the automated E2E budget guard.

Authority:

- [`docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md`](docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md)
- [`docs/VALIDATION_FIXTURE_CORPUS_AND_MANIFEST.md`](docs/VALIDATION_FIXTURE_CORPUS_AND_MANIFEST.md)
- [`docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md`](docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md)
- [`docs/AUTOMATED_E2E_BUDGET.json`](docs/AUTOMATED_E2E_BUDGET.json)

This document does not override the lane and execution-policy definitions in
`TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md`. It narrows them into concrete
entrypoint names, runner commands, retention rules, and budget-guard
integration so downstream suites do not invent their own runner policy.

Current repo truth as of 2026-04-07:

- `tests/suites/e2e_core_review_happy_path.toml` is present as suite metadata.
- `packages/cli/tests/e2e_core_review_happy_path.rs` is landed as the
  executable implementation for that suite id.
- No execution policy should be described as having demonstrated functional E2E
  coverage unless the suite is actually run there.
- The metadata file is a registration record, not an implementation milestone
  by itself.
- `AUTOMATED_E2E_BUDGET.json` now budgets six major E2E journeys, but only the
  implemented and actually executed subset counts as coverage.
- Historical metadata still includes labels such as `prop_*`, `accept_*`, and
  `smoke_*`; treat those as sub-kinds inside the three-lane model until the
  harness metadata is simplified.

---

## Governing Rules

- Roger has exactly three validation lanes: `unit`, `integration`, and `e2e`.
- Entry points such as `fast-local`, `pr`, `gated`, `nightly`, and `release`
  are execution-policy names, not lane names.
- One runner policy. Suites attach metadata; the entrypoints read it.
  Suites must not each embed execution-policy logic.
- The E2E budget is machine-readable. The budget guard reads
  `docs/AUTOMATED_E2E_BUDGET.json`. Growth beyond the budget requires the
  five justification fields in that file before it is allowed.
- Artifact retention is per-tier and unconditional on failure for the named
  tiers. Suites must not suppress failure artifacts on these tiers.
- `manual-only` and `smoke` suites do not run in CI. They run in release
  preparation and are documented here for completeness only.

## Rust Quality Gates

These commands are part of Roger's Rust validation baseline even though they are
not separate validation lanes.

Required command set:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo llvm-cov --workspace --all-targets --summary-only
```

Rules:

- `cargo fmt --check` and `cargo clippy` are quality gates, not substitutes for
  suite evidence
- `cargo llvm-cov` is Roger's coverage-reporting source of truth, but coverage
  percentages do not override invariant ownership or proof-artifact
  requirements
- targeted suite replay should use `cargo test -p <package> --test <suite> --
  --nocapture`
- Roger does not require `rch` or any other remote compile wrapper as part of
  this baseline

---

## CI Execution Policies

### Policy 1 — Fast Local (`fast-local`)

**When:** Developer loop. Runs in under 60 seconds on a dev machine.

**Command:**
```sh
cargo test --workspace --lib -- --skip accept_ --skip e2e_ --skip smoke_
```
or, if a workspace-level `make` target is defined:
```sh
make test-fast
```

**Lane mix:** targeted `unit`

**Excludes:** all non-lib integration-family suites, acceptance-subkind suites,
E2E, and smoke suites

**Artifact retention:** none (fast-local runs are not CI jobs)

**Budget guard:** not enforced (no E2E suites run)

---

### Policy 2 — PR Gate (`pr`)

**When:** Every pull request. Target under 5 minutes on CI hardware.

**Command:**
```sh
cargo test --workspace -- --skip accept_ --skip e2e_ --skip smoke_
```
or:
```sh
make test-pr
```

**Lane mix:** broad `unit`, targeted `integration`

**Excludes:** provider-acceptance subkind suites, E2E, and smoke suites

**Artifact retention:** `target/test-artifacts/` for failures only;
preserved by CI artifact upload for 7 days.

**Budget guard:** enforced; `int_*` suites may not contain `e2e_` suites
or any test that crosses more than two boundaries.

---

### Policy 3 — Gated (`gated`)

**When:** Manual operator gate before higher-confidence merge/release decisions. May take longer than PR gate.

**Command:**
```sh
make test-gated
```

**Lane mix:** broad `unit`, broad `integration`

**Excludes:** `e2e_*` suite and smoke

**Artifact retention:** full `target/test-artifacts/` tree preserved for
all failures; upload for 14 days.

**Budget guard:** enforced; `accept_*` suites must not add new blessed
automated E2E tests.

---

### Policy 4 — Nightly (`nightly`)

**When:** Manual or operator-scheduled higher-cost validation run.

**Command:**
```sh
make test-nightly
```

**Lane mix:** broad `unit`, broad `integration`, selected `e2e`.

This policy counts as functional E2E coverage only when that suite is actually
run here.

**Artifact retention:** full tree; upload for 30 days.

**Budget guard:** **strictly enforced**. Before running, the budget guard
reads `docs/AUTOMATED_E2E_BUDGET.json` and checks:
- `current_planned_blessed_automated_e2e_count` ≤
  `blessed_automated_e2e_budget` (currently 6)
- if over budget: emit the five required justification fields as a CI
  failure with a `WARN` annotation and block merge until the fields are
  present in the budget file.

Important:
- budget compliance and suite registration do not replace the executable suite
- an implementation bead for `e2e_core_review_happy_path` closes only after the
  executable suite lands and is run
- the six-slot budget is a ceiling, not a requirement that one nightly run
  executes every major journey serially; implemented E2Es may be sharded or
  selected by claim surface

---

### Policy 5 — Release (`release`)

**When:** Release cut. Manually triggered as an operator gate.

**Command:**
```sh
make test-release
```

**Lane mix:** current lane evidence plus targeted smoke suites with automated
stubs, including `smoke_browser_launch_chrome`,
`smoke_browser_launch_brave`, and `smoke_browser_launch_edge`; manual smoke
checklist is run by the release owner separately.

**Artifact retention:** full tree plus a release-candidate artifact bundle;
upload indefinitely.

**Budget guard:** strictly enforced as in the `nightly` execution policy.

### Supported-Browser Launch Smoke Policy

`smoke_browser_launch_chrome`, `smoke_browser_launch_brave`, and
`smoke_browser_launch_edge` are named suite metadata ids for supported-browser
launch scenarios. They remain smoke suites and do not count toward the
heavyweight E2E budget.

Run these suites in the `release` execution policy when:

- bridge host registration behavior changed
- launch payload/envelope handling changed
- extension packaging lane changes could affect browser launch behavior
- release/support wording changed for Chrome/Brave/Edge launch claims

If none of the above changed, the `release` execution policy may rely on:

- green `int_bridge_*` coverage
- most recent passing browser-launch smoke artifacts

---

## Suite Metadata Registration

Every suite must register its metadata via the shared harness helper before
running any tests. In Rust this is done with a call to the harness bootstrap
at the start of the test module:

```rust
// At the top of each suite file or integration test module:
roger_test_harness::register_suite! {
    id: "int_harness_opencode_resume",
    family: "int_harness_",
    flow_ids: &["F01", "F01.1"],
    fixture_families: &["fixture_resumebundle_stale_locator"],
    support_tier: "opencode_tier_b",
    degraded: false,
    bounded: false,
    tier: "integration",
    preserve_failure_artifacts: true,
}
```

The `roger_test_harness::register_suite!` macro (or equivalent function
call) must:

1. Validate that `id` is unique within the workspace.
2. Validate that `family` matches one of the registered prefix families.
3. Validate that each `flow_ids` entry maps to a known flow in
   `REVIEW_FLOW_MATRIX.md` (checked at test-compile time via a generated
   constant table).
4. Initialize the artifact output directory for this suite under
   `target/test-artifacts/<tier>/<suite_id>/`.
5. If `preserve_failure_artifacts = true`, register a test-teardown hook
   that copies failure artifacts to `target/test-artifacts/failures/<suite_id>/`.

---

## Artifact Retention Behavior

Artifact retention is mandatory for:

| Execution policy | Behavior |
|------------------|----------|
| fast-local | no retention |
| PR | failure artifacts only, 7-day upload |
| gated | full artifacts on failure, 14-day upload |
| nightly | full artifacts always, 30-day upload |
| release | full artifacts plus bundle, indefinite upload |

`preserve_failure_artifacts = true` in the suite metadata ensures the
teardown hook fires. Suites that set it to `false` on PR tier will not have
their failures uploaded; this is only acceptable for pure `unit_*` and
`prop_*` suites where the test output is self-contained.

---

## E2E Budget Guard Integration

The budget guard is a thin Rust binary or build-script check that:

1. Reads `docs/AUTOMATED_E2E_BUDGET.json`.
2. Counts the number of suites with `tier = "e2e"` in the registered suite
   metadata.
3. If count > `blessed_automated_e2e_budget`, emits a structured error:
   ```
   ROGER_E2E_BUDGET_EXCEEDED: found <N> blessed automated E2E suites,
   budget allows <MAX>. To grow the budget, update
   docs/AUTOMATED_E2E_BUDGET.json with the five required justification
   fields and get explicit sign-off.
   ```
4. The budget guard must run as part of the `nightly` and `release`
   execution policies. It may optionally run on `pr` as a warning-only check.

Required justification fields (from `AUTOMATED_E2E_BUDGET.json`):
- `product_promise_defended`
- `why_lower_layer_is_insufficient`
- `boundaries_crossed`
- `estimated_maintenance_cost`
- `why_not_acceptance_or_release_smoke`

---

## Acceptance Summary For `rr-025.3`

This document fixes:

- the five CI tier entrypoints with commands, suite inclusions,
  exclusions, and artifact retention rules
- the suite metadata registration contract (`register_suite!` macro
  signature and validation rules)
- artifact retention behavior per tier and the preservation hook contract
- E2E budget guard behavior, read path, and required justification fields

Downstream suites (`rr-011.x`, `rr-025.4`) can now declare their metadata
and target a named tier without inventing runner policy.
