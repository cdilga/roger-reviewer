# Critique Round 03 Supplement for Roger Reviewer

This artifact records the formal integration of
[`SUPPLEMENTARY_CHATGPT54PRO_FEEDBACK_ROUND_03.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/SUPPLEMENTARY_CHATGPT54PRO_FEEDBACK_ROUND_03.md)
into the canonical planning set before the next adversarial review round.

## Highest-Value Revisions

### 1. Resolve the hybrid-search contradiction

The prior plan simultaneously said Roger should ship hybrid search from the
first real search slice and deferred search to a late polish phase. That
contradiction is now resolved.

Integrated decision:

- hybrid retrieval remains in scope from the first real Roger search slice
- lexical retrieval is primary
- the semantic corpus is narrow, local-only, and best-effort rather than
  everything-indexed by default
- rollout now moves search/index foundation earlier, while later phases focus on
  hardening promotion, overlays, and evaluation rather than "adding search"

### 2. Formalize Roger as a scoped evidence system

The supplement sharpened the memory model in the right direction. Roger should
not become a global assistant memory or a multi-agent flywheel.

Integrated decision:

- searchable evidence and promoted reusable memory are now treated as different
  layers
- repo is the default scope
- project and org are explicit overlays rather than ambient inheritance
- memory classes, trust rules, and promotion/demotion states are now explicit in
  the canonical plan

### 3. Make indexing non-blocking and generation-aware

The supplement's strongest operational point was that review freshness and index
freshness are not the same thing.

Integrated decision:

- canonical DB writes happen first
- lexical/vector sidecars update in background worker threads inside the same
  local process
- lexical-only degraded mode is explicitly acceptable
- full rebuilds are generation-based and swap atomically from the DB snapshot

### 4. Simplify multi-instance storage semantics

The plan still had ambiguity about whether Roger state would be copied per
reviewer. That ambiguity is now removed.

Integrated decision:

- one canonical Roger store per user profile is now the default
- worktrees are opt-in for isolated execution, conflicting local repo state, or
  elevated mutation-capable flows
- named-instance design now focuses on explicit repo-local resource isolation
  rather than DB-copy synchronization

### 5. Treat TOON as optional prompt packaging only

The supplement made the TOON recommendation narrower and more defensible.

Integrated decision:

- canonical storage and internal IPC remain ordinary rows and compact JSON
- TOON is an optional prompt packer for large, mostly tabular payloads
- enabling TOON now depends on model-specific smoke tests rather than optimism

## Integrated Outcome

These revisions have been merged into:

- [`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md)
- [`BEAD_SEED_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/BEAD_SEED_FOR_ROGER_REVIEWER.md)

The imported beads workspace has not been repolished in this pass. The updated
markdown plan and seed are now the correct baseline for that later work.

## Implications for the Next Adversarial Round

- pressure-test the `project` boundary rule so overlay scope does not become
  ambient org memory
- pressure-test outcome labeling for promotion/demotion and anti-pattern capture
- pressure-test the chosen local embedding model and packaging path without
  expanding v1 scope
- pressure-test the browser-extension packaging/build path so it can remain a
  dependency-light JS/TS exception inside an otherwise Rust-first local runtime
