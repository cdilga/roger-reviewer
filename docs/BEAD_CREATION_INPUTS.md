# Bead Creation Inputs

Status: process support for planning-to-beads convergence and bead-creation
skills.

Purpose:

- define the bounded authoritative packet that bead-creation and bead-polish
  workflows should consume
- prevent bead creation from depending on a scavenger hunt across historical
  briefs and side plans
- define how Roger converges from broad planning docs into a stable bead-input
  artefact set

Authority:

- `AGENTS.md` remains the operational authority
- `PLAN_FOR_ROGER_REVIEWER.md` remains the canonical product and implementation
  plan
- this file defines the planning-to-beads input packet and exclusion rules; it
  does not replace the canonical plan, support contracts, or live beads

## Why this exists

Roger's planning doctrine already prefers one dense canonical plan, narrow
support contracts, and temporary side plans that merge back once accepted.

Bead-creation skills need one more thing:

- an explicit statement of which documents are allowed into the packet that
  generates or reshapes beads

Without that packet, agents drift into one of two bad modes:

- they read too little and create under-specified beads
- they read too much and import stale historical rationale into current
  decomposition

## Core rule

Beads should be created from a bounded authoritative packet for the current
lane, not from the whole docs tree.

The packet should be:

- small enough to fit in context with room for actual reasoning
- authoritative enough to define current product truth
- narrow enough that historical rationale does not masquerade as current scope

## Packet order

Default authority order for bead creation:

1. `AGENTS.md`
2. `docs/PLAN_FOR_ROGER_REVIEWER.md`
3. the relevant support contract(s) and ADRs for the lane
4. `docs/BEAD_SEED_FOR_ROGER_REVIEWER.md`
5. this file
6. one bounded side-plan only if accepted truth has not yet merged back
7. validation docs only when the bead's promise depends on proof shape

Historical critiques, prior reconciliation rounds, and raw archive material are
excluded by default.

## Minimum packet by bead type

### 1. Ordinary implementation bead

Minimum packet:

1. `AGENTS.md`
2. `docs/PLAN_FOR_ROGER_REVIEWER.md`
3. one or more relevant support contracts or ADRs
4. `docs/BEAD_SEED_FOR_ROGER_REVIEWER.md`

Examples of relevant support docs:

- storage or migration work:
  `DATA_MODEL_AND_STORAGE_CONTRACT.md`,
  `STORE_MIGRATION_COMPATIBILITY_AND_OPERATOR_CONTRACT.md`
- harness or continuity work:
  `HARNESS_SESSION_LINKAGE_CONTRACT.md`
- prompt and outcome work:
  `PROMPT_PRESET_AND_OUTCOME_CONTRACT.md`
- robot surface work:
  `ROBOT_CLI_CONTRACT.md`
- search, memory, or recall work:
  `SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md`
- TUI workspace work:
  `TUI_WORKSPACE_AND_OPERATOR_FLOW_CONTRACT.md`,
  `TUI_RUNTIME_SUPERVISOR_POLICY.md`

### 2. UX or surface bead

Minimum packet:

1. `AGENTS.md`
2. `docs/PLAN_FOR_ROGER_REVIEWER.md`
3. surface-specific contract(s)
4. `docs/BEAD_SEED_FOR_ROGER_REVIEWER.md`
5. one active bounded side-plan only if the contract has not yet absorbed the
   accepted truth

Current examples:

- TUI cockpit work:
  `TUI_WORKSPACE_AND_OPERATOR_FLOW_CONTRACT.md`
- unresolved cross-surface reconciliation:
  `ROUND_05_SURFACE_RECONCILIATION_BRIEF.md` only while merge-back remains
  incomplete

### 3. Validation or proof bead

Minimum packet:

1. `AGENTS.md`
2. `docs/PLAN_FOR_ROGER_REVIEWER.md`
3. `docs/TESTING.md`
4. `docs/VALIDATION_INVARIANT_MATRIX.md`
5. `docs/TEST_HARNESS_GUIDELINES.md`
6. any additional validation contract needed for the lane
7. `docs/BEAD_SEED_FOR_ROGER_REVIEWER.md`

### 4. Planning or bead-shaping bead

Minimum packet:

1. `AGENTS.md`
2. `docs/PLAN_FOR_ROGER_REVIEWER.md`
3. this document
4. `docs/DOCS_TREE_INVENTORY_AND_CLEANUP_PLAN.md` when the task is docs
   convergence
5. `docs/BEAD_SEED_FOR_ROGER_REVIEWER.md`

## Exclusion rules

Do not include these in ordinary bead creation unless the task explicitly needs
them:

- historical critique rounds
- historical reconciliation briefs or outcomes
- raw brain dumps
- operator runbooks
- smoke notes
- one-off audit or incident docs
- external exploration notes

They are rationale, evidence, or operations support, not current decomposition
authority.

## Side-plan rule

At most one bounded side-plan should be in the bead-creation packet for a given
lane.

Rules:

- if accepted truth already exists in the canonical plan or a support contract,
  exclude the side-plan
- if a side-plan is still needed to understand current scope, it must be named
  explicitly in the bead packet
- once the side-plan's accepted truth is merged back, remove it from the packet
  and downgrade or archive it

## Round-file rule

Roger should not collapse all `ROUND_XX` documents into one mega-file.

Instead:

- keep round files as historical rationale or bounded reconciliation artefacts
- merge accepted truth back into the canonical plan and support contracts
- downgrade or archive the round file once its active merge-back work is done

The convergence target is not "one rounds file." The convergence target is:

- one canonical plan
- narrow durable support contracts
- one bead seed plus live beads
- a bounded packet for bead creation

## Bead-output expectations

Bead-creation skills should emit or preserve these fields whenever relevant:

- one clear promise
- one primary ownership surface
- one primary proof story
- explicit dependency edges
- relevant invariant ids when the bead changes behavior
- the cheapest truthful validation lane
- named suite, fixture, or artefact expectations when proof is part of the
  promise

### Mandatory capture for search and recall beads

If a bead touches search, recall, memory surfacing, hybrid retrieval, or agent
memory access, it must capture all of the following explicitly rather than
falling back to vague “query” language:

- whether the promise is about planner intent, lexical retrieval, hybrid
  retrieval, memory surfacing, or degraded recovery
- the concrete `query_mode` or the rule for resolving compatibility-ingress
  `auto` into a concrete planned mode before execution
- the allowed `retrieval_mode` values involved, especially whether the bead
  touches `hybrid`, `lexical_only`, or `recovery_scan`
- whether the bead changes `RecallEnvelope` fields such as
  `requested_query_mode`, `resolved_query_mode`, `retrieval_mode`, lane, scope,
  or explainability
- whether the bead is allowed to touch candidate-versus-promoted behavior
- the exact invariant ids and fixtures that defend “no opaque simple-query
  default” and “no recovery mode masquerading as healthy retrieval”

Anti-pattern:

- any bead phrased as “improve search,” “add a simple query,” or “just use
  auto/default search” without the explicit planner and degraded-mode contract
  above is underspecified and should be split or rejected

If a proposed bead cannot name those cleanly, it should be split or the packet
should be clarified before creation.

## Convergence checklist

Roger's planning artefacts are converged enough for stable bead creation when:

- accepted product truth is mostly in `PLAN_FOR_ROGER_REVIEWER.md`
- stable implementation seams are covered by support contracts rather than side
  briefs
- the bead seed reflects current decomposition strategy
- validation proof obligations map to invariants rather than vibe
- only one active side-plan per lane still needs merge-back
- bead-creation skills can operate from a bounded packet without rereading
  historical rounds
