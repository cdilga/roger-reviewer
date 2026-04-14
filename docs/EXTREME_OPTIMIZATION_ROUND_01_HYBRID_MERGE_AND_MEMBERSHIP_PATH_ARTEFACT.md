# Extreme Optimization Round 01: Hybrid Merge And Membership Path Artefact

Status: planning artefact for the next measured optimisation wave.

Authority:
- `AGENTS.md`
- `docs/skills/ROGER_EXTREME_SOFTWARE_OPTIMIZATION.md`
- `docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md`
- `docs/DATA_MODEL_AND_STORAGE_CONTRACT.md`

Primary code surfaces:
- `packages/storage/src/lib.rs`
- `packages/storage/tests/prior_review_lookup_smoke.rs`
- `packages/storage/tests/prior_review_lookup_perf.rs`
- `packages/cli/src/lib.rs`

## 1. Scope

This artefact answers one bounded question:

- how should Roger optimise the in-memory hybrid merge path of `prior_review_lookup` without changing observable output semantics or drifting away from the repository's search and memory contracts

The focus is deliberately narrow:

- repeated membership checks using `iter().any(...)`
- evidence/promoted/tentative merge mechanics
- semantic score map construction and propagation
- sort cost and tie-breaking invariants
- whether some of this work belongs upstream in the semantic/query side or downstream in the machine-side projection rather than inside the merge loop itself

This artefact is not a broad search redesign. It is a planning and correctness document for the next measured optimisation slice.

## 2. Current measured context

Main-thread measurements already established a representative release-mode hybrid workload through `packages/storage/tests/prior_review_lookup_perf.rs`.

Known measurements from the current round:

- baseline before recent optimisations: `p50=4843us`, `p95=5202us`, `p99=5372us`, `mean=4823.9us`
- after recent landed optimisations: about `p50=2702us`, `p95=2953us`, `p99=3176us`, `mean=2743us`

Already-landed levers:

- cached prepared statements
- unstable lane sorts with deterministic comparators
- one-pass semantic candidate partitioning into evidence vs memory score maps

This artefact assumes those changes already exist in the current tree and asks what should come next.

## 3. Current end-to-end flow

### 3.1 Storage ingress

`PriorReviewLookupQuery` in `packages/storage/src/lib.rs:860-870` carries:

- `scope_key`
- `repository`
- `query_text`
- `limit`
- `include_tentative_candidates`
- scope widening flags
- `semantic_assets_verified`
- `semantic_candidates`

`PriorReviewLookupResult` in `packages/storage/src/lib.rs:914-921` returns three lane-partitioned vectors rather than one unified result set:

- `evidence_hits`
- `promoted_memory`
- `tentative_candidates`

### 3.2 `prior_review_lookup` control flow

The main function is `packages/storage/src/lib.rs:2440-2668`.

It does the following, in order:

1. Resolve `scope_bucket` from `scope_key`.
2. Fail closed for disabled `project:` or `org:` overlays by returning empty lane vectors.
3. Normalize `query_text` by trimming, lowercasing, and then taking only the first whitespace-delimited token as the lexical query.
4. Clamp `limit` to `1..=100`.
5. Inspect lexical and semantic sidecar state and build degraded reasons.
6. Run three lexical queries:
   - `lookup_evidence_hits(...)`
   - `lookup_memory_hits(..., ["established", "proven"], ...)`
   - optional `lookup_memory_hits(..., ["candidate"], ...)`
7. If semantic retrieval is operational:
   - build two semantic score maps with `semantic_scores_by_target(...)`
   - append semantic-only evidence hits not already present lexically
   - append semantic-only memory hits not already present lexically
   - propagate semantic scores onto all lane members
   - recompute `fused_score`
   - sort each lane independently
8. Otherwise:
   - zero semantic scores
   - set `fused_score` from lexical score only
9. Return the three lane vectors plus mode and degraded reasons.

### 3.3 Lexical helpers

`lookup_evidence_hits` lives at `packages/storage/src/lib.rs:3357-3433`.

Important properties:

- filters to the target repository via `json_extract(rs.review_target, '$.repository')`
- lexical score is a weighted sum over fingerprint, title, and normalized summary
- ordering in lexical-only mode is:
  - `lexical_score DESC`
  - `rs.updated_at DESC`
  - `f.rowid DESC`

`lookup_memory_hits` lives at `packages/storage/src/lib.rs:3435-3531`.

Important properties:

- filters by `scope_key` and allowed `state`
- lexical score is a weighted sum over normalized key, statement, anchor digest, plus state bonus
- ordering in lexical-only mode is:
  - `lexical_score DESC`
  - `updated_at DESC`
  - `rowid DESC`

### 3.4 Semantic helpers

`semantic_scores_by_target` lives at `packages/storage/src/lib.rs:4192-4207`.

Important properties:

- partitions candidates into evidence and memory maps
- normalizes each float score to integer milli-units with `semantic_score_to_milli`
- collapses duplicate candidate ids by keeping the max semantic score per id

### 3.5 Semantic-only hydration

Hybrid mode uses one-by-one fetch helpers:

- `evidence_hit_by_id` at `packages/storage/src/lib.rs:3533-3578`
- `memory_hit_by_id` at `packages/storage/src/lib.rs:3580-3609`

These helpers do not return lexical scoring context. They hydrate the row and then the merge path assigns:

- `semantic_score_milli`
- `fused_score`

with lexical score defaulting to `0` for semantic-only rows.

### 3.6 CLI search projection

`handle_search` lives at `packages/cli/src/lib.rs:3323-3442`.

Current operator CLI behavior matters because it shows where merge work is and is not actually consumed today:

- `rr search` currently forces `include_tentative_candidates = false`
- `rr search` currently forces `semantic_assets_verified = false`
- `rr search` currently passes `semantic_candidates = Vec::new()`
- so the shipped operator CLI path is lexical-only today

The CLI then:

1. flattens only `evidence_hits` and `promoted_memory` into one `items` vector
2. omits `tentative_candidates`
3. sorts that combined vector only by numeric `score` descending
4. truncates globally afterward

This is a downstream projection, not the storage merge path itself, but it strongly affects what can be simplified safely and where later optimisation should live.

## 4. Concrete code map

### Storage types and entrypoint

- `packages/storage/src/lib.rs:860-870` — `PriorReviewLookupQuery`
- `packages/storage/src/lib.rs:873-921` — retrieval mode and lane result structs
- `packages/storage/src/lib.rs:2440-2668` — `prior_review_lookup`

### Hybrid merge and membership hotspot

- `packages/storage/src/lib.rs:2553-2593`

This is the core hotspot for this artefact:

- `evidence_hits.iter().any(...)`
- `promoted_memory.iter().any(...)`
- `tentative_candidates.iter().any(...)`
- semantic-only hydration and lane assignment

### Score propagation and lane sorting

- `packages/storage/src/lib.rs:2595-2634`

This block:

- re-reads the score maps for every row
- recomputes fused scores
- sorts each lane independently

### Lexical query helpers

- `packages/storage/src/lib.rs:3357-3433` — evidence lexical query
- `packages/storage/src/lib.rs:3435-3531` — memory lexical query

### Semantic-only by-id hydration

- `packages/storage/src/lib.rs:3533-3578` — evidence by id
- `packages/storage/src/lib.rs:3580-3609` — memory by id

### Semantic candidate collapse

- `packages/storage/src/lib.rs:4192-4207` — `semantic_scores_by_target`

### Current operator-facing search projection

- `packages/cli/src/lib.rs:3350-3413`

### Behavioural tests

- `packages/storage/tests/prior_review_lookup_smoke.rs:74-170`
- `packages/storage/tests/prior_review_lookup_smoke.rs:174-273`
- `packages/storage/tests/prior_review_lookup_smoke.rs:278-300`

### Current manual perf harness

- `packages/storage/tests/prior_review_lookup_perf.rs:209-276`

## 5. Current invariants and why they matter

These are the invariants a safe optimisation must preserve.

### 5.1 Lane partitioning is semantically meaningful

The search contract explicitly distinguishes:

- `evidence_hits`
- `promoted_memory`
- `tentative_candidates`

See `docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md:172-217`.

This means lane assignment is not just an implementation detail. It affects:

- `memory_lane`
- `citation_posture`
- `surface_posture`
- whether a candidate is shown as ordinary recall or review-only material

Any optimisation that collapses lanes or lets the semantic hook decide lane membership would violate the contract unless explicitly redesigned.

### 5.2 Semantic duplicate collapse is max-by-id

`semantic_scores_by_target` keeps the maximum semantic score for each target id. That is current behavior, not an incidental artifact.

Safe optimisation must preserve:

- one semantic score per target id
- winner is `max(score_milli)`

### 5.3 Fused score formula is exact

`fused_score(lexical, semantic)` is currently:

