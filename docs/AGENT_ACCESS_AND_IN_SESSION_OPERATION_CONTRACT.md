# Agent Access And In-Session Operation Contract

Status: Proposed
Class: proposed support contract
Audience: maintainers shaping Roger’s active-agent surfaces across CLI, harness, TUI, and future MCP projections

Authority:

- [`AGENTS.md`](../AGENTS.md)
- [`PLAN_FOR_ROGER_REVIEWER.md`](./PLAN_FOR_ROGER_REVIEWER.md)
- [`HARNESS_SESSION_LINKAGE_CONTRACT.md`](./HARNESS_SESSION_LINKAGE_CONTRACT.md)
- [`SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md`](./SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md)
- [`ROBOT_CLI_CONTRACT.md`](./ROBOT_CLI_CONTRACT.md)

If this document conflicts with the canonical plan, the canonical plan wins
until it is deliberately updated.

---

## Purpose

Roger needs one coherent contract for how active agents operate inside a live
review session.

Today the relevant pieces exist, but they are fragmented across:

- session linkage and reseed rules
- robot CLI envelopes
- narrow harness-native commands
- TUI operator grammar
- search and memory policy

This document proposes a single operating contract that keeps those surfaces
aligned.

---

## Core principle

The agent never operates as a free-floating chat thread.

The agent is always operating against a Roger-owned session context with:

- a bound review target
- a current stage and continuity packet
- explicit allowed scopes
- explicit degraded-mode state
- explicit surface capabilities

Roger owns that contract. Individual harnesses and external facades only
project it.

---

## Non-goals

This contract does not authorize:

- automatic GitHub posting
- automatic memory promotion as a hidden side effect
- a daemon-centered architecture
- unconstrained embedding of a full raw harness shell inside Roger
- skills or prompt packs masquerading as stateful access surfaces

---

## Proposed core object: `AgentAccessContext`

Every active in-session agent should be able to resolve a bounded context packet
with at least:

- `review_session_id`
- `review_run_id` when present
- `provider`
- `session_locator_ref`
- `resume_bundle_ref`
- `review_target`
- `active_stage`
- `attention_state`
- `selection_context`
- `allowed_scopes`
- `prompt_baseline_ref`
- `degraded_flags`
- `allowed_capabilities`

Recommended derived fields:

- current anchor summary
- current findings-in-focus
- current draft queue summary
- current retrieval generation metadata
- continuity quality

---

## Capability lanes

Roger should define explicit lanes rather than treating all agent operations as
one undifferentiated “tool access” surface.

### Lane 1: `read_query`

Purpose:

- inspect session state
- inspect findings
- search evidence and memory
- inspect degraded or blocked conditions

Expected outputs:

- machine-readable envelopes
- provenance labels
- lane labels such as `evidence_hits`, `tentative_candidates`,
  `promoted_memory`

### Lane 2: `clarify`

Purpose:

- ask Roger to perform follow-up clarification work against selected findings or
  session context

Rules:

- clarification must stay linked to finding lineage or session lineage
- clarification is not a loophole for arbitrary mutation

### Lane 3: `request_memory_promotion`

Purpose:

- request review of candidate memory for promotion, demotion, deprecation, or
  anti-pattern marking

Rules:

- this creates a request or queue item
- it does not silently mutate durable memory state
- the resulting memory action remains auditable

### Lane 4: `request_draft`

Purpose:

- ask Roger to materialize or refine outbound draft work from current findings

Rules:

- drafts remain Roger-owned domain objects
- draft creation should preserve finding references and review context

### Lane 5: `request_approval`

Purpose:

- expose approval-needed state and create the operator handoff path

Rules:

- the agent may request the handoff
- the agent does not self-approve

### Lane 6: `return_or_dropout_control`

Purpose:

- deliberate dropout to the harness
- deliberate return to Roger
- truthful reseed when continuity is degraded

Rules:

- dropout and return remain explicit modes
- uncertainty must bias to reseed, not pretend continuity

### Lane 7: `post`

Purpose:

- execute already-approved outbound posting

Rules:

- not part of the ordinary agent lane
- remains visibly elevated and operator-authorized through TUI/CLI approval
  flow

---

## Surface roles

### `rr --robot`

Role:

- stable machine-readable read/query and dry-run control surface

Should own:

- status
- sessions
- findings
- search
- review/resume dry-run planning
- bridge and setup inspection where applicable

Should not own:

- final approval
- ambient interactive mutation workflows
- implicit widening of agent capabilities

### Harness-native Roger commands

Role:

- narrow ergonomic continuity affordances inside supported harnesses

Current safe subset:

- `roger-help`
- `roger-status`
- `roger-findings`
- `roger-return`

Possible later additions, only behind separate beads and validation:

- `roger-refresh`
- `roger-clarify`
- `roger-open-drafts`

Rules:

- capability-gated per harness
- must fail truthfully when unsupported
- must never become the only way to access a Roger capability

### TUI

Role:

- authoritative operator workbench

Should own:

- triage
- clarification review
- memory recall inspection
- promotion review
- draft review
- approval
- recovery and invalidation handling

### Thin MCP facade

Role:

- optional later projection for external agents or tool clients

Rules:

- must be a strict facade over the same Roger core contract
- must not invent its own scope or memory policy
- must not become an always-on daemon requirement in steady state

### Skills and prompt packs

Role:

- guide defaults and behavior

Rules:

- useful for prompt baselines and conventions
- never an authority surface
- never a substitute for session-bound state access

---

## Required retrieval envelope for agents

Every agent-facing retrieval response should include:

- `session_id`
- `query`
- `allowed_scopes`
- `outcome`
- `degraded_flags`
- `promoted_memory`
- `tentative_candidates`
- `evidence_hits`
- `warnings`
- `next_actions`

Each hit should carry at least:

- `id`
- `kind`
- `scope_bucket`
- `memory_lane`
- `trust_state`
- `anchor_overlap_summary`
- `explain_summary`

This is the key defense against opaque retrieval behavior.

---

## Prompt baseline and continuity packet

In-session agents should not have to reconstruct Roger context from scratch.

The active continuity packet should preserve:

- launch intent
- resolved prompt preset or baseline
- current stage summary
- unresolved follow-up questions
- selected findings and evidence anchors
- current draft queue summary
- degraded or blocked notes relevant to the next step

This packet should ride through dropout, return, and reseed flows.

---

## Degraded-mode rules

The agent surface must degrade truthfully.

Required rules:

- lexical-only retrieval must say it is lexical-only
- missing or unverified semantic assets must be surfaced explicitly
- unsupported harness-native commands must point to their `rr` fallback
- weak scope or unavailable overlays must not silently widen to broader memory
- reseed-required continuity must not pretend to be direct resume

---

## Recommended staged architecture

1. Create a canonical `AgentAccess` core contract in app-core and storage.
2. Project `rr --robot` from that contract for read/query and dry-run control.
3. Reconcile harness-native command reality with the tiny safe subset Roger
   already actually supports.
4. Make the TUI the real operator workbench for recall, promotion review,
   clarification, draft review, approval, and recovery.
5. Add a thin stdio MCP facade only after the core contract and surface-truth
   rules are stable.
6. Keep skills and prompt packs as behavioral defaults only.

---

## Result

Roger should expose one coherent active-agent story:

- session-bound
- scope-aware
- provenance-rich
- truthfully degraded
- and visibly elevated around mutation and approval

That is the missing architecture layer between “we have some search” and “the
right agent can use the right memory correctly during a real review session.”
