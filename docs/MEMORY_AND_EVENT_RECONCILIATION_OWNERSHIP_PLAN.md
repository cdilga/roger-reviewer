# Memory And Event Reconciliation Ownership Plan

Status: merged into canonical/support contracts on 2026-04-14
Class: historical side-plan / merged ownership input
Audience: maintainers resolving memory, outcome-event, and active-agent spec drift before the next bead wave

Primary inputs:

- [`MEMORY_INTEGRATION_SPEC_INCONSISTENCY_AUDIT.md`](./MEMORY_INTEGRATION_SPEC_INCONSISTENCY_AUDIT.md)
- [`UX_SURFACE_AUDIT_FOR_MEMORY_AND_AGENT_ACCESS.md`](./UX_SURFACE_AUDIT_FOR_MEMORY_AND_AGENT_ACCESS.md)
- [`QMD_LIFT_AND_AGENT_MEMORY_RECONCILIATION_BRIEF.md`](./QMD_LIFT_AND_AGENT_MEMORY_RECONCILIATION_BRIEF.md)
- [`AGENT_ACCESS_AND_IN_SESSION_OPERATION_CONTRACT.md`](./AGENT_ACCESS_AND_IN_SESSION_OPERATION_CONTRACT.md)

---

## Purpose

This document does two things:

1. assigns ownership for resolving the confirmed spec inconsistencies
2. recommends the best resolution sequence for each one

The most urgent issue is the outcome-event model discrepancy, because it leaks
into storage shape, prompt capture, active-agent auditability, and later memory
learning.

---

## Ownership model

Ownership here is by surface and authority layer, not by person.

### O1. Spec authority owner

Owns:

- ADRs
- support contracts
- reconciliation briefs
- canonical naming and state-taxonomy choices

Primary files:

- `docs/adr/*.md`
- `docs/*_CONTRACT.md`
- this document and companion audit artefacts

### O2. Domain and storage owner

Owns:

- canonical domain object set
- relational/event shape
- persistence semantics
- migration-safe field decisions

Primary files and code:

- [`DATA_MODEL_AND_STORAGE_CONTRACT.md`](./DATA_MODEL_AND_STORAGE_CONTRACT.md)
- [`CORE_DOMAIN_SCHEMA_AND_FINDING_FINGERPRINT.md`](./CORE_DOMAIN_SCHEMA_AND_FINDING_FINGERPRINT.md)
- `packages/storage`
- `packages/app-core`

### O3. CLI and harness owner

Owns:

- `rr --robot` surface
- harness-native Roger command surface
- truthful dry-run and degraded behavior
- active-agent read/query access

Primary files and code:

- [`ROBOT_CLI_CONTRACT.md`](./ROBOT_CLI_CONTRACT.md)
- [`HARNESS_SESSION_LINKAGE_CONTRACT.md`](./HARNESS_SESSION_LINKAGE_CONTRACT.md)
- [`adr/007-harness-native-roger-command-surface.md`](./adr/007-harness-native-roger-command-surface.md)
- `packages/cli`
- `packages/app-core`

### O4. TUI owner

Owns:

- operator workbench behavior
- recall inspection
- promotion review
- clarification and draft review handoff
- approval visibility

Primary files and code:

- [`TUI_WORKSPACE_AND_OPERATOR_FLOW_CONTRACT.md`](./TUI_WORKSPACE_AND_OPERATOR_FLOW_CONTRACT.md)
- [`TUI_RUNTIME_SUPERVISOR_POLICY.md`](./TUI_RUNTIME_SUPERVISOR_POLICY.md)
- `packages/app-core/src/tui_shell.rs`

### O5. Extension owner

Owns:

- PR-page launcher
- bounded status mirroring
- no-status fallback
- truthful action model for the thin browser surface

Primary files and code:

- [`EXTENSION_PACKAGING_AND_RELEASE_CONTRACT.md`](./EXTENSION_PACKAGING_AND_RELEASE_CONTRACT.md)
- [`ATTENTION_EVENT_AND_NOTIFICATION_CONTRACT.md`](./ATTENTION_EVENT_AND_NOTIFICATION_CONTRACT.md)
- `apps/extension`

