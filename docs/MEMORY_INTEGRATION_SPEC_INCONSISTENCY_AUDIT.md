# Memory Integration Spec Inconsistency Audit

Status: Proposed
Class: bounded side-plan / audit brief
Audience: maintainers reconciling the remaining Roger spec around search, memory, and active-agent operation

Primary references:

- [`PLAN_FOR_ROGER_REVIEWER.md`](./PLAN_FOR_ROGER_REVIEWER.md)
- [`DATA_MODEL_AND_STORAGE_CONTRACT.md`](./DATA_MODEL_AND_STORAGE_CONTRACT.md)
- [`CORE_DOMAIN_SCHEMA_AND_FINDING_FINGERPRINT.md`](./CORE_DOMAIN_SCHEMA_AND_FINDING_FINGERPRINT.md)
- [`SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md`](./SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md)
- [`PROMPT_PRESET_AND_OUTCOME_CONTRACT.md`](./PROMPT_PRESET_AND_OUTCOME_CONTRACT.md)
- [`adr/009-prompt-preset-and-outcome-events.md`](./adr/009-prompt-preset-and-outcome-events.md)
- [`REVIEW_FLOW_MATRIX.md`](./REVIEW_FLOW_MATRIX.md)
- [`VALIDATION_INVARIANT_MATRIX.md`](./VALIDATION_INVARIANT_MATRIX.md)
- [`VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md`](./VALIDATION_MATRIX_AND_FIXTURE_OWNERSHIP.md)
- [`AGENT_ACCESS_AND_IN_SESSION_OPERATION_CONTRACT.md`](./AGENT_ACCESS_AND_IN_SESSION_OPERATION_CONTRACT.md)

---

## Purpose

This audit exists to answer one question:

Where does the remaining spec still drift, under-specify, or contradict itself
around memory integration?

The memory story is now directionally strong:

- searchable evidence and promoted memory are separate
- repo is the default scope
- project and org are explicit overlays
- degraded lexical-only behavior must be truthful
- active agents should not get ambient mutation authority

But those ideas are not yet fully canonicalized across the rest of the spec.

---

## Short answer

The policy is more coherent than the schema, flows, and validation ownership.

The biggest remaining inconsistencies are:

1. outcome-event shape drift across accepted ADR, support contract, and storage
   contract
2. missing canonical ownership for the active-agent memory contract
3. missing flow and invariant coverage for recall envelopes, promotion requests,
   and candidate-versus-promoted behavior
4. a narrower live command surface than some parts of the spec now imply

---

## What is already aligned

These parts are directionally consistent and should be preserved:

- repo-first locality
- explicit `project` and `org` overlays only
- no silent widening from weak repo results to broader memory
- explicit separation between:
  - `evidence_hits`
  - `tentative_candidates`
  - `promoted_memory`
- explicit promotion, demotion, `deprecated`, and `anti_pattern` handling
- truthful degraded lexical-only mode
- approval and posting remaining visibly elevated

This means the core policy is not the main problem anymore. The problem is that
the rest of the spec has not fully absorbed the consequences of that policy.

---

## Confirmed inconsistencies

### I1. Outcome-event schema collision

The outcome-event model is split across incompatible shapes.

`ADR 009` uses:

- `kind`
- event kinds such as `finding_created`, `finding_triage_changed`,
  `finding_draft_created`, `draft_approved`, `draft_invalidated`,
  `draft_posted`, `draft_post_failed`, and `usefulness_labeled`

The implementation-facing support contract uses:

- `event_type`
- a common envelope with `actor_kind`, `actor_id`, `source_surface`, and
  `payload`
- event types such as `finding_emitted`, `finding_state_changed`,
  `draft_materialized`, `approval_state_changed`, and
  `posted_action_recorded`

The storage contract currently models `OutcomeEvent` with a third shape:

- `kind`
- `entity_id`
- `entity_kind`
- `extra_json`

This is not acceptable as-is. It is not merely naming drift; it would produce
schema and event-taxonomy churn if implemented naively.

Default reconciliation:

- treat accepted [`adr/009-prompt-preset-and-outcome-events.md`](./adr/009-prompt-preset-and-outcome-events.md)
  as the naming authority unless the canonical plan is updated
- reconcile the support contract and storage contract downward to one
  implementation-facing envelope
- keep structured payload support, but do not rename event kinds ad hoc

### I2. `source_surface` vocabulary drift

The prompt/outcome support contract uses:

- `cli`
- `tui`
- `extension`
- `external-link`

The storage contract uses:

- `cli`
- `tui`
- `extension`
- `direct`

This should not remain ambiguous.

Default reconciliation:

- canonicalize to one Roger-owned enum
- use `external_link` as the domain name if a new normalized snake_case enum is
  introduced
- treat `direct` and `external-link` as transitional aliases only

### I3. Domain-entity omission for memory

The broader storage contract treats these as canonical aggregates:

- `MemoryItem`
- `MemoryEdge`
- `UsageEvent`

But the narrower core-domain schema omits them from its canonical entity set.

That creates an avoidable ambiguity:

- is memory truly first-class in the domain model
- or only a storage concern

Default reconciliation:

- memory entities should be added explicitly to the narrower domain-schema
  contract
- memory is not optional plumbing anymore; it is part of Roger’s review domain

### I4. Prompt-favorites under-modeling

The prompt contract wants favorites persisted per profile with optional display
ordering.

The storage contract currently models favorites as a boolean on `PromptPreset`.

