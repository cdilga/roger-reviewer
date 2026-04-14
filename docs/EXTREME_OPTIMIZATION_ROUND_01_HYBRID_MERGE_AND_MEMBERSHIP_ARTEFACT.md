# Extreme Optimization Round 01: Hybrid Merge And Membership Artefact

Status: investigation artefact  
Date: 2026-04-14  
Scope: read-only codebase investigation plus one planning artefact

## 1. Scope and exact question

This artefact answers one narrow question:

After the recent statement-prepare/parse reuse wins, what unnecessary CPU work
and allocation remain in the merge/reconciliation half of
`prior_review_lookup`, especially around:

- semantic candidate partitioning after retrieval
- repeated membership checks over evidence and memory IDs
- ordering, de-duplication, and tie-break semantics
- result-shaping work that happens after lexical/semantic retrieval already
  produced candidate sets

Important repo truth:

- the current `rr search` CLI entry does **not** execute the hybrid path today
  because `handle_search` passes `semantic_assets_verified: false` and
  `semantic_candidates: Vec::new()` at `packages/cli/src/lib.rs:3351-3362`
- the measured hybrid hotspot is therefore the storage-layer
  `prior_review_lookup` path exercised by
  `packages/storage/tests/prior_review_lookup_perf.rs`, not a currently live
  hybrid CLI flow

That distinction matters because it changes where the next optimisation wave
should land: storage first, CLI shaping second.

## 2. Evidence base

Files read in full or in the relevant hot sections:

- `AGENTS.md`
- `docs/skills/ROGER_EXTREME_SOFTWARE_OPTIMIZATION.md`
- `packages/storage/src/lib.rs`
- `packages/storage/tests/prior_review_lookup_smoke.rs`
- `packages/storage/tests/prior_review_lookup_perf.rs`
- `packages/cli/src/lib.rs`
- `docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md`
- `docs/DATA_MODEL_AND_STORAGE_CONTRACT.md`
- `docs/CORE_DOMAIN_SCHEMA_AND_FINDING_FINGERPRINT.md`
- `docs/ROBOT_CLI_CONTRACT.md`

Commands run during this investigation:

- `cargo test -p roger-storage --test prior_review_lookup_smoke -- --nocapture`
  - passed: 3/3 tests
- `cargo test -p roger-storage --release --test prior_review_lookup_perf -- --ignored --nocapture`
  - local run result:
    `p50_us=2919 p95_us=3474 p99_us=4538 mean_us=3004.2`
  - output cardinalities:
    `evidence_hits=306 promoted_memory=128 tentative_candidates=80 semantic_candidates=416`

I am treating the main-thread release measurements as the primary baseline for
decision-making:

- earlier baseline before recent wins:
  `p50=4843us p95=5202us p99=5372us mean=4823.9us`
- recent landed local optimisations:
  `p50~2702us p95~2953us p99~3176us mean~2743us`

My local rerun is slightly slower but is the same order of magnitude and, more
importantly for this artefact, confirms the current result cardinalities that
amplify merge-loop cost.

## 3. Current end-to-end flow

### 3.1 Current CLI search flow

Current `rr search` flow in `packages/cli/src/lib.rs:3323-3438`:

1. parse and validate `--query`
2. infer repo context
3. open `RogerStore`
4. call `prior_review_lookup` with:
   - `scope_key = repo:{repository}`
   - `limit = user_limit + 1`
   - `include_tentative_candidates = false`
   - `allow_project_scope = false`
   - `allow_org_scope = false`
   - `semantic_assets_verified = false`
   - `semantic_candidates = []`
5. convert returned evidence and promoted-memory hits into `serde_json::Value`
   objects
6. sort those JSON values by `score`
7. truncate to requested limit

Current consequence:

- live CLI search is lexical-only
- tentative candidates are not surfaced
- combined cross-lane ordering is imposed in CLI, not in storage

### 3.2 Storage hybrid path

Hybrid-capable flow in `packages/storage/src/lib.rs:2440-2668`:

1. classify scope and fail closed for disallowed project/org overlays
2. normalize query, then reduce lexical matching to the first whitespace token
   at `2482-2488`
3. read lexical and semantic index readiness plus semantic asset verification
4. determine `semantic_operational`
5. run lexical evidence retrieval via `lookup_evidence_hits`
6. run lexical promoted-memory retrieval via `lookup_memory_hits`
7. optionally run lexical tentative-candidate retrieval via `lookup_memory_hits`
8. if hybrid is operational:
   - partition semantic candidates by target kind via
     `semantic_scores_by_target`
   - merge semantic-only evidence hits
   - merge semantic-only memory hits
   - rescore every lane
   - sort every lane