### O6. Validation owner

Owns:

- invariant IDs
- flow coverage
- fixture and suite ownership
- proof obligations for all new memory and event claims

Primary files:

- [`REVIEW_FLOW_MATRIX.md`](./REVIEW_FLOW_MATRIX.md)
- [`VALIDATION_INVARIANT_MATRIX.md`](./VALIDATION_INVARIANT_MATRIX.md)
- [`VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`](./VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md)

---

## Highest-priority discrepancy: outcome-event model

### Problem

The spec currently has three incompatible outcome-event shapes:

- ADR 009 event taxonomy and shared fields
- prompt/outcome support contract envelope
- storage contract relational shortcut shape

If left unresolved, this will cause:

- schema churn
- event-emission drift
- broken analytics assumptions
- weaker agent-memory auditability

### Best resolution

Use a blended resolution, not a winner-takes-all rewrite:

1. **Use ADR 009 as the event-kind naming authority.**
   Keep event kinds such as:
   - `finding_created`
   - `finding_triage_changed`
   - `finding_draft_created`
   - `draft_approved`
   - `draft_invalidated`
   - `draft_posted`
   - `draft_post_failed`
   - `usefulness_labeled`
   - `pr_merged`
   - `pr_closed_unmerged`

2. **Use the support contract’s richer envelope fields.**
   Preserve:
   - `actor_kind`
   - `actor_id`
   - `source_surface`
   - structured payload
   - optional `review_run_id`
   - optional `prompt_invocation_id`

3. **Use the storage contract as the relational boundary, but expand it.**
   The `OutcomeEvent` row should no longer be only:
   - `kind`
   - `entity_id`
   - `entity_kind`
   - `extra_json`

   Instead, the best persisted shape is:

   - `id`
   - `kind`
   - `created_at`
   - `review_session_id`
   - `review_run_id` nullable
   - `prompt_invocation_id` nullable
   - `actor_kind` nullable
   - `actor_id` nullable
   - `source_surface`
   - `entity_id` nullable
   - `entity_kind` nullable
   - `payload_json`

4. **Treat `entity_id` and `entity_kind` as indexing aids, not the canonical semantics.**
   The canonical semantics live in:
   - `kind`
   - the common envelope
   - typed payload fields

5. **Remove support-contract renames that fight ADR 009.**
   Specifically:
   - `event_type` should become `kind`
   - support-contract event names like `finding_emitted` and
     `draft_materialized` should be reconciled to ADR 009 event names instead
     of becoming a second taxonomy

### Ownership

- Primary owner: O1 Spec authority owner
- Secondary owner: O2 Domain and storage owner
- Validation owner: O6

### Resolution sequence

1. Reconcile [`PROMPT_PRESET_AND_OUTCOME_CONTRACT.md`](./PROMPT_PRESET_AND_OUTCOME_CONTRACT.md)
   to ADR 009 event names.
2. Expand [`DATA_MODEL_AND_STORAGE_CONTRACT.md`](./DATA_MODEL_AND_STORAGE_CONTRACT.md)
   to the richer persisted envelope.
3. Add the resulting event shape to the next app-core/storage bead.
4. Add validation ownership for event emission and event-schema truth.

---

## Resolution table for the remaining inconsistencies

### R1. `source_surface` vocabulary drift

Problem:

- `external-link` versus `direct`
- current set also under-represents future `harness_command` and `system`
  sources

Best resolution:

- canonicalize one Roger-owned enum:
  - `cli`
  - `tui`
  - `extension`
  - `external_link`
  - `harness_command`
  - `system`
- treat `direct` and `external-link` as migration aliases only

Why this is the best resolution:

- resolves current drift
- absorbs harness-native and system-generated events cleanly
- avoids needing another vocabulary rewrite later

Ownership:

- Primary: O1
- Secondary: O2 and O3

Resolution sequence:

1. update ADR/support/storage docs to the canonical enum
2. define aliasing/migration note once
3. align command and prompt/event capture code later

