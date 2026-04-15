# Docs Tree Inventory And Cleanup Plan

Status: active documentation-cleanup process support.

Purpose:

- inventory every Markdown document under `docs/`
- assign each file one documentation class
- assign each file one immediate cleanup action
- keep documentation cleanup aligned with the agentic-first planning posture in
  `PLAN_FOR_ROGER_REVIEWER.md` and `AGENTS.md`

Authority:

- `AGENTS.md` remains the operational authority
- `PLAN_FOR_ROGER_REVIEWER.md` remains the canonical product and implementation
  plan
- this file is process support for cleaning up the documentation tree; it does
  not replace the canonical plan or support contracts

Special rule for this cleanup:

- keep `docs/PLAN_FOR_TRUTHFUL_PROVIDER_PARITY_AND_GITHUB_COPILOT_CLI.md` as a
  bounded side-plan only until `rr-92l0.1` merges accepted truth back into the
  canonical plan and support contracts; do not let it become a permanent
  parallel authority source

## Action vocabulary

| Action | Meaning |
|--------|---------|
| `keep` | Keep in place as part of the stable docs tree |
| `keep+status` | Keep, but add or tighten an explicit status/header in a later pass |
| `merge-back` | Keep for now, but merge accepted truth back into canonical/support docs and then downgrade or archive |
| `archive-later` | Preserve, but move to a clearer archive/history namespace in a later cleanup slice |

## 1. Canonical and bounded planning docs

| Path | Class | Action | Notes |
|------|-------|--------|-------|
| `docs/PLAN_FOR_ROGER_REVIEWER.md` | canonical plan | `keep` | Main product truth |
| `docs/PLAN_FOR_TRUTHFUL_PROVIDER_PARITY_AND_GITHUB_COPILOT_CLI.md` | bounded side-plan | `merge-back` | Preserve only until accepted truth is merged back through `rr-92l0.1`; then downgrade or archive |
| `docs/PLAN_FOR_EXTENSION_SETUP_AND_HAPPY_PATH_VALIDATION.md` | bounded side-plan | `merge-back` | Keep while extension happy-path recovery remains active |
| `docs/PLAN_FOR_SCHEMA_MIGRATIONS_AND_UPDATE_COMPATIBILITY.md` | bounded side-plan | `merge-back` | Keep until migration contract and implementation fully supersede it |
| `docs/ROUND_05_SURFACE_RECONCILIATION_BRIEF.md` | bounded side-plan | `merge-back` | Useful reconciliation brief; accepted TUI workspace truth should migrate into support contracts and the canonical plan rather than staying here long-term |

## 2. Active support contracts and matrices

| Path | Class | Action | Notes |
|------|-------|--------|-------|
| `docs/ATTENTION_EVENT_AND_NOTIFICATION_CONTRACT.md` | support contract | `keep` | Canonical attention-state contract |
| `docs/CORE_DOMAIN_SCHEMA_AND_FINDING_FINGERPRINT.md` | support contract | `keep` | Core entity and finding identity contract |
| `docs/DATA_MODEL_AND_STORAGE_CONTRACT.md` | support contract | `keep` | Canonical data/storage model contract |
| `docs/EXECUTION_GOVERNANCE_AND_REPO_BOUNDARY.md` | support contract | `keep` | Delivery-governance contract |
| `docs/EXTENSION_PACKAGING_AND_RELEASE_CONTRACT.md` | support contract | `keep` | Extension packaging and release contract |
| `docs/HARNESS_SESSION_LINKAGE_CONTRACT.md` | support contract | `keep` | Roger-to-harness continuity boundary |
| `docs/PROMPT_PRESET_AND_OUTCOME_CONTRACT.md` | support contract | `keep` | Prompt preset and outcome-event contract |
| `docs/PERSONA_JOURNEYS_AND_CHAOS_RECOVERY.md` | support matrix | `keep` | User-language persona journeys with stable scenario ids, chaos/recovery scripts, and restart/corruption cuts for flow design and E2E selection |
| `docs/RELEASE_AND_TEST_MATRIX.md` | support matrix | `keep` | Provider/browser/OS validation truth |
| `docs/RELEASE_CALVER_VERSIONING_CONTRACT.md` | support contract | `keep` | Release-version authority |
| `docs/REVIEW_FLOW_MATRIX.md` | support matrix | `keep` | Flow family coverage matrix |
| `docs/ROBOT_CLI_CONTRACT.md` | support contract | `keep` | Stable machine interface contract |
| `docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md` | support contract | `keep` | Search and memory policy contract |
| `docs/STORE_MIGRATION_COMPATIBILITY_AND_OPERATOR_CONTRACT.md` | support contract | `keep` | Migration and operator compatibility contract |
| `docs/TESTING.md` | support contract | `keep` | Operator-facing testing doctrine and validation-contract entrypoint |
| `docs/TEST_EXECUTION_TIERS_AND_E2E_BUDGET.md` | support contract | `keep` | Execution-tier and E2E budget contract |
| `docs/TEST_HARNESS_GUIDELINES.md` | support contract | `keep` | Harness policy and suite-layer contract |
| `docs/TUI_RUNTIME_SUPERVISOR_POLICY.md` | support contract | `keep` | TUI runtime policy |
| `docs/TUI_WORKSPACE_AND_OPERATOR_FLOW_CONTRACT.md` | support contract | `keep` | First-release TUI workspace and operator-flow contract |
| `docs/VALIDATION_CI_TIERS_AND_ENTRYPOINTS.md` | support contract | `keep` | CI entrypoint and artifact policy |
| `docs/VALIDATION_FIXTURE_CORPUS_AND_MANIFEST.md` | support contract | `keep` | Fixture corpus contract |
| `docs/VALIDATION_HARNESS_SCAFFOLD_CONTRACT.md` | support contract | `keep` | Harness structure and metadata contract |
| `docs/VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md` | support matrix | `keep` | Coverage ownership and support-claim mapping |