9. else:
   - zero semantic score on every lane
   - compute fused score from lexical only
10. return lane-separated `PriorReviewLookupResult`

### 3.3 What the perf harness actually stresses

`packages/storage/tests/prior_review_lookup_perf.rs:79-275` seeds:

- 416 semantic candidates total
- evidence semantic candidates:
  - 16 PRs
  - 18 semantic candidates per PR
  - 288 total evidence semantic candidate IDs
- memory semantic candidates:
  - 80 lexical-ish + 48 semantic-only
  - 128 total memory semantic candidate IDs

The release harness with `limit=100` currently returns:

- `evidence_hits=306`
- `promoted_memory=128`
- `tentative_candidates=80`

That means `limit` does not act as a final hybrid cap today. The merge loop is
sorting and returning substantially more than the lexical seed size.

## 4. Concrete code map

| Surface | File and lines | Role |
| --- | --- | --- |
| CLI entry | `packages/cli/src/lib.rs:3323-3438` | `handle_search`; current lexical-only caller and JSON result shaping |
| Query/result types | `packages/storage/src/lib.rs:861-922` | `PriorReviewLookupQuery`, retrieval mode, evidence hit, memory hit, result |
| Main merge function | `packages/storage/src/lib.rs:2440-2668` | scope gating, degraded reasons, lexical retrieval, semantic merge, sorting |
| Lexical evidence query | `packages/storage/src/lib.rs:3357-3433` | canonical DB lexical scan and evidence hydration |
| Lexical memory query | `packages/storage/src/lib.rs:3435-3531` | canonical DB lexical scan and memory hydration |
| Semantic-only evidence hydration | `packages/storage/src/lib.rs:3533-3578` | fetch evidence by ID during merge |
| Semantic-only memory hydration | `packages/storage/src/lib.rs:3580-3609` | fetch memory by ID during merge |
| Semantic partition helper | `packages/storage/src/lib.rs:4192-4223` | partition by kind, dedup by max score, clamp/round score, fuse score |
| Memory schema/indexes | `packages/storage/migrations/0007_prior_review_lookup_memory_hooks.sql:1-25` | memory rows and repo-scope lookup indexes |
| Hybrid smoke coverage | `packages/storage/tests/prior_review_lookup_smoke.rs:73-308` | degraded lexical-only, hybrid fusion, overlay fail-closed behavior |
| Perf harness | `packages/storage/tests/prior_review_lookup_perf.rs:79-275` | representative hybrid merge workload and percentile reporting |
| Search contract | `docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md:150-217, 438-544` | retrieval mode, recall envelope, retrieval order, degraded semantics |
| CLI robot contract | `docs/ROBOT_CLI_CONTRACT.md:252-314` | required `rr search` output fields and lane semantics |

## 5. In-memory algorithm observations

### 5.1 Semantic candidate partitioning

`semantic_scores_by_target` at `packages/storage/src/lib.rs:4192-4207` is
already doing one useful thing correctly:

- it borrows candidate IDs as `&str`, so there is no extra ID clone cost in the
  partition map itself
- it deduplicates duplicate semantic candidates by target kind and target ID
  using `max(score)`
- evidence and memory IDs remain partitioned into separate maps, which is good
  because cross-table ID namespaces should not be assumed globally unique

What it does not do:

- it does not carry any membership or position information for already surfaced
  lexical hits
- it assumes a naive `50/50` map-capacity split via
  `HashMap::with_capacity(candidates.len() / 2)` for both maps

Capacity tuning is not the main opportunity here. The missed opportunity is
that the next stage still falls back to repeated vector scans.

### 5.2 Repeated membership checks are still the clearest hot-loop waste

The strongest remaining low-risk inefficiency is in
`packages/storage/src/lib.rs:2557-2593`.

Evidence merge currently does:

- iterate semantic evidence map
- for each ID, scan `evidence_hits.iter().any(...)`
- if not found, fetch by ID and push

Memory merge currently does:

- iterate semantic memory map
- for each ID, scan `promoted_memory.iter().any(...)`
- then scan `tentative_candidates.iter().any(...)`
- if not found, fetch by ID and route by state

On the representative perf corpus:

- semantic evidence IDs: 288
- semantic memory IDs: 128
- final lane sizes: 306 evidence, 128 promoted, 80 tentative

So the current path performs avoidable post-retrieval string-comparison work
roughly proportional to:

- `288 x growing evidence_hits.len()`
- `128 x (growing promoted_memory.len() + growing tentative_candidates.len())`