### R2. Memory entities omitted from the narrower domain schema

Problem:

- `MemoryItem`, `MemoryEdge`, and `UsageEvent` are canonical in storage but not
  in the narrower domain-schema contract

Best resolution:

- add those three entities explicitly to
  [`CORE_DOMAIN_SCHEMA_AND_FINDING_FINGERPRINT.md`](./CORE_DOMAIN_SCHEMA_AND_FINDING_FINGERPRINT.md)
- treat memory as a first-class Roger domain concern, not just a storage side
  table

Why this is the best resolution:

- memory is now core to agent operation and search/recall
- omission creates artificial ambiguity about whether memory is canonical

Ownership:

- Primary: O2
- Secondary: O1

Resolution sequence:

1. add the entities to the canonical entity set
2. add one short scope/ownership note tying them back to the memory policy
3. reflect the same set in future beads and migrations

### R3. Prompt favorites under-modeled

Problem:

- support docs imply ordered favorites
- storage only models a boolean `is_favorite`

Best resolution:

- narrow `0.1.0` to boolean favorites only
- explicitly defer favorite ordering unless a real product need appears

Why this is the best resolution:

- resolves the inconsistency with the least schema churn
- avoids inventing prompt UX complexity that is not central to the memory
  program

Ownership:

- Primary: O1
- Secondary: O2

Resolution sequence:

1. downgrade ordered favorites from the support contract
2. keep `is_favorite` in storage
3. revisit ordering only if a later UX slice truly needs it

### R4. In-harness command surface implied too broadly

Problem:

- some docs imply `roger-clarify`, `roger-open-drafts`, and `roger-refresh` are
  effectively there
- accepted contract and live code only safely support help/status/findings/return

Best resolution:

- narrow all `0.1.0` support claims to the tiny safe subset
- keep the richer commands as planned follow-on capabilities

Why this is the best resolution:

- truthful narrowing is better than aspirational widening
- preserves the safe capability ladder already implied by the code

Ownership:

- Primary: O3
- Secondary: O1 and O6

Resolution sequence:

1. align flow docs and support docs to the tiny safe subset
2. add future beads for `roger-clarify`, `roger-open-drafts`, and
   `roger-refresh`
3. only widen support claims after those commands are real and validated

### R5. No canonical recall envelope

Problem:

- memory policy and agent-access artefacts now imply a richer per-hit recall
  contract
- no canonical contract owns it

Best resolution:

- add a narrow canonical `RecallEnvelope` contract, either:
  - as a new support contract, or
  - as an explicit subsection inside
    [`SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md`](./SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md)

Minimum required fields:

- `scope_bucket`
- `memory_lane`
- `trust_state`
- `degraded_flags`
- `anchor_overlap_summary`
- `explain_summary`
- `citation_posture`

Why this is the best resolution:

- gives CLI/TUI/agent surfaces one shared explanation model
- resolves the biggest missing piece in “right memory at the right time”

Ownership:

- Primary: O1
- Secondary: O2, O3, O4, O6

Resolution sequence:

1. freeze the envelope shape
2. make `rr search --robot` the first required consumer
3. project the same envelope into TUI recall and inspector surfaces
4. add invariants for recall-envelope truth

### R6. No canonical promotion-request path

Problem:

- the policy defines promotion/demotion
- the new agent-access artefact defines a `request_memory_promotion` lane
- no domain object, event, or queue contract owns the request path

Best resolution:

- add a bounded Roger-owned request object such as `MemoryReviewRequest` or
  `PromotionIntent`
- treat agents and prompts as requesters only, never direct mutators

Why this is the best resolution:

- preserves auditability
- preserves explicit human/operator review for high-trust memory changes
- avoids hidden prompt-driven memory mutation

Ownership:

- Primary: O2
- Secondary: O4 and O6

Resolution sequence:

1. define the request object and allowed transitions
2. surface review in TUI
3. add event emission for accepted/rejected promotion actions
4. validate non-mutating request behavior separately from accepted mutation