## 3. Bead, onboarding, ADR, and process-support docs

| Path | Class | Action | Notes |
|------|-------|--------|-------|
| `docs/BEADS_WORKSPACE_STATUS.md` | process support | `keep+status` | Current workspace health and repair notes |
| `docs/BEAD_SEED_FOR_ROGER_REVIEWER.md` | bead seed | `keep` | Canonical decomposition seed |
| `docs/BEAD_CREATION_INPUTS.md` | process support | `keep` | Bounded authoritative packet for bead-creation and bead-polish workflows |
| `docs/DEV_MACHINE_ONBOARDING.md` | process support | `keep` | Machine setup and workflow access guide |
| `docs/DOCS_TREE_INVENTORY_AND_CLEANUP_PLAN.md` | process support | `keep` | Full-tree docs inventory and cleanup action map |
| `docs/IMPLEMENTATION_SOURCES.md` | process support | `keep+status` | External implementation reference ledger, not product truth |
| `docs/PLANNING_WORKFLOW_PROMPTS.md` | process support | `keep` | Prompt pack for critique/integration/bead-polish loops |
| `docs/REFERENCE_SOURCES_AND_EXPLORATION_TARGETS.md` | process support | `keep` | Approved exploration targets |
| `docs/REPO_ONBOARDING_AND_DISCOVERY_PROMPTS.md` | process support | `keep` | Repo study and authority-mapping prompts |
| `docs/adr/README.md` | ADR index | `keep` | ADR entrypoint |
| `docs/adr/001-rust-first-local-runtime.md` | ADR | `keep` | Architectural decision record |
| `docs/adr/002-harness-and-session-durability-contract.md` | ADR | `keep` | Architectural decision record |
| `docs/adr/003-browser-bridge-and-extension-dependency-policy.md` | ADR | `keep` | Architectural decision record |
| `docs/adr/004-scope-and-memory-promotion-policy.md` | ADR | `keep` | Architectural decision record |
| `docs/adr/005-multi-instance-and-resource-isolation.md` | ADR | `keep` | Architectural decision record |
| `docs/adr/006-structured-findings-contract-and-repair-loop.md` | ADR | `keep` | Architectural decision record |
| `docs/adr/007-harness-native-roger-command-surface.md` | ADR | `keep` | Architectural decision record |
| `docs/adr/008-tui-runtime-and-concurrency-boundary.md` | ADR | `keep` | Architectural decision record |
| `docs/adr/009-prompt-preset-and-outcome-events.md` | ADR | `keep` | Architectural decision record |
| `docs/beads/BEAD_AND_PROMPT_FAILURE_PATTERNS.md` | bead/process support | `keep` | Retrospective for better bead/prompt shaping |
| `docs/beads/rr-1ab.4-fresh-init-upstream-prep.md` | bead/process support | `keep+status` | Narrow bead-specific planning artifact |