This is exactly the class of CPU work the storage layer is doing after the
retrieval stages have already identified candidate IDs.

### 5.3 Hydration sequencing is functional but not economical

Current sequencing:

1. lexical queries fully hydrate hit structs
2. semantic partition creates score maps
3. semantic-only misses are hydrated one row at a time by ID
4. all lane items are then rescored in full
5. all lanes are then sorted in full

This has two waste patterns:

- semantic-only hits are assigned `semantic_score_milli` and `fused_score`
  before push at `2564-2567` and `2585-2586`, then those same fields are
  rewritten again during the later full-vector rescoring pass at `2595-2612`
- lexical hits do not get their semantic scores applied until the later full
  lane pass, even though an index structure could update them in place while the
  merge map is being consumed

The current implementation therefore pays both:

- repeated membership scanning
- full-lane rescoring after membership resolution

### 5.4 Ordering, de-duplication, and tie-break semantics

Within each storage lane, ordering is currently deterministic and defensible:

- sort key 1: `fused_score DESC`
- sort key 2: `lexical_score DESC`
- sort key 3: `id ASC`

This is implemented at `2614-2634`.

Implications:

- `sort_unstable_by` is acceptable here because the comparator already defines a
  total order using the ID tie-break
- fused-score collisions do not erase lexical bias because lexical score is the
  explicit second key
- duplicate semantic candidates for the same target collapse to the max semantic
  score before sorting
- hybrid mode does **not** preserve the lexical SQL tie-break from
  `lookup_evidence_hits` / `lookup_memory_hits`
  (`updated_at DESC, rowid DESC`); once the lane is resorted, equal fused and
  equal lexical scores fall back to `id ASC` instead

Cross-lane ordering is a different story:

- storage returns three separate vectors
- current CLI search concatenates evidence first, then promoted memory, then
  sorts combined JSON objects by score only at `3398-3408`
- equal-score ties in CLI therefore preserve insertion order because Rust's
  `sort_by` is stable, which means evidence wins equal-score ties over promoted
  memory purely because evidence is appended first

That is deterministic today, but it is implicit, surface-local, and not stated
as product truth.

### 5.5 Hybrid result growth is currently unbounded by `limit`

This is the highest-leverage semantic observation in the current code.

`limit` is applied to:

- lexical evidence query
- lexical promoted-memory query
- lexical tentative-candidate query

It is **not** applied to:

- semantic-only evidence additions
- semantic-only memory additions
- post-merge lane sizes

The perf harness proves the behavior directly:

- query `limit=100`
- final result sizes `306 / 128 / 80`

This means the hybrid merge path can do large amounts of post-retrieval work
even when the caller only asked for a relatively small result set.

Whether that is a bug or an acceptable current contract is not yet settled in
code or docs. It is definitely an optimisation opportunity, but it is not an
isomorphic refactor unless the intended limit semantics are first frozen.

### 5.6 Current `rr search` still does unnecessary work after storage returns

Even though the live CLI path is lexical-only today, it still does extra CPU and
allocation in `packages/cli/src/lib.rs:3368-3412`:

- builds full `serde_json::Value` objects before it knows which ones survive
  truncation
- sorts by repeatedly extracting `"score"` from dynamic JSON values
- performs truncation only after materializing and sorting those JSON values
- drops `tentative_candidates` completely

That is not the main hybrid hotspot, but it is unnecessary shaping work after
storage already emitted typed lane results.

## 6. Explicit treatment of set-backed membership and precomputed lookup structures

### 6.1 Preferred shapes

For the next isomorphic storage optimisation slice, the most useful structures
are not plain sets. They are index maps:

- evidence lane:
  - `HashMap<&str, usize>` mapping `finding_id -> evidence_hits index`
- memory lanes:
  - `HashMap<&str, MemorySlot>`
  - where `MemorySlot` is an enum like:
    - `Promoted(usize)`
    - `Tentative(usize)`

Why index maps are better than `HashSet<&str>` alone:

- they provide O(1)-ish membership checks
- they let the merge loop update existing lexical hits in place
- they eliminate the need for the later full-lane semantic-score pass for
  already surfaced hits

### 6.2 Ordering implications

If these maps are used only for membership and index lookup:

- determinism is preserved
- final order still comes from the existing vector sort keys
- hash iteration order does not leak into user-visible ordering

If instead the implementation starts iterating map order or preserving semantic
candidate insertion order via an order-preserving map, the surface semantics
would change. Nothing in the current docs requires semantic retrieval order to
win ties. The safer posture is:

- use maps only as lookup aids
- keep vectors as the canonical ordering containers
- keep the final explicit sort

### 6.3 Fail-closed semantics