- `lexical_score * 10 + semantic_score_milli`

That makes lexical score the dominant term and semantic score a refinement within the lexical bucket.

Safe optimisation must preserve:

- integer arithmetic
- saturation behavior
- exact ordering implications

### 5.4 Hybrid mode changes tie-breaking

This is easy to miss and materially important.

In lexical-only mode, ordering comes from SQL:

- evidence: `lexical_score DESC, rs.updated_at DESC, f.rowid DESC`
- memory: `lexical_score DESC, updated_at DESC, rowid DESC`

In hybrid mode, each lane is re-sorted in Rust by:

- `fused_score DESC`
- `lexical_score DESC`
- id ascending

Specifically:

- evidence uses `finding_id`
- memory uses `memory_id`

That means hybrid mode is not just lexical-only plus semantic bonuses. It also replaces recency-based tie-breaking with deterministic id-based tie-breaking.

This matters because:

- any top-k or streaming optimisation must preserve mode-specific ordering
- "same results" means same lane ordering, not just same membership

### 5.5 Current CLI global ordering is weaker than storage lane ordering

The current CLI projection sorts flattened `items` only by numeric score and leaves ties to stable insertion order.

Consequences:

- cross-lane ties are effectively resolved by "evidence first, then promoted memory" because evidence items are appended first
- storage lane ordering leaks into CLI ordering
- current CLI search does not exercise hybrid path today, but any future hybrid CLI path will inherit this surface behavior unless fixed

This does not block storage optimisation, but it means storage should not be bent to preserve accidental CLI tie behavior. The correct long-term target is the canonical `RecallEnvelope`, not current CLI quirks.

## 6. Where the remaining work actually is

For the merge path specifically, the remaining cost is concentrated in four shapes:

1. repeated linear membership checks against growing vectors
2. per-row map lookups during semantic score propagation
3. full vector re-sorts after semantic enrichment
4. lane-local merge mechanics that know too much about downstream shaping

The already-landed one-pass `semantic_scores_by_target` change removed one unnecessary pass over the semantic candidate list. The next wins are now mostly about avoiding repeated scans over the result vectors themselves and about simplifying the merge state representation.

## 7. Candidate optimisation levers

Scoring uses the skill's formula:

- `Impact`: 1-5
- `Confidence`: 1-5
- `Effort`: 1-5
- rank = `(Impact × Confidence) / Effort`

### Ranked summary

| Rank | Candidate | Impact | Confidence | Effort | Score |
|---|---|---:|---:|---:|---:|
| 1 | Add set-backed membership for existing ids per lane | 3 | 5 | 1 | 15.0 |
| 2 | Build lane-local id-to-index maps and propagate scores in place | 3 | 4 | 2 | 6.0 |
| 3 | Use exact top-k selection per lane after semantic enrichment | 2 | 3 | 2 | 3.0 |
| 4 | Push candidate collapse and bounded pruning upstream into the semantic hook input | 3 | 2 | 3 | 2.0 |
| 5 | Redesign storage to emit one globally ordered envelope instead of lane vectors | 4 | 2 | 5 | 1.6 |

The first two are safe isomorphic optimisation levers. The last two are not.

## 8. Candidate analysis

### Candidate 1: set-backed membership for existing ids per lane

Shape:

- build `HashSet<&str>` or `HashSet<String>` for:
  - existing evidence ids
  - existing promoted memory ids
  - existing tentative memory ids
- update these sets as semantic-only hits are appended
- replace `iter().any(...)` membership checks in `packages/storage/src/lib.rs:2557-2579`

Expected win class:

- low-to-moderate CPU win on hybrid workloads with many semantic candidates and medium-sized lane vectors
- specifically attacks repeated `O(existing_hits)` scans inside the semantic loop

Blast radius:

- narrow
- isolated to the hybrid merge block
- no schema changes
- no query changes

Correctness oracle:

- exact same lane membership
- exact same score fields
- exact same lane ordering after final sort
- exact same degraded reasons and mode

Evidence needed before implementation:

- same manual perf harness already used in this round
- optionally add a targeted test with duplicate semantic ids and overlapping lexical hits

Why it is safe:

- this replaces only the membership test implementation
- it does not change fetch rules, scoring rules, or sorting rules

Recommended status:

- best next measured implementation slice in this specific hotspot

### Candidate 2: build lane-local id-to-index maps and propagate scores in place

Shape:

- after lexical retrieval, build:
  - evidence id -> index
  - promoted memory id -> index
  - tentative memory id -> index