That is enough for “starred or not,” but not enough for ordered shortcuts or
future scoped favorites.

Default reconciliation:

- if favorites remain in scope for `0.1.0`, give them a small ordering model
- otherwise explicitly downgrade ordered favorites from the support contract

### I5. In-harness command surface implied too broadly

Some flow and planning docs now imply a richer in-session command story than the
current accepted contract and current code actually support.

Current safe truth:

- `roger-help`
- `roger-status`
- `roger-findings`
- `roger-return`

Still only optional follow-on:

- `roger-refresh`
- `roger-clarify`
- `roger-open-drafts`

Default reconciliation:

- keep the tiny safe subset as the actual `0.1.0` supported command surface
- treat clarification and draft-opening as planned follow-ons, not current
  support claims

---

## Missing canonical pieces

### M1. No canonical recall envelope

The new memory and agent-access artefacts now imply a richer recall contract:

- why did this item surface
- which lane did it come from
- what scope owns it
- what invalidation checks still pass
- whether it is safe to cite or only inspect cautiously

No current canonical invariant or flow family owns this envelope.

Default reconciliation:

- add one canonical recall-envelope contract or fold it into the search-memory
  policy document deliberately

### M2. No canonical promotion-request path

The memory policy defines promotion and demotion states, but not yet the
explicit request path for:

- `request_memory_promotion`
- deprecation requests
- anti-pattern marking requests

The new agent-access contract proposes that lane, but no storage, event, or
validation contract owns it yet.

Default reconciliation:

- model promotion requests as explicit Roger-owned state transitions or queue
  items
- do not leave them as prompt-side magic

### M3. Session baseline and prompt baseline are conceptually required but not fully modeled

The plan and new agent-access artefact both imply a stable session baseline and
prompt baseline packet.

Current contracts snapshot prompt invocations correctly, but they do not yet
define:

- a first-class baseline object
- a baseline-change event
- a canonical way for active agents to resolve “what is the current baseline
  versus what are current run modifiers”

Default reconciliation:

- model baseline as a first-class Roger continuity concept, not just a UI hint

---

## Missing flow coverage

### F09 is too narrow

`F09 Search and Recall During Review` is directionally correct but not yet deep
enough.

What it already covers:

- scoped retrieval
- repo-only versus explicit broader overlay
- degraded lexical-only mode
- stale-memory suppression

What it does not yet cover:

- candidate-versus-promoted memory behavior
- recall-envelope explainability
- promotion-request flows
- active-agent use of recall inside a live session

Default reconciliation:

- widen `F09` or add a sibling flow family for active-agent memory access and
  promotion

### No first-class active-agent memory flow family

The plan plus the new `AgentAccess` artefact now define a real active-agent
operating model, but the flow matrix still fragments it across:

- clarification
- search and recall
- in-harness command affordances
- return and dropout

Default reconciliation:

- add one flow family for active in-session agent operation over Roger-owned
  search, memory, clarification, draft request, and return control

---

## Validation ownership gaps

Current invariant coverage is too narrow for the memory program now described.

Current explicit search invariants:

- `INV-SEARCH-001` scope never widens silently
- `INV-SEARCH-002` degraded lexical-only mode remains truthful

What is missing:

- candidate memory never silently behaves like promoted memory
- recall envelope must expose lane, scope, and degraded truth
- promotion requests are auditable and non-mutating until accepted
- active-agent memory access must degrade truthfully across CLI, harness, TUI,
  and optional MCP
- session baseline and prompt baseline must remain visible and stable enough for
  agents to operate safely

Default reconciliation:

- add new invariants rather than stretching `INV-SEARCH-001` and
  `INV-SEARCH-002` beyond usefulness

---

## Sensible defaults and automation posture

The user instruction here is correct: defer to humans only when we cannot
provide sensible defaults or automated mechanisms.

These are sensible defaults now:

### Search and recall

- default to repo scope
- disable `project` and `org` overlays unless explicitly enabled
- keep tentative candidates off normal retrieval unless anchor overlap is high
  or the operator/agent explicitly asks
- degrade to lexical-only truthfully

### Memory lifecycle

- do not auto-promote from search hits alone
- require explicit evidence-backed promotion rules
- demote quickly on contradiction or harm

### Agent surfaces

- `rr --robot` remains the stable machine-readable read/query surface
- harness-native Roger commands remain a tiny ergonomic subset only
- the TUI remains the authoritative mutation, review, and approval workbench
- any future thin MCP facade must mirror Roger core rather than invent policy

### Prompt and session context

- preserve immutable prompt invocation snapshots
- treat baseline versus modifier state as part of Roger continuity, not agent
  folklore

---

## Recommended next reconciliation steps

1. Reconcile the outcome-event taxonomy across ADR, support contract, and
   storage contract.
2. Add memory entities explicitly to the narrower core-domain schema contract.
3. Add a canonical recall-envelope contract or fold it into the search-memory
   contract.
4. Add one flow family for active in-session agent memory use and one for
   promotion-request handling if they remain separate.
5. Add missing invariants for:
   - candidate-versus-promoted behavior
   - recall envelope truth
   - promotion-request auditability
   - active-agent degraded-mode parity

---

## Result

Roger no longer has a vague memory problem.

It has a mostly coherent memory policy that now needs:

- one canonical schema/event reconciliation
- one canonical active-agent crosswalk
- and one stronger validation story

That is a much better place to be.