The membership structure must not weaken Roger's current fail-closed behavior.

Required rules:

- keep evidence and memory namespaces separate
- keep repository and scope filters in the row-hydration path
- only insert a newly seen semantic-only ID into the map after the hit is
  actually accepted into a surfaced lane
- if `memory_hit_by_id` returns a `candidate` while
  `include_tentative_candidates` is false, do not surface it and do not
  silently upgrade it into promoted memory
- if a semantic candidate points at a missing row or an out-of-scope row, skip
  it exactly as today

### 6.4 What not to use first

I would not start with:

- `BTreeSet` or `BTreeMap`
  - deterministic iteration is not needed because these structures should not
    drive output order
- `IndexMap`
  - useful only if product truth later says semantic candidate arrival order
    matters
- a single cross-kind set
  - evidence IDs and memory IDs are distinct domains and should stay distinct

## 7. Candidate optimisation levers ranked by `(Impact x Confidence) / Effort`

Scoring uses a coarse 1-5 scale.

| Rank | Lever | Impact | Confidence | Effort | Score |
| --- | --- | ---: | ---: | ---: | ---: |
| 1 | Index-map-based merge plus single-pass score application | 4 | 5 | 2 | 10.0 |
| 2 | CLI typed staging before JSON sort/truncate | 2 | 5 | 2 | 5.0 |
| 3 | Batch hydrate semantic-only IDs instead of per-ID lookups | 4 | 4 | 4 | 4.0 |
| 4 | Explicit lane-cap or top-K hybrid merge | 5 | 3 | 4 | 3.75 |
| 5 | Projection-aware search-hit hydration | 3 | 3 | 4 | 2.25 |

## 8. Candidate details

### 8.1 Candidate 1: index-map-based merge plus single-pass score application

Description:

- build lane index maps from lexical hits
- while building those maps, apply semantic score if one already exists for the
  lexical hit
- when scanning semantic score maps:
  - update in-place if ID already exists
  - otherwise hydrate once, push once, record slot once
- keep final sorts exactly as they are

Expected win class:

- medium CPU win
- small allocation win
- strongest low-risk next step

Blast radius:

- `packages/storage/src/lib.rs` only
- mostly `prior_review_lookup`

Correctness oracle:

- same lane membership
- same per-lane order
- same degraded reasons
- same fused scores

Evidence needed before implementation:

- add targeted regression tests for:
  - duplicate semantic candidates for the same target use max score
  - equal fused-score ties preserve lexical bias then ID order
  - candidate memory stays hidden when tentative candidates are disabled
  - missing semantic IDs are ignored, not surfaced
- rerun release perf harness and compare to current numbers

Why this ranks first:

- it directly removes the two clearest post-retrieval wastes without changing
  the storage API or product semantics

### 8.2 Candidate 2: CLI typed staging before JSON sort/truncate

Description:

- stage search items in a typed struct or tuple
- sort/truncate typed items
- serialize to JSON only after the final item set is known

Expected win class:

- low-to-medium today
- higher later if hybrid search is threaded into CLI

Blast radius:

- `packages/cli/src/lib.rs` search path only

Correctness oracle:

- identical robot payload for the current implemented fields

Evidence needed before implementation:

- a snapshot test or golden test for current JSON output
- an explicit tie-break for equal-score cross-lane items, because the current
  CLI behavior is implicit stable insertion order

Why it does not rank first:

- current CLI does not even hit the hybrid branch yet

### 8.3 Candidate 3: batch hydrate semantic-only IDs

Description:

- collect missing evidence IDs and missing memory IDs
- fetch them in bulk rather than one `query_row` call per ID

Possible exact shapes:

- dynamic `IN (?, ?, ...)` queries per lane
- chunked batches if placeholder count becomes large
- a temporary table / CTE approach if the implementation wants a more stable SQL
  shape

Expected win class:

- medium CPU win if semantic-only additions remain large
- reduced FFI and SQLite stepping overhead

Blast radius:

- storage SQL
- prepared-statement strategy

Correctness oracle:

- same repository/scope filtering
- same lane routing by memory state
- same per-lane ordering after final sort

Evidence needed before implementation:

- re-profile after Candidate 1 first
- if per-ID lookups remain prominent, then batch hydration is justified

Why it is not first:

- Candidate 1 is cheaper and likely removes a large share of the remaining
  purely in-memory waste without changing SQL shape

### 8.4 Candidate 4: explicit lane-cap or top-K hybrid merge

Description:

- stop allowing semantic-only additions to grow lane vectors without bound
- either:
  - cap per lane after merge, or
  - maintain a bounded top-K structure during merge

