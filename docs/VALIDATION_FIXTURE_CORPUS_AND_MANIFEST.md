# Validation Fixture Corpus and Manifest

This document closes `rr-025.2`.

It defines the canonical Roger Reviewer `0.1.0` fixture corpus: the named
fixture families, their intended suite consumers, degraded-mode annotations,
provenance rules, and update policy.

Authority:

- [`docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md) — scaffold layout and `MANIFEST.toml` format
- [`docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md) — suite ownership and support claims
- [`docs/TEST_HARNESS_GUIDELINES.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/TEST_HARNESS_GUIDELINES.md) — tier rules and double policy

Each family in this document maps to a directory under `tests/fixtures/`
and must have a `MANIFEST.toml` whose `family` key matches the name here.

---

## Governing Rules

- Fixture data lives only in `tests/fixtures/<family>/`. No ad hoc repos or
  developer-machine temp state.
- Each fixture has one canonical purpose. When a test needs multiple
  conditions, compose two small fixtures rather than growing one.
- Fixture files that encode intentionally broken or degraded state must have
  their broken conditions explicitly listed in `MANIFEST.toml`.
- Fixture provenance (where the fixture data came from) must be documented
  in the `MANIFEST.toml` `[fixture.provenance]` block.
- Fixtures do not encode real GitHub tokens, email addresses, or real repo
  URLs unless they are sanitized public examples.

---

## Fixture Families

### `fixture_repo_compact_review`

**Purpose:** A small, self-contained review target with a single module,
a short diff, and a predictable finding set. Primary happy-path fixture.

**Allowed consumers:** `int_cli_*`, `int_tui_*`, `e2e_core_review_happy_path`

**Contents:**
- a small synthetic repo snapshot (no real origin)
- a compact unified diff representing a PR-like change
- a pre-built structured findings pack (`StructuredFindingsPack`) for replay

**Degraded conditions:** none; this fixture is intentionally clean

---

### `fixture_repo_monorepo_review`

**Purpose:** A larger multi-crate/multi-package repo target with cross-file
findings, search-recall pressure, and longer artifact chains.

**Allowed consumers:** `int_search_*`, `int_tui_*`

**Contents:**
- a synthetic multi-module repo snapshot
- a diff spanning multiple files and packages
- structured findings with cross-file code-evidence locations

**Degraded conditions:** none

---

### `fixture_same_pr_multi_instance`

**Purpose:** Two or more valid local review session targets for the same PR
on the same machine. Exercises instance-selection and routing logic.

**Allowed consumers:** `int_cli_*`, `int_bridge_*`, `smoke_same_pr_instances_*`

**Contents:**
- two pre-seeded Roger session ledger entries pointing to the same PR
- different local paths or worktrees for each instance

**Degraded conditions:**
- `two_valid_instances`: both sessions exist and are resumable

---

### `fixture_findings_valid_minimal`

**Purpose:** A structurally valid, minimal `StructuredFindingsPack` with
primary and supporting code-evidence locations.

**Allowed consumers:** `unit_*`, `int_harness_*`

**Contents:**
- one fully conformant `StructuredFindingsPack` (serialized JSON)
- at least two findings with `CodeEvidenceLocation` anchors
- a corresponding `FindingFingerprint` map

**Degraded conditions:** none

---

### `fixture_findings_partial_mixed`

**Purpose:** A pack with a mix of valid and salvageable findings — some
findings are structurally valid, some require repair.

**Allowed consumers:** `unit_*`, `int_harness_*`, `rr-011.3`

**Contents:**
- a `StructuredFindingsPack` with a valid subset and a repair-required subset
- one finding with a missing `CodeEvidenceLocation`
- one finding with a structurally invalid fingerprint (wrong hash format)

**Degraded conditions:**
- `partial_valid`: pack is not fully valid; repair loop must handle the
  invalid subset without discarding the valid subset

---

### `fixture_findings_raw_only`

**Purpose:** No structured pack — only raw provider output. Forces the
harness through the normalization-from-raw path.

**Allowed consumers:** `int_harness_*`, `rr-011.3`

**Contents:**
- raw OpenCode session transcript without a pre-extracted `StructuredFindingsPack`
- a manifest entry declaring the expected normalization output shape

**Degraded conditions:**
- `no_structured_pack`: structured output is absent; normalization from raw
  is the only path

---

### `fixture_findings_invalid_anchor`

**Purpose:** A structurally valid finding with a stale or bad
`CodeEvidenceLocation` — the anchor points to a line that no longer exists
in the diff or repo snapshot.

**Allowed consumers:** `prop_*`, `rr-011.2`, `rr-011.3`

**Contents:**
- a `StructuredFindingsPack` with one finding whose anchor is stale
- a repo snapshot delta that explains why the anchor is stale

**Degraded conditions:**
- `stale_anchor`: the anchor line number has shifted; the finding must be
  marked for refresh reconciliation

---

### `fixture_resumebundle_stale_locator`

**Purpose:** A stale `SessionLocator` (points to a moved or deleted OpenCode
session) paired with a valid `ResumeBundle` for the reseed path.

**Allowed consumers:** `accept_opencode_*`, `accept_gemini_*`, `rr-011.5`

**Contents:**
- a `SessionLocator` encoded to point at a non-existent session path
- a corresponding `ResumeBundle` with sufficient context for reseed
- the expected continuity quality report after reseed

**Degraded conditions:**
- `stale_locator`: direct reopen will fail; only the reseed path succeeds

---

### `fixture_opencode_dropout_return`

**Purpose:** Intentional bare-harness dropout and `rr return` control-flow
fixture. Proves Roger can hand off to plain OpenCode and accept the return.

**Allowed consumers:** `accept_opencode_*`, `rr-011.5`

**Contents:**
- a pre-seeded Roger session ledger entry with a valid `SessionLocator`
- a synthetic OpenCode session that includes the `ROGER_CONTROL_BUNDLE`
  environment marker
- the expected state after `rr return` rebinds the session

**Degraded conditions:**
- `bare_harness_active`: the session is live in plain OpenCode without Roger
  UI; `rr return` must rebind without data loss

---

### `fixture_bridge_launch_only_no_status`

**Purpose:** Native Messaging bridge present and functional, but configured
to honest launch-only mode with no status readback.

**Allowed consumers:** `int_bridge_*`, `rr-011.4`

**Contents:**
- a Native Messaging request envelope for a PR launch intent
- a bridge response indicating launch-only mode (no session status available)
- the expected Roger behavior: treat the launch as fire-and-forget, surface
  no status claim

**Degraded conditions:**
- `no_status_bridge_mode`: the bridge is operating in launch-only mode;
  Roger must not fake a status it cannot read

---

### `fixture_bridge_install_recovery`

**Purpose:** Missing or drift-version host manifest for the Native Messaging
bridge. Exercises install recovery and version-check paths.

**Allowed consumers:** `int_bridge_*`, `smoke_bridge_install_*`

**Contents:**
- a synthetic NativeMessaging host manifest directory with a missing or
  wrong-version manifest
- the expected install-repair flow trigger and output

**Degraded conditions:**
- `missing_manifest`: no host manifest present; bridge must guide the user
  through repair rather than silently failing
- `version_drift`: manifest exists but version does not match the bundled
  bridge binary

---

### `fixture_github_draft_batch`

**Purpose:** A local `OutboundDraftBatch` with multiple `OutboundDraft`
entries and a pending approval token.

**Allowed consumers:** `int_github_*`, `e2e_core_review_happy_path`

**Contents:**
- a serialized `OutboundDraftBatch` bound to a synthetic PR
- two or three draft entries with different GitHub comment target types
  (inline, top-level)
- an approval token stub

**Degraded conditions:** none; this is a clean pre-approval draft batch

---

### `fixture_partial_post_recovery`

**Purpose:** One posted action succeeds while another fails mid-batch.
Exercises partial-post recovery and audit trail behavior.

**Allowed consumers:** `int_github_*`, `rr-011.4`

**Contents:**
- a draft batch where the first item has a synthetic GitHub success response
  and the second item has a synthetic 422 error
- expected `PostedAction` audit records (one success, one failure)
- expected draft state after partial recovery

**Degraded conditions:**
- `partial_post`: posting is partially complete; the batch must not be
  silently retried as a whole; failed items must remain in a repairable state

---

### `fixture_refresh_rebase_target_drift`

**Purpose:** A rebased PR target where anchors have moved. Exercises refresh
identity, invalidation, and anchor-reconciliation behavior.

**Allowed consumers:** `prop_*`, `int_github_*`, `rr-011.2`, `rr-011.4`

**Contents:**
- an original `StructuredFindingsPack` with known code anchors
- a rebased diff where those anchor lines have shifted
- expected invalidation decisions for each finding

**Degraded conditions:**
- `anchor_drift_after_rebase`: the diff has changed; some findings must be
  marked stale and some may be reconcilable

---

### `fixture_migration_and_artifact_integrity`

**Purpose:** Roger SQLite schema migration path and artifact-budget
enforcement fixture.

**Allowed consumers:** `int_storage_*`

**Contents:**
- a pre-existing Roger store at a prior schema version
- the expected migration outcome
- an artifact set that is close to the configured budget limit, and one that
  exceeds it

**Degraded conditions:**
- `pre_migration_schema`: the store must be migrated before use; do not
  assume the latest schema
- `artifact_budget_exceeded`: artifact eviction must be triggered during
  budget enforcement

---

## Fixture Provenance and Update Rules

**Provenance:** Each fixture was synthesized from first principles to be
small, purpose-built, and free of real credentials or real repo history.
No fixture captures a real production system's state.

**Update rules:**

1. A fixture may only be updated if the underlying Roger contract it tests
   has changed.
2. All changes to a fixture must update the `MANIFEST.toml` `[fixture.provenance]`
   block with a short description and the bead or PR that caused the change.
3. When a contract change invalidates a fixture, do not silently remove it.
   Mark it `deprecated = true` in `MANIFEST.toml` and open a bead to replace
   or remove it before the next release.
4. Fixtures that encode intentionally degraded state must have their
   `degraded_conditions` block kept up to date. A degraded fixture that
   silently passes on the non-degraded path is a test-quality failure.

---

## Acceptance Summary For `rr-025.2`

This document now provides:

- the full list of named initial fixture families with purpose, allowed
  consumers, and degraded-condition annotations
- provenance and update rules that prevent silent fixture drift
- enough structure for `rr-025.3` to wire CI artifact retention and for
  implementation beads to load fixtures by family name without ad hoc paths

`rr-025.3` can now proceed to wire suite metadata, CI tiers, and artifact
retention into validation entrypoints using this corpus definition plus the
layout from `VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md`.