## 4. Historical critiques, readiness records, and research notes

| Path | Class | Action | Notes |
|------|-------|--------|-------|
| `docs/ALIEN_ARTEFACTS_FOR_ROGER_REVIEWER.md` | historical critique support | `keep+status` | External critique packet |
| `docs/ALIEN_WORKFLOWS_FOR_ROGER_REVIEWER.md` | historical critique support | `keep+status` | External critique workflow pack |
| `docs/ARCHIVE_PLAN_FOR_FLYWHEEL_STACK_BOOTSTRAP.md` | archive artifact | `archive-later` | Already marked archived; move under a clearer archive/history namespace later |
| `docs/BR_TRUST_AUDIT_2026-03-31.md` | historical incident note | `archive-later` | One-off trust audit; preserve but separate from live product docs |
| `docs/BR_UPSTREAM_BUG_REPROS_2026-04-01.md` | historical incident note | `archive-later` | One-off repro dossier; preserve but archive later |
| `docs/CRITIQUE_ROUND_01_FOR_ROGER_REVIEWER.md` | historical critique | `keep+status` | Historical rationale only |
| `docs/CRITIQUE_ROUND_02_FOR_ROGER_REVIEWER.md` | historical critique | `keep+status` | Historical rationale only |
| `docs/CRITIQUE_ROUND_03_FOR_ROGER_REVIEWER.md` | historical critique | `keep+status` | Historical rationale only |
| `docs/CRITIQUE_ROUND_03_SUPPLEMENT_FOR_ROGER_REVIEWER.md` | historical critique | `keep+status` | Historical integration record |
| `docs/READINESS_IMPLEMENTATION_GATE_DECISION.md` | historical decision record | `keep` | Authoritative readiness gate result |
| `docs/READINESS_REVIEW_FIRST_IMPLEMENTATION_SLICE_WITHOUT_EXTENSION.md` | historical decision record | `keep` | Authoritative readiness artifact |
| `docs/READINESS_REVIEW_SYNTHESIS.md` | historical synthesis | `keep+status` | Historical readiness synthesis |
| `docs/ROUND_04_ARCHITECTURE_RECONCILIATION_BRIEF.md` | historical reconciliation record | `keep+status` | Historical prep brief |
| `docs/ROUND_04_ARCHITECTURE_RECONCILIATION_OUTCOME.md` | historical reconciliation record | `keep+status` | Historical outcome record |
| `docs/ROUND_04_WORKTREE_SETUP_RESEARCH.md` | historical research note | `keep+status` | Research note; not canonical spec |
| `docs/SUPPLEMENTARY_CHATGPT54PRO_FEEDBACK_ROUND_03.md` | historical research artifact | `keep+status` | Raw external research artifact |
| `docs/SWARM_BEAD_OPERABILITY_REMEDIATION_PLAN.md` | historical/process note | `archive-later` | Swarm-control remediation note, not product truth |
| `docs/SWARM_LIVE_LOOP_REHEARSAL_20260331.md` | historical rehearsal record | `archive-later` | Bounded rehearsal evidence |
| `docs/SWARM_QUEUE_TRUST_REHEARSAL_20260331.md` | historical rehearsal record | `archive-later` | Bounded rehearsal evidence |
| `docs/TOON_OUTPUT_FORMAT_EVALUATION.md` | historical evaluation note | `archive-later` | One-off output-format evaluation |

## 5. Operator runbooks, smoke checklists, and design notes

