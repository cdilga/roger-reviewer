# QMD Lift And Agent Memory Reconciliation Brief

Status: Proposed
Class: bounded side-plan / reconciliation brief
Audience: Roger maintainers preparing the next search, memory, and agent-surface bead-shaping pass
Scope: concepts to lift from QMD, memory-promotion evolution, and the missing in-session agent operating contract

---

## Why this brief exists

QMD is a strong local retrieval system. Roger already has a more careful policy
story for scope, approval, provenance, and memory promotion, but Roger still
under-specifies how active agents inside a live review session should access
memory and control surfaces.

This brief exists to reconcile those two facts:

- Roger should lift meaningful retrieval ideas from QMD
- Roger should not let QMD become the architecture owner
- the next bead wave should capture the agent-side operating gap explicitly

---

## Short answer

QMD is not a better overall architecture for Roger.

It is a better retrieval reference implementation than Roger currently has.
Roger should copy the retrieval mechanics it needs, reimplement them in
Rust-first Roger terms, and keep Roger-owned authority over:

- session truth
- scope boundaries
- memory promotion and demotion
- approval-sensitive mutation
- degraded-mode behavior

The biggest current gap is not “hybrid search is missing.” The biggest gap is
that Roger still lacks one coherent supported contract for how in-session
agents:

- discover current context
- query evidence versus promoted memory
- request clarification or draft work
- ask for memory promotion
- drop out and return truthfully

---

## Current repo truth forcing this brief

### Search policy is ahead of implementation

Roger already has the right high-level policy in:

- [`SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md`](./SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md)
- [`adr/004-scope-and-memory-promotion-policy.md`](./adr/004-scope-and-memory-promotion-policy.md)
- [`PLAN_FOR_ROGER_REVIEWER.md`](./PLAN_FOR_ROGER_REVIEWER.md)

But the current live implementation still shows the gap:

- `prior_review_lookup` is real and scope-aware, but lexical retrieval is still
  canonical-DB scoring rather than the intended Tantivy-grade engine in
  [`../packages/storage/src/lib.rs`](../packages/storage/src/lib.rs)
- semantic retrieval is present as a gated slice, not yet a fully mature
  always-available retrieval layer in
  [`../packages/storage/src/semantic_embedder.rs`](../packages/storage/src/semantic_embedder.rs)

### Agent control surfaces are fragmented

Roger already has partial pieces:

- durable session/reseed/dropout rules in
  [`HARNESS_SESSION_LINKAGE_CONTRACT.md`](./HARNESS_SESSION_LINKAGE_CONTRACT.md)
- robot-facing read/query envelopes in
  [`ROBOT_CLI_CONTRACT.md`](./ROBOT_CLI_CONTRACT.md)
- narrow harness-native command affordances in
  [`adr/007-harness-native-roger-command-surface.md`](./adr/007-harness-native-roger-command-surface.md)
- a TUI operator grammar in
  [`TUI_WORKSPACE_AND_OPERATOR_FLOW_CONTRACT.md`](./TUI_WORKSPACE_AND_OPERATOR_FLOW_CONTRACT.md)

But the live code is still narrower than that larger story:

- `RogerCommandId` currently exposes only help/status/findings/return in
  [`../packages/app-core/src/lib.rs`](../packages/app-core/src/lib.rs)
- `safe_harness_command_bindings` only exposes the small safe subset, primarily
  for OpenCode, in the same file
- the robot CLI is broader, but still mostly a read/query/docs surface in
  [`../packages/cli/src/lib.rs`](../packages/cli/src/lib.rs)
- the TUI shell is still more scaffold than full operator workbench in
  [`../packages/app-core/src/tui_shell.rs`](../packages/app-core/src/tui_shell.rs)
- the extension still projects `awaiting_outbound_approval` into `show_findings`
  rather than a real draft-queue path in
  [`../apps/extension/src/content/main.js`](../apps/extension/src/content/main.js)

---

## Reconciliation decisions

### D1. QMD remains a reference implementation, never the product authority

Keep `_exploration/qmd` as an exploration target and benchmark source. Do not
make it the canonical store, canonical memory policy, or canonical agent
surface.

### D2. Roger keeps authority over memory semantics

Roger must continue to own:

- explicit scope buckets: `repo`, `project`, `org`
- retrieval lanes: `evidence_hits`, `tentative_candidates`,
  `promoted_memory`
- promotion and demotion states
- provenance labels and contradiction handling

### D3. Retrieval mechanics are worth lifting aggressively

Roger should lift and reimplement:

- typed query planning
- BM25-first lexical retrieval
- hybrid lexical/vector fusion
- chunk selection and rerank-on-best-chunk
- explainable scoring and retrieval provenance
- stronger developer-facing search UX

### D4. Memory recall is a gated surface, not a ranked blob

The right memory at the right time should be determined by:

- locality
- anchor overlap
- trust state
- freshness and invalidation status
- explicit overlay enablement

Not by “global best match wins.”

### D5. Roger needs one coherent in-session agent contract

The next architecture pass must define a single `AgentAccess`-style contract
that makes the surface roles explicit:

- `rr --robot`: machine-readable read/query and dry-run control
- harness-native Roger commands: narrow ergonomic continuity layer only
- TUI: authoritative operator workbench and mutation/approval surface
- thin MCP facade: optional later projection of the same core contract
- skills/prompt packs: defaults and behavioral guidance only

---

## Companion artefacts

This brief intentionally fans out into four deeper companion documents:

1. [`QMD_RETRIEVAL_LIFT_FOR_ROGER_REVIEWER.md`](./QMD_RETRIEVAL_LIFT_FOR_ROGER_REVIEWER.md)
   captures the copy/reimplement/reject retrieval matrix
2. [`MEMORY_PROMOTION_AND_SCOPE_RECALL_EVOLUTION.md`](./MEMORY_PROMOTION_AND_SCOPE_RECALL_EVOLUTION.md)
   captures how Roger should learn over time without ambient memory bleed
3. [`AGENT_ACCESS_AND_IN_SESSION_OPERATION_CONTRACT.md`](./AGENT_ACCESS_AND_IN_SESSION_OPERATION_CONTRACT.md)
   proposes the missing active-agent operating contract
4. [`QMD_AND_AGENT_MEMORY_BEAD_SHAPING_INPUT.md`](./QMD_AND_AGENT_MEMORY_BEAD_SHAPING_INPUT.md)
   translates this material into bead-friendly proof groups

---

## Straightforward path forward

1. Freeze the agent-access contract before adding richer external facades.
2. Replace the current lexical scoring path with the intended query-planned
   lexical engine and explainability layer.
3. Make recall event-driven and locality-gated so the right memories surface
   without turning project/org memory into ambient bleed.
4. Add hybrid retrieval, chunk selection, and reranking as scoped supplements,
   not as new authority layers.
5. Only add a thin MCP facade after the core contract, robot envelopes, and TUI
   mutation boundaries all tell one truthful story.

---

## Result

Roger should aim to be “QMD-grade retrieval inside a Roger-owned review and
memory architecture.”

That means:

- better retrieval than Roger has today
- stricter authority boundaries than QMD provides
- a real active-agent operating model, not just better search internals

