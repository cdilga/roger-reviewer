# QMD And Agent Memory Bead Shaping Input

Status: Proposed
Class: bead/process support
Audience: maintainers preparing the next bead creation or graph-widening pass

Purpose:

- preserve the QMD-lift and agent-memory analysis in bead-friendly form
- prevent the next bead pass from reducing the work to “add hybrid search”
- shape proof groups around truthful product promises

Primary inputs:

- [`QMD_LIFT_AND_AGENT_MEMORY_RECONCILIATION_BRIEF.md`](./QMD_LIFT_AND_AGENT_MEMORY_RECONCILIATION_BRIEF.md)
- [`QMD_RETRIEVAL_LIFT_FOR_ROGER_REVIEWER.md`](./QMD_RETRIEVAL_LIFT_FOR_ROGER_REVIEWER.md)
- [`MEMORY_PROMOTION_AND_SCOPE_RECALL_EVOLUTION.md`](./MEMORY_PROMOTION_AND_SCOPE_RECALL_EVOLUTION.md)
- [`AGENT_ACCESS_AND_IN_SESSION_OPERATION_CONTRACT.md`](./AGENT_ACCESS_AND_IN_SESSION_OPERATION_CONTRACT.md)

---

## Shaping rules

### R1. Shape proof groups, not package buckets

These beads are about user-visible and agent-visible truth:

- what retrieval Roger actually has
- when memory is allowed to surface
- what active agents can actually do
- which surfaces own approval-sensitive state

Do not reduce them to “storage work”, “CLI work”, or “MCP work” without naming
the defended promise.

### R2. Contract-first before facade-first

Do not add a richer external MCP-style facade before:

- the core agent-access contract exists
- retrieval envelopes are provenance-rich
- degraded behavior is aligned across CLI, harness, and TUI

### R3. Search quality and memory policy are separate proof stories

QMD-inspired retrieval improvements and Roger memory-promotion rules should not
be collapsed into one oversized bead.

### R4. Keep mutation elevated

Agent access beads must not let search or MCP work blur into approval or
posting authority.

---

## Required bead groups

### G1. Agent access core contract

Promise:

- Roger has one coherent in-session agent contract for read/query, clarification,
  memory requests, draft requests, approval handoff, and dropout/return control

Own:

- core `AgentAccess` object model
- session context packet
- retrieval envelope shape
- capability lanes

Primary proof:

- unit and integration coverage over context resolution and capability gating

### G2. Harness and robot surface reconciliation

Promise:

- CLI robot surfaces and harness-native commands tell one truthful story

Own:

- reconcile current `roger-help/status/findings/return` reality
- define truthful fallback from harness commands to `rr`
- align docs/help/router behavior

Primary proof:

- integration tests for command routing and truthful unsupported-path fallback

### G3. Lexical engine uplift and query planning

Promise:

- Roger has a real planned lexical search layer rather than ad hoc DB scoring

Own:

- query-plan object
- lexical field boosts
- scope partitioning in the lexical layer
- explainable lexical result metadata

Primary proof:

- integration tests over repo-first retrieval, scope fences, and explain output

### G4. Hybrid retrieval, chunk selection, and rerank

Promise:

- Roger gains bounded hybrid retrieval and chunk-aware reranking without
  changing authority or degraded-mode truth

Own:

- bounded semantic candidate path
- fusion logic
- chunk selection
- rerank fallback rules

Primary proof:

- integration tests proving lexical-only degradation, fusion behavior, and
  deterministic explainability

### G5. Memory promotion and scope-recall gates

Promise:

- Roger learns over time without ambient cross-scope pollution

Own:

- event-driven invalidation triggers
- memory surfacing gates
- promotion and demotion request flow
- future explicit overlay posture for broader memory

Primary proof:

- integration tests for candidate-versus-promoted behavior, harmful demotion,
  contradiction handling, and overlay blocking

### G6. TUI agent workbench and operator handoff

Promise:

- the TUI becomes the real operator surface for recall inspection, clarification,
  promotion review, draft review, approval, and recovery

Own:

- recall inspection views
- clarification and composer linkage
- promotion review or queue path
- draft and approval handoff visibility

Primary proof:

- integration or bounded workflow tests plus manual smoke for the mutation
  boundary and recovery path

### G7. Optional thin MCP facade

Promise:

- Roger may expose an external tool-facing projection without creating a second
  authority center

Own:

- strict facade over the core `AgentAccess` contract
- no independent policy
- no daemon requirement in steady state

Primary proof:

- integration tests proving parity with the core envelopes and truthful degraded
  behavior

This group should remain explicitly optional until G1 through G6 are credible.

### G8. Validation and benchmark harness

Promise:

- Roger can measure itself against the current retrieval baseline and the QMD
  reference posture without widening support claims dishonestly

Own:

- retrieval benchmark fixtures
- explainability assertions
- degraded-mode parity checks
- agent-surface parity checks across CLI, harness, TUI, and optional MCP

Primary proof:

- validation suites and named benchmark fixtures, not ad hoc screenshots or
  manual claims

---

## Recommended implementation order

1. G1 Agent access core contract
2. G2 Harness and robot surface reconciliation
3. G3 Lexical engine uplift and query planning
4. G5 Memory promotion and scope-recall gates
5. G4 Hybrid retrieval, chunk selection, and rerank
6. G6 TUI agent workbench and operator handoff
7. G8 Validation and benchmark harness
8. G7 Optional thin MCP facade

This order keeps the authority story ahead of the facade story.

---

## Anti-patterns for bead shaping

Do not create beads that:

- say only “add QMD-like search” without naming the product promise
- merge retrieval internals, memory promotion, and MCP into one oversized task
- imply all harnesses have the same in-session command support
- let robot or MCP work become implicit approval/posting access
- widen support claims before the degraded and fallback stories are proven

---

## Result

If the next bead pass follows this input, Roger should gain:

- a stronger retrieval stack
- a clearer memory-learning model
- a real active-agent operating contract
- and a truthful path for future MCP-style access without architectural drift