| Path | Class | Action | Notes |
|------|-------|--------|-------|
| `docs/extension-entry-ux-smoke.md` | operator/runbook doc | `keep+status` | Smoke path for extension placement and fallback |
| `docs/extension-identity-direction.md` | design note | `keep+status` | Narrow design rationale; not product spec |
| `docs/extension-panel-theme-smoke.md` | operator/runbook doc | `keep+status` | Theme/readability smoke checklist |
| `docs/extension-visual-identity-smoke.md` | operator/runbook doc | `keep+status` | Visual identity smoke evidence |
| `docs/extension-visual-identity.md` | design note | `keep+status` | Bounded visual-identity decision record |
| `docs/release-publish-operator-smoke.md` | operator/runbook doc | `keep+status` | Release publish smoke checklist |
| `docs/swarm/CI_FAILURE_INTAKE_RUNBOOK.md` | operator/runbook doc | `keep+status` | CI failure watcher operations |
| `docs/swarm/HUMAN_OPERATOR_FLYWHEEL_GUIDE.md` | operator/runbook doc | `keep+status` | Canonical human operator doctrine plus direct `ntm` usage guide |
| `docs/swarm/NTM_OPERATOR_GUIDE.md` | operator/runbook doc | `keep+status` | Compatibility pointer to the canonical human operator guide |
| `docs/swarm/command_palette.md` | operator/runbook doc | `keep+status` | Command palette content, not product spec |
| `docs/swarm/maintenance-lane-policy.md` | operator/runbook doc | `keep+status` | Swarm maintenance-lane policy |
| `docs/swarm/maintenance-marching-orders.md` | operator/runbook doc | `keep+status` | Swarm maintenance prompt/runbook |
| `docs/swarm/overnight-marching-orders.md` | operator/runbook doc | `keep+status` | Swarm worker startup prompt |
| `docs/swarm/readiness-review-marching-orders.md` | operator/runbook doc | `keep+status` | Historical readiness-review prompt |
| `docs/swarm/worker-operating-doctrine.md` | operator/runbook doc | `keep+status` | Long-form swarm worker doctrine |

## 6. Skills and raw context

| Path | Class | Action | Notes |
|------|-------|--------|-------|
| `docs/roger-reviewer-brain-dump.md` | raw intent/archive artifact | `keep+status` | Original intent source, not current spec |
| `docs/skills/README.md` | reusable skill index | `keep` | Scoped skills directory entrypoint |
| `docs/skills/DICKLESWORTHSTONE_SOURCE_EXTRACTION_NOTES.md` | reusable skill/process artifact | `keep+status` | Source-extraction note for reusable Roger skills |
| `docs/skills/ROGER_ALIEN_ARTIFACT_DECISION_CONTRACT.md` | reusable skill | `keep` | Reusable Roger skill artifact |
| `docs/skills/ROGER_EXTREME_SOFTWARE_OPTIMIZATION.md` | reusable skill | `keep` | Reusable Roger skill artifact |

## Immediate cleanup slices

### Slice A: status-header normalization

Add an explicit first-screen status header to every file marked `keep+status`.
Target wording should be short and literal, for example:

- `Status: historical critique; rationale only`
- `Status: operator runbook; not product spec`
- `Status: bounded side-plan; merge accepted truth back into canonical docs`
- `Status: reusable skill artifact`

### Slice B: archive and history path normalization

Move `archive-later` files into a clearer namespace once links are updated.
Recommended future namespaces:

- `docs/archive/`
- `docs/history/`
- `docs/operator/` or `docs/runbooks/` if the current root gets too crowded

Do not move files piecemeal without updating `AGENTS.md`, canonical links, and
any operator scripts that reference them.

### Slice C: side-plan merge-back

For every file marked `merge-back`:

1. identify the accepted truth that still matters
2. fold that truth into `PLAN_FOR_ROGER_REVIEWER.md` and the relevant support
   contract
3. downgrade the side-plan to historical, or archive it if the canonical docs
   fully replace it

This rule explicitly applies while still **keeping**
`docs/PLAN_FOR_TRUTHFUL_PROVIDER_PARITY_AND_GITHUB_COPILOT_CLI.md` as an active
bounded side-plan for now.

### Slice D: AGENTS authority-map cleanup

After the tree is reclassified and status-normalized:

- simplify the `AGENTS.md` planning-doc list into clearer groups by class
- keep the authority order explicit
- avoid forcing new agents to infer doc class from filename alone

### Slice E: bead-input packet stabilization

Once accepted truth has been merged back:

- keep `docs/BEAD_CREATION_INPUTS.md` aligned with the actual canonical packet
- remove side-plans from the bead-creation packet as soon as the relevant
  canonical plan sections and support contracts are sufficient
- prefer adding one narrow support contract over keeping a broad side-plan alive
  purely for bead-shaping context
- do not let historical rounds creep back into ordinary bead-creation inputs

## Completion criteria for the docs-tree cleanup

The full-tree cleanup is complete only when:

- every Markdown file under `docs/` fits one explicit class
- every file has one explicit action
- non-canonical docs are visibly marked so they cannot be mistaken for current
  product truth
- accepted side-plan content is folded back into canonical/support docs
- bead-creation skills can operate from one bounded authoritative packet without
  depending on historical rounds by default
- the docs root no longer mixes live product truth, historical rationale, and
  operator runbooks without explicit labeling
