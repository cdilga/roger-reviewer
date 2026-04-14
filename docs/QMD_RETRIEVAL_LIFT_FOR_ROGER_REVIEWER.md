# QMD Retrieval Lift For Roger Reviewer

Status: Proposed
Class: bounded side-plan / retrieval reference extraction
Audience: maintainers shaping Roger search, indexing, and agent-facing retrieval beads

Authority:

- [`AGENTS.md`](../AGENTS.md) and
  [`PLAN_FOR_ROGER_REVIEWER.md`](./PLAN_FOR_ROGER_REVIEWER.md) remain canonical
- this document captures concepts to lift from `_exploration/qmd`
- QMD code is more trustworthy than the QMD README when they disagree

---

## Purpose

Roger needs a durable extraction of what is worth lifting from QMD so later bead
shaping does not reduce the exercise to “add hybrid search.”

The goal is narrower and more useful:

- copy QMD’s strong retrieval mechanics
- reimplement them inside Roger’s Rust-first, scope-aware architecture
- reject the parts that would flatten Roger’s authority boundaries

---

## Core stance

Roger should borrow QMD’s retrieval posture, not QMD’s product shape.

QMD is strong at:

- typed query handling
- BM25-first retrieval
- local vector lookup
- fusion and reranking
- chunk selection
- explainability
- developer-facing search ergonomics

QMD is not designed to own:

- review-session truth
- approval workflows
- scope-gated memory promotion
- provenance buckets across repo/project/org overlays

---

## Lift matrix

### Query model

Copy:

- typed query intent such as lexical, vector, and HyDE-style expansion
- explicit short-circuiting when lexical evidence is already strong
- query planning rather than “search everything with one heuristic”

Reimplement:

- a Rust `SearchPlan` or equivalent with Roger-specific fields for:
  - active scope set
  - session identity
  - anchor set
  - trust floor
  - candidate-versus-promoted allowances
  - degraded semantic flags

Reject:

- treating query expansion as an authority layer
- unscoped retrieval plans that can silently widen to broader memory

### Lexical retrieval

Copy:

- BM25-first posture
- strong field-aware lexical ranking
- query validation and deterministic execution planning

Reimplement:

- Tantivy-backed lexical retrieval partitioned by Roger scope
- field boosts for anchors Roger already cares about:
  - file path
  - symbol
  - finding class
  - policy source
  - session summary
  - note title or summary

Reject:

- a flat all-corpus lexical layer that hides provenance
- treating SQLite substring scoring as the long-term lexical contract

### Vector retrieval

Copy:

- precomputed query/document vector strategy
- local-only vector lookup
- bounded semantic corpus instead of embedding everything available

Reimplement:

- a Rust-native vector sidecar compatible with Roger’s explicit asset policy
- semantic retrieval only over the curated corpus Roger already names:
  - promoted semantic and procedural memory
  - accepted findings and session summaries
  - repo docs, ADRs, and policy excerpts
  - compact commit and issue summaries

Reject:

- raw codebase embedding as the default memory substrate
- Node/Bun native-module dependence in Roger’s critical path

### Fusion

Copy:

- weighted fusion across lexical and semantic candidate lists
- bounded candidate caps
- deterministic tie-breaks

Reimplement:

- Roger-owned fusion that preserves separate result buckets when needed:
  - `promoted_memory`
  - `tentative_candidates`
  - `evidence_hits`
- lexical-biased weighting with explicit scope bias:
  - `repo` beats `project`
  - `project` beats `org`

Reject:

- collapsing repo, project, and org results into one anonymous score stream

### Chunking

Copy:

- content-aware chunk boundaries
- best-chunk selection before reranking
- structure-sensitive chunking rather than arbitrary fixed windows

Reimplement:

- chunking tuned for Roger evidence and memory material:
  - code evidence anchors
  - ADR sections
  - policy excerpts
  - finding summaries
  - compact historical notes
- tree-sitter or equivalent only where it materially improves code symbol
  boundaries

Reject:

- chunk strategies that add runtime or dependency burden without helping review
  anchors

### Reranking

Copy:

- rerank only the bounded top set
- rerank on best chunk, not entire documents
- blended display of retrieval and rerank contributions

Reimplement:

- a bounded local reranker behind an explicit feature/capability layer
- deterministic fallback when rerank is unavailable
- no change in policy semantics when rerank is absent

Reject:

- turning rerank into a hidden oracle that silently overrides Roger’s trust,
  scope, or approval boundaries

### Explainability

Copy:

- surfaced retrieval reasons
- explicit lexical/semantic/rerank contribution reporting
- operator-visible “why this surfaced” outputs

Reimplement:

- machine-readable provenance envelopes for `rr search --robot`
- human-readable explain mode in CLI/TUI
- per-hit fields such as:
  - scope bucket
  - memory lane
  - lexical score
  - semantic contribution
  - rerank contribution
  - anchor overlap summary
  - degraded-mode notes

Reject:

- prose-only explanations
- retrieval surfaces that force users or agents to infer why a result appeared

### Indexing lifecycle

Copy:

- explicit reindex/update/cleanup posture
- generation-aware rebuilds
- durable local status visibility

Reimplement:

- Roger rebuilds from canonical SQLite-family state only
- separate lexical/vector generations with atomic swap
- dirtiness tracking aligned to Roger events:
  - finding changes
  - note edits
  - session checkpoints
  - commit/rebase changes
  - policy changes
  - scope binding changes
  - tokenizer/embedding/schema changes

Reject:

- treating lexical/vector sidecars as authoritative
- global background services as the architecture center

### Developer-facing search UX

Copy:

- toolable local search experience
- `--json` plus explicit explainability
- fast status and inspection affordances

Reimplement:

- Roger-specific UX that is session-aware and approval-aware:
  - `rr search`
  - `rr findings`
  - `rr status`
  - `rr sessions`
  - later optional richer inspection commands over the same core contract

Reject:

- making search the posting or mutation surface
- requiring an always-on daemon in steady state

---

## Important QMD caution

QMD’s README is useful for orientation, but the code is the primary truth source.
Its schema description has already drifted relative to the live code.

Roger should therefore treat `_exploration/qmd/src/*.ts` as the primary source
for lift decisions and use the README only for workflow orientation.

---

## Recommended first Roger retrieval slice

1. Introduce a real Roger query-planning layer instead of direct ad hoc search
   calls.
2. Replace the current long-term lexical placeholder path with the intended
   lexical engine and field boosts.
3. Preserve Roger’s existing three retrieval lanes and scope buckets.
4. Add explainability before adding deeper semantic cleverness.
5. Add bounded hybrid fusion and best-chunk reranking only after the lexical and
   provenance layers are truthful.

---

## Result

The right target is not “Roger with QMD glued on.”

The right target is:

- QMD-grade retrieval mechanics
- Roger-owned scope and memory semantics
- a search surface that helps both humans and in-session agents without
  becoming the authority layer itself