- when semantic candidates arrive:
  - update semantic score directly on known lexical hits
  - append semantic-only hits and record their index immediately
- eliminate the later full "map lookup for every row" passes where possible

Expected win class:

- moderate CPU win
- avoids:
  - repeated vector membership scans
  - repeated hash map lookups in three post-processing loops

Blast radius:

- still local to `prior_review_lookup`
- more logic churn than Candidate 1
- touches both merge and score propagation structure

Correctness oracle:

- exact same lane membership
- exact same `semantic_score_milli`
- exact same `fused_score`
- exact same per-lane ordering

Evidence needed before implementation:

- same manual perf harness
- a lane-equivalence test comparing old and new output on a duplicate-heavy candidate set would be ideal if a temporary alternate path is kept during development

Why it is safe:

- if implemented carefully, it only changes the bookkeeping representation
- it does not require changing query inputs or output contracts

Caution:

- do not fold this together with batch hydration or unified envelope redesign in one diff

Recommended status:

- second optimisation pass after Candidate 1, not before

### Candidate 3: exact top-k selection per lane after semantic enrichment

Shape:

- if a lane exceeds `limit`, use an exact `select_nth_unstable_by` or bounded heap approach with the current comparator
- then sort only the retained prefix exactly

Expected win class:

- potentially small on current measured workload
- could matter more if semantic candidate sets grow materially beyond current bounds

Blast radius:

- local to sorting
- requires care because lane lengths can temporarily exceed `limit` after semantic-only additions

Correctness oracle:

- exact same first `limit` items in the exact same order for each lane

Evidence needed before implementation:

- profiling or trace data showing lane sort cost is still material after Candidate 1 and Candidate 2
- datasets where candidate set size is significantly larger than returned lane size

Why it is not first:

- current `limit` is capped at 100, so the sort universe is not unbounded
- there is measurable sort cost, but it is not yet obvious that this beats the simpler bookkeeping wins

Recommended status:

- hold until after the membership and propagation passes are measured

### Candidate 4: push candidate collapse and bounded pruning upstream into the semantic hook input

Shape:

- upstream semantic machinery could pre-collapse duplicate ids and optionally send only the best candidate ids per target kind or per provisional lane budget

Expected win class:

- moderate if current semantic candidate lists are much wider than what storage actually needs

Blast radius:

- crosses the storage boundary
- changes the semantics of the semantic candidate input contract
- requires coordination with the semantic sidecar hook or machine-side orchestration

Correctness oracle:

- same storage output when the pruned candidate set still contains every id that could affect the top returned rows

Evidence needed before implementation:

- real distributions of semantic candidate counts from future semantic retrieval paths, not only the perf harness
- proof that pruning cannot exclude an id that should surface after fusion

Why this is risky:

- the canonical store is the source of truth, not the semantic sidecar
- the sidecar should not decide durable memory lane or final recall semantics

Recommended status:

- design-only for now

### Candidate 5: redesign storage to emit one globally ordered `RecallEnvelope`

Shape:

- replace three lane vectors with one canonical envelope list carrying lane metadata

Expected win class:

- could simplify downstream projection and remove duplicate sort/remix work

Blast radius:

- very large
- storage contract change
- CLI/TUI/worker projection change
- test contract change

Correctness oracle:

- explicitly new output contract, not isomorphic to current behavior

Evidence needed before implementation:

- accepted product decision, not just profiling

Why this is not a performance pass:

- this is an architectural redesign
- it may be the right long-term move, but it is not an "extreme optimisation" lever in the narrow isomorphic sense

Recommended status:

- do not package this as a perf patch

## 9. Explicit treatment of key questions

### 9.1 Set-backed membership

This is the clearest safe optimisation still left in the merge block.

Observed current behavior:

- evidence semantic merge uses `evidence_hits.iter().any(...)`
- memory semantic merge uses `promoted_memory.iter().any(...)` and `tentative_candidates.iter().any(...)`

Problem:

- this re-scans vectors for every semantic id
- complexity becomes roughly proportional to `semantic_ids × current_lane_size`

Recommendation:

- introduce explicit ephemeral membership sets
- update them immediately when a semantic-only hit is appended

This keeps authority and lane assignment in storage while removing wasted repeated scans.

### 9.2 Alternate merge shapes

There are three plausible merge shapes.

Current shape:

- vector-first
- membership by scan
- score propagation by later map lookup

Safer alternate shape:

- vector plus id-index map per lane
- direct mutation when semantic score arrives
- final sort unchanged

Aggressive alternate shape:

- one temporary map of `id -> row state` followed by vector materialization

Recommendation:

- stop at the vector plus id-index-map shape
- do not jump to a fully map-first redesign yet

Reason:

- it preserves the existing result structures
- it minimizes proof burden
- it is easier to rollback

### 9.3 Semantic score propagation

Current propagation is correct but mechanically redundant:

- semantic-only appended rows are assigned scores immediately
- then every row in every lane is visited again and score-mapped from `HashMap`

This is safe but not minimal.

Recommendation:

- fold score propagation into the id-index bookkeeping pass
- for lexical hits already present, set semantic fields directly once
- for semantic-only hits, set fields on insertion

That removes repeated `HashMap::get(...)` work and clarifies the merge logic.

### 9.4 Partial streaming

Streaming is not the right next move.

Why:

- storage returns lane vectors, not a global stream
- exact per-lane ordering still requires full comparator evaluation
- mode-specific ordering differs between lexical-only and hybrid
- current limit is bounded to 100

A streaming or heap-based design only becomes clearly worthwhile if:

- semantic candidate widths grow much larger in real workloads
- or the API changes to demand only a small exact prefix

For the current repository state, streaming adds proof complexity faster than it removes cost.

### 9.5 Can some concern move upstream or machine-side?

Yes, but only part of it.

Should stay in storage:

- lane authority
- memory state interpretation
- scope checks
- final fusion and final lane ordering

May move upstream later:

- duplicate candidate collapse
- bounded candidate pruning, if proven safe
- semantic candidate ordering or pre-grouping

Should probably move machine-side later:

- surface-specific flattening and presentation shaping
- cross-lane top-N projection
- UI-specific tie-breaking decisions

Important boundary:

- the semantic hook or sidecar must not become authoritative for whether a memory item is `candidate`, `established`, or `proven`
- that remains canonical-store truth per `docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md` and `docs/DATA_MODEL_AND_STORAGE_CONTRACT.md`

## 10. Safe isomorphic optimisations vs output-changing redesigns

### Safe isomorphic optimisations

These preserve current output exactly if implemented carefully:

- set-backed membership instead of `iter().any(...)`
- lane-local id-to-index bookkeeping
- direct in-place semantic score propagation
- exact top-k selection using the current comparator, if measured worthwhile

### Output-changing redesigns

These are not "safe perf passes" and must be treated as product or contract changes:

- changing lane semantics or lane count
- letting the semantic sidecar decide lane membership
- changing fused score formula
- replacing hybrid id-based tie-breaking with recency or semantic score tie-breaking
- replacing storage lane vectors with one globally ranked envelope without a contract update
- globally truncating storage results before lane partition is complete

## 11. Recommended next implementation slice

Recommended next bead or slice:

- `storage: prior_review_lookup hybrid merge uses set-backed membership and lane-local id indexes while preserving exact hybrid ordering`

Suggested acceptance boundary:

- no contract change
- exact output equivalence for existing smoke tests
- same degraded-mode behavior
- measured improvement on the existing manual perf harness

Recommended sequence:

1. Implement set-backed membership first as the single lever.
2. Re-measure with the current perf harness.
3. Only if the hotspot remains visible, implement lane-local id-index bookkeeping and direct score propagation as a second pass.

## 12. What should not be optimised yet

Do not optimise these yet:

- global search surface shape
  - current CLI is still lexical-only and is not yet the canonical hybrid consumer
- unified storage envelope redesign
  - too large, not an isomorphic perf change
- semantic-sidecar pruning heuristics
  - premature until real semantic candidate distributions are observed in live paths
- concurrency or parallel merge work
  - current bottleneck is data-structure overhead, not obvious parallel slack
- fused score math
  - currently part of ordering semantics
- SQL/schema denormalisation in this artefact
  - that is a different optimisation axis than the merge path itself

## 13. Bottom line

The remaining hybrid merge work is real but now narrow.

The best next move is not a search redesign. It is a disciplined, measured, reversible pass that:

- replaces repeated linear membership scans with set-backed membership
- then, if justified by re-measurement, adds lane-local id-index bookkeeping so semantic score propagation happens in place

That keeps Roger aligned with its current contracts:

- canonical store remains authoritative
- lane semantics remain explicit
- hybrid ordering remains deterministic
- the merge code gets simpler without pretending the semantic sidecar or CLI projection is the source of truth
