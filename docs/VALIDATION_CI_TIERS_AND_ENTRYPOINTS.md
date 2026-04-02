# Validation CI Tiers and Entrypoints

This document closes `rr-025.3`.

It connects Roger's validation harness scaffold to the execution lanes,
defines the CI tier entrypoints, fixes the suite metadata registration
contract, and wires artifact retention and the automated E2E budget guard.

Authority:

- [`docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md)
- [`docs/VALIDATION_FIXTURE_CORPUS_AND_MANIFEST.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/VALIDATION_FIXTURE_CORPUS_AND_MANIFEST.md)
- [`docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md)
- [`docs/AUTOMATED_E2E_BUDGET.json`](/Users/cdilga/Documents/dev/roger-reviewer/docs/AUTOMATED_E2E_BUDGET.json)

This document does not override the tier definitions in
`TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md`. It narrows them into concrete
entrypoint names, runner commands, retention rules, and budget-guard
integration so downstream suites do not invent their own runner policy.

---

## Governing Rules

- One runner policy. Suites attach metadata; the entrypoints read it.
  Suites must not each embed tier logic.
- The E2E budget is machine-readable. The budget guard reads
  `docs/AUTOMATED_E2E_BUDGET.json`. Growth beyond the budget requires the
  five justification fields in that file before it is allowed.
- Artifact retention is per-tier and unconditional on failure for the named
  tiers. Suites must not suppress failure artifacts on these tiers.
- `manual-only` and `smoke` suites do not run in CI. They run in release
  preparation and are documented here for completeness only.

---

## CI Tier Entrypoints

### Tier 1 — Fast Local (`fast-local`)

**When:** Developer loop. Runs in under 60 seconds on a dev machine.

**Command:**
```sh
cargo test --workspace --lib -- --skip accept_ --skip e2e_ --skip smoke_
```
or, if a workspace-level `make` target is defined:
```sh
make test-fast
```

**Includes:** `unit_*`, `prop_*`

**Excludes:** all integration, acceptance, E2E, and smoke suites

**Artifact retention:** none (fast-local runs are not CI jobs)

**Budget guard:** not enforced (no E2E suites run)

---

### Tier 2 — PR Gate (`pr`)

**When:** Every pull request. Target under 5 minutes on CI hardware.

**Command:**
```sh
cargo test --workspace -- --skip accept_ --skip e2e_ --skip smoke_
```
or:
```sh
make test-pr
```

**Includes:** `unit_*`, `prop_*`, `int_*`

**Excludes:** provider acceptance, E2E, and smoke suites

**Artifact retention:** `target/test-artifacts/` for failures only;
preserved by CI artifact upload for 7 days.

**Budget guard:** enforced; `int_*` suites may not contain `e2e_` suites
or any test that crosses more than two boundaries.

---

### Tier 3 — Gated (`gated`)

**When:** Merge to main. May take longer than PR gate.

**Command:**
```sh
make test-gated
```

**Includes:** `unit_*`, `prop_*`, `int_*`, `accept_opencode_*`,
`accept_gemini_*`

**Excludes:** `e2e_*` suite and smoke

**Artifact retention:** full `target/test-artifacts/` tree preserved for
all failures; upload for 14 days.

**Budget guard:** enforced; `accept_*` suites must not add new blessed
automated E2E tests.

---

### Tier 4 — Nightly (`nightly`)

**When:** Scheduled nightly CI run on the main branch.

**Command:**
```sh
make test-nightly
```

**Includes:** all of Tier 3 plus `e2e_core_review_happy_path`

**Artifact retention:** full tree; upload for 30 days.

**Budget guard:** **strictly enforced**. Before running, the budget guard
reads `docs/AUTOMATED_E2E_BUDGET.json` and checks:
- `current_planned_blessed_automated_e2e_count` ≤
  `blessed_automated_e2e_budget` (currently 1)
- if over budget: emit the five required justification fields as a CI
  failure with a `WARN` annotation and block merge until the fields are
  present in the budget file.

---

### Tier 5 — Release (`release`)

**When:** Release cut. Manually triggered.

**Command:**
```sh
make test-release
```

**Includes:** all of Tier 4 plus targeted smoke suites with automated stubs,
including `smoke_browser_launch_chrome`, `smoke_browser_launch_brave`, and
`smoke_browser_launch_edge`; manual smoke checklist is run by the release owner
separately.

**Artifact retention:** full tree plus a release-candidate artifact bundle;
upload indefinitely.

**Budget guard:** strictly enforced as in Tier 4.

### Supported-Browser Launch Smoke Policy

`smoke_browser_launch_chrome`, `smoke_browser_launch_brave`, and
`smoke_browser_launch_edge` are named suite metadata ids for supported-browser
launch scenarios. They remain smoke suites and do not count toward the
heavyweight E2E budget.

Run these suites in Tier 5 release validation when:

- bridge host registration behavior changed
- launch payload/envelope handling changed
- extension packaging lane changes could affect browser launch behavior
- release/support wording changed for Chrome/Brave/Edge launch claims

If none of the above changed, Tier 5 may rely on:

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

| Tier | Behavior |
|------|----------|
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
4. The budget guard must run as part of Tier 4 (nightly) and Tier 5
   (release). It may optionally run on PR gate as a warning-only check.

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