Expected win class:

- potentially high
- especially important on corpora where semantic candidates greatly exceed the
  requested limit

Blast radius:

- storage semantics
- CLI/TUI expectations
- tests and docs

Correctness oracle:

- cannot be "same output" until limit semantics are explicitly defined

Evidence needed before implementation:

- product decision on whether `limit` means:
  - per lane
  - final surfaced combined items
  - lexical seed size only
- follow-up golden cases for truncation and lane counts

Why it is not the first optimisation diff:

- the performance leverage is real, but the semantic contract is not frozen yet

### 8.5 Candidate 5: projection-aware search-hit hydration

Description:

- return lighter hit structs for compact search surfaces
- avoid hydrating strings and fields that the current surface does not consume

Expected win class:

- medium allocation win
- maybe medium CPU win if large semantic-only hydration remains

Blast radius:

- storage API shape
- CLI/TUI recall projection logic

Correctness oracle:

- every surface still preserves the required `RecallEnvelope` truth for its
  projection

Evidence needed before implementation:

- decide which fields are truly required for the first shipped `rr search`
  projection
- align live CLI output with `docs/ROBOT_CLI_CONTRACT.md:252-314`

Why it ranks last:

- it is a wider contract change and is not needed to capture the clearest
  storage-merge win

## 9. Work that should move earlier or later

### 9.1 Move earlier into retrieval/merge setup

These belong earlier than the current hot merge loop:

- build lane membership/index maps immediately after lexical retrieval
- apply semantic scores to already surfaced lexical hits while those maps are
  built, not in a later full pass
- if Candidate 3 is pursued, collect semantic-only misses first and hydrate them
  in bulk before the final sort

### 9.2 Keep later in output shaping

These should remain surface-local rather than move into storage yet:

- cross-lane flattening for CLI presentation
- surface-specific citation posture or UI-oriented display text

Reason:

- docs still describe lane-separated `RecallEnvelope` truth and explicit search
  intent fields
- the live CLI output is not yet fully aligned with that contract
- pushing more cross-lane presentation behavior into storage now would risk
  hardening the wrong surface semantics

### 9.3 Do not move fail-closed gates later

These should stay before surfacing:

- scope gating
- semantic asset verification
- semantic index readiness checks
- candidate visibility policy for tentative memory

Those are safety and truthfulness rules, not cosmetic search shaping.

## 10. Recommended next implementation slice

Recommended next bead:

`prior_review_lookup: replace vector membership scans with slot maps and
single-pass score application`

Why this slice first:

- it is the largest obviously isomorphic optimisation still visible in source
- it stays inside storage
- it does not require a product decision on limit semantics
- it gives cleaner evidence for whether batch hydration is still worth doing

Suggested acceptance contract:

1. no change in lane membership, degraded reasons, or within-lane ordering
2. new regression coverage for:
   - duplicate semantic candidates
   - equal fused-score ties
   - candidate visibility gating
   - missing semantic IDs
3. release perf harness shows a real win on the representative corpus
4. smoke tests remain green

Suggested follow-on bead after that:

`prior_review_lookup: freeze hybrid limit semantics and cap post-merge result
growth`

That one should not be folded into the first optimisation diff because it is
not obviously isomorphic.

## 11. What I would not optimise yet

I would explicitly defer the following:

- changing `fused_score` weighting in `packages/storage/src/lib.rs:4219-4222`
  - that changes ranking semantics
- changing the first-token lexical-query reduction at `2482-2488`
  - that is retrieval behavior, not merge-loop cleanup
- concurrency or parallel merge logic
  - current hotspot is still ordinary per-query CPU work; concurrency would add
    determinism and complexity risk too early
- custom hashers, map micro-tuning, or alternative collection crates
  - premature until the vector-scan waste is removed
- projection-aware storage structs before the search-item contract is tightened
  - otherwise we risk optimizing toward the wrong surface shape
- lane-capping without a contract decision
  - potentially high payoff, but not safely isomorphic

## 12. Bottom line

The remaining hot-path waste is real and source-visible.

The clearest next optimisation is not a broad rewrite. It is a narrow
storage-only refactor:

- replace repeated `.iter().any(...)` membership checks with slot maps
- apply semantic scores in the same pass that resolves membership
- keep final sort semantics unchanged

That should reduce hybrid merge CPU without changing Roger's output contract.
Only after that re-measurement should the team decide whether the next wave is:

- bulk semantic-only hydration, or
- explicit hybrid result capping based on a frozen limit contract

The current CLI search path also does unnecessary shaping work, but it is a
second-order target until hybrid retrieval is actually wired through that
surface.