### R7. Session baseline and prompt baseline under-modeled

Problem:

- active-agent operation needs a stable baseline packet
- current contracts capture prompt invocations but not a first-class baseline
  object

Best resolution:

- model a bounded `SessionBaselineSnapshot` or equivalent continuity object
  attached to session or run state

Minimum contents:

- preset id or baseline id
- resolved baseline prompt digest
- current explicit objective digest
- review mode
- allowed scopes
- active safety posture

Why this is the best resolution:

- gives agents and UIs a stable answer to “what is the current baseline?”
- prevents session context from collapsing into folklore

Ownership:

- Primary: O2
- Secondary: O1, O3, O4

Resolution sequence:

1. freeze baseline fields in the continuity layer
2. define when baseline changes create a new event
3. expose it in robot/TUI session views

### R8. Flow coverage too narrow for the memory program

Problem:

- `F09` covers search and recall only partially
- no flow family owns active-agent memory use or promotion requests

Best resolution:

- widen `F09` to cover recall-envelope truth and candidate-versus-promoted
  behavior
- add a new dedicated flow family for active in-session agent operation over
  search, memory, clarification, draft request, and return control
- add another small flow if promotion review remains distinct

Why this is the best resolution:

- keeps memory work visible at the scenario level
- prevents validation from collapsing back to only lexical/scope tests

Ownership:

- Primary: O6
- Secondary: O1

Resolution sequence:

1. update flow matrix
2. map new flows to fixture/suite ownership
3. only then widen bead acceptance criteria

### R9. Validation coverage too narrow

Problem:

- current search invariants only cover:
  - scope bleed
  - degraded lexical-only mode

Best resolution:

- add new invariants rather than overloading the existing two

Recommended new invariants:

- `INV-SEARCH-003`: recall envelope surfaces lane, scope, and degraded truth
- `INV-SEARCH-004`: candidate memory never silently behaves like promoted
  memory
- `INV-AGENT-001`: active-agent read/query and request lanes degrade truthfully
  across CLI, harness, TUI, and optional MCP
- `INV-CONTEXT-001`: session baseline and prompt baseline remain resolvable and
  stable across dropout, return, and reseed

Why this is the best resolution:

- keeps proof obligations small and explicit
- makes future memory and agent claims mechanically discoverable

Ownership:

- Primary: O6
- Secondary: O1, O2, O3, O4

Resolution sequence:

1. add invariant rows
2. assign fixture and suite ownership
3. shape implementation beads against those invariants

### R10. CLI, TUI, and extension UX drift

Problem:

- CLI naming drift around approval state
- TUI still thinner than its contract
- extension still blurs `awaiting_outbound_approval` into a findings action

Best resolution:

- keep the surfaces intentionally asymmetric:
  - CLI/robot = canonical active-agent read/query plane
  - TUI = canonical operator workbench
  - extension = thin mirror and launcher

Why this is the best resolution:

- aligns with current safe architecture
- avoids trying to make every surface do everything

Ownership:

- CLI/harness: O3
- TUI: O4
- extension: O5
- validation cross-check: O6

Resolution sequence:

1. fix semantic naming drift first
2. reconcile TUI contract versus live shell scope
3. keep extension thin and explicitly non-authoritative

---

## Recommended overall order

1. Resolve the outcome-event model and `source_surface` vocabulary first.
2. Add memory entities to the narrower domain schema.
3. Freeze recall envelope and promotion-request contract.
4. Freeze session baseline context.
5. Reconcile flow coverage and invariants.
6. Reconcile CLI/TUI/extension semantics against those contracts.

This order is deliberate:

- schema and taxonomy first
- memory and agent contract second
- flow and validation ownership third
- UX alignment last

---

## Result

The best reconciliation posture is:

- use ADR 009 for event names
- use the richer support-contract envelope fields
- expand storage to persist the real envelope
- keep repo-first, explicit-overlay memory semantics
- and make CLI, TUI, and extension intentionally asymmetric instead of trying
  to force parity

That resolves the current issues with the least architectural churn and the
highest future usefulness.
