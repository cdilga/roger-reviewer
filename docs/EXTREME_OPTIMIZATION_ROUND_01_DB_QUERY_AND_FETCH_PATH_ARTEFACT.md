# Extreme Optimization Round 01: DB Query And Fetch Path Artefact

Status: investigation artefact for the Round 01 extreme-optimisation planning pass.

Audience: maintainers deciding the next `prior_review_lookup` optimisation bead.

Primary question: what DB and fetch-path work does `prior_review_lookup` still do today, where does that work come from in the current schema/query shape, and which optimisation levers are worth pulling next without weakening Roger's storage truth, scope safety, or replayability.

## Scope

This artefact is intentionally narrow.

It answers:

1. how the current `rr search` path reaches `prior_review_lookup`
2. where repository / PR identity is extracted from JSON-backed SQL today
3. how the current schema helps or hurts future denormalisation or alternate indexed columns
4. how the semantic merge path currently re-hydrates semantic-only ids
5. whether the current user-facing `rr search` path does unnecessary DB work
6. which optimisation levers are worth doing next, ranked with the Roger optimisation heuristic `(Impact × Confidence) / Effort`

It does not attempt to redesign search semantics, replace the lexical engine with Tantivy in this artefact, or change Roger's canonical storage truth.

## Method

Required repo context read:

- `AGENTS.md`
- `docs/skills/ROGER_EXTREME_SOFTWARE_OPTIMIZATION.md`
- `packages/storage/src/lib.rs`
- `packages/storage/tests/prior_review_lookup_smoke.rs`
- `packages/storage/tests/prior_review_lookup_perf.rs`
- `packages/cli/src/lib.rs`
- `packages/storage/migrations/0001_init.sql`
- `packages/storage/migrations/0006_finding_materialization.sql`
- `packages/storage/migrations/0007_prior_review_lookup_memory_hooks.sql`
- `docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md`
- `docs/DATA_MODEL_AND_STORAGE_CONTRACT.md`
- `docs/CORE_DOMAIN_SCHEMA_AND_FINDING_FINGERPRINT.md`

Additional direct validation performed during this investigation:

- `cargo test -p roger-storage prior_review_lookup_ -- --nocapture`
- one-shot ignored perf harness invocation to capture live result cardinality, not latency truth:
  `ROGER_STORAGE_PERF_WARMUP=1 ROGER_STORAGE_PERF_ITERATIONS=1 cargo test -p roger-storage prior_review_lookup_perf_hybrid_hot_path_reports_percentiles -- --ignored --nocapture`
- `EXPLAIN QUERY PLAN` against a fresh DB built from the relevant migrations for:
  - `lookup_evidence_hits`
  - `lookup_memory_hits`
  - `evidence_hit_by_id`
  - `memory_hit_by_id`
  - bulk `IN (...)` alternatives for evidence and memory id hydration

## Executive answer

The current hot path is still doing materially more canonical-DB work than the user-facing CLI needs, and the remaining DB-shaped costs break into two distinct buckets.

First, the current CLI `rr search` path is always lexical-only in this slice. `handle_search` passes `semantic_assets_verified = false`, `semantic_candidates = Vec::new()`, and `include_tentative_candidates = false`, so the user-facing command can never execute hybrid retrieval today even though `prior_review_lookup` still performs semantic readiness bookkeeping on every call. That means the current shipped path still pays for:

- two `index_state` point reads
- semantic manifest verification file reads and hashing
- one evidence lexical query
- one promoted-memory lexical query
- bounded over-fetch because each lane gets `limit + 1` separately and the CLI truncates only after flattening the lanes

Second, the storage-level hybrid path still has a real DB/query-shape problem after statement-cache wins. It does lexical SQL first, then individually re-fetches semantic-only ids one at a time with `evidence_hit_by_id` and `memory_hit_by_id`. In the existing perf harness corpus, a `limit = 100` lookup still returned `evidence_hits = 306`, `promoted_memory = 128`, and `tentative_candidates = 80`, which proves the merge path can grow far beyond the requested top-k before the CLI ever truncates anything.

That makes the next wave clearer:

1. if the target is the current shipped CLI path, split out an explicit lexical-only fast path and stop paying semantic bookkeeping costs when the caller has already disabled semantic retrieval
2. if the target is the measured hybrid storage hot path, replace one-by-one semantic id hydration with bulk fetch helpers before touching denormalisation
3. only after the fetch shape is cleaned up should denormalised repository / PR columns be considered, and then only on `review_sessions`, not on `findings`

## Current end-to-end flow

### 1. CLI entry

`packages/cli/src/lib.rs:3323-3443`

`handle_search`:

- validates `--query`
- resolves repo context from explicit `--repo` or cwd
- opens `RogerStore`
- sets `limit = min(parsed.limit.unwrap_or(10), 100)`
- calls `prior_review_lookup` with:
  - `scope_key = "repo:{repository}"`
  - `repository = {repository}`
  - `query_text = user text`
  - `limit = limit + 1`
  - `include_tentative_candidates = false`
  - `allow_project_scope = false`
  - `allow_org_scope = false`
  - `semantic_assets_verified = false`
  - `semantic_candidates = Vec::new()`

Important repo-truth consequence:

- the current CLI path cannot execute hybrid retrieval in this slice
- the current CLI path never asks for candidate memory

### 2. Store open

`packages/storage/src/lib.rs:969-980`

`RogerStore::open`:

- creates the store/artifact/sidecar directories
- opens SQLite
- enables foreign keys
- applies migrations

This is part of end-to-end search latency, but it is not the focus of this artefact. The rest of this document focuses on the query/fetch work after the store is open.

### 3. `prior_review_lookup`

`packages/storage/src/lib.rs:2440-2665`

The storage path does the following in order:

1. resolves the scope class and fail-closed overlay rules
2. lowercases the query text
3. keeps only the first whitespace token as the lexical SQL token
4. clamps `limit` into `1..=100`
5. loads lexical `index_state`
6. loads semantic `index_state`
7. verifies the semantic asset manifest
8. decides whether semantic retrieval is operational
9. runs lexical evidence lookup
10. runs lexical promoted-memory lookup
11. optionally runs lexical candidate-memory lookup
12. if semantic retrieval is operational:
    - builds `HashMap`s of semantic scores by target id
    - checks whether each semantic candidate is already present
    - individually re-fetches missing ids from canonical DB
    - applies fused scores
    - sorts each lane
13. otherwise, zeroes the semantic score fields and returns lexical-only results

### 4. CLI projection

`packages/cli/src/lib.rs:3368-3443`

The CLI:

- flattens `evidence_hits` and `promoted_memory` into one `items` array
- ignores `tentative_candidates` entirely in this command
- sorts the flattened array by `score`
- truncates to the user-visible `limit`

That means storage currently does lane-local retrieval first, while the CLI does the final global truncation later.

## Concrete code map

Primary code surfaces:

- `packages/cli/src/lib.rs:3323-3443`
  - `handle_search`
- `packages/storage/src/lib.rs:860-922`
  - `PriorReviewLookupQuery`
  - `PriorReviewEvidenceHit`
  - `PriorReviewMemoryHit`
  - `PriorReviewLookupResult`
- `packages/storage/src/lib.rs:1012-1048`
  - `verify_semantic_asset_manifest`
- `packages/storage/src/lib.rs:2440-2665`
  - `prior_review_lookup`
- `packages/storage/src/lib.rs:3336-3355`
  - `index_state`
- `packages/storage/src/lib.rs:3357-3432`
  - `lookup_evidence_hits`
- `packages/storage/src/lib.rs:3435-3531`
  - `lookup_memory_hits`
- `packages/storage/src/lib.rs:3533-3578`
  - `evidence_hit_by_id`
- `packages/storage/src/lib.rs:3580-3609`
  - `memory_hit_by_id`
- `packages/storage/src/lib.rs:4192-4222`
  - `semantic_scores_by_target`
  - `fused_score`

Relevant schema and migration surfaces:

- `packages/storage/migrations/0001_init.sql:9-18`
  - `review_sessions(review_target TEXT NOT NULL, ...)`
- `packages/storage/migrations/0001_init.sql:30-45`
  - `findings`
- `packages/storage/migrations/0006_finding_materialization.sql:1-18`
  - added `normalized_summary`, severity/confidence, and last-seen fields
- `packages/storage/migrations/0007_prior_review_lookup_memory_hooks.sql:1-24`
  - `memory_items`
  - `idx_memory_items_scope_state`
  - `idx_memory_items_scope_key`
  - `idx_review_sessions_repository`
  - `idx_findings_session_updated`

Relevant tests and contract docs:

- `packages/storage/tests/prior_review_lookup_smoke.rs:74-305`
- `packages/storage/tests/prior_review_lookup_perf.rs:79-275`
- `packages/cli/tests/session_aware_cli_smoke.rs:1498-1546`
- `docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md:29-72`
- `docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md:148-217`
- `docs/DATA_MODEL_AND_STORAGE_CONTRACT.md:14-65`
- `docs/CORE_DOMAIN_SCHEMA_AND_FINDING_FINGERPRINT.md:257-310`
- `docs/CORE_DOMAIN_SCHEMA_AND_FINDING_FINGERPRINT.md:488-513`

## What the current schema and query path are actually doing

### Observation 1: repository / PR identity is still hot-path JSON extraction

`review_sessions.review_target` is still stored as a JSON blob at write time.

Evidence:

- `packages/storage/src/lib.rs:1084-1104` serialises `ReviewTarget` into `review_target`
- `packages/storage/migrations/0001_init.sql:9-18` stores `review_target TEXT NOT NULL`
- `lookup_evidence_hits` and `evidence_hit_by_id` both read:
  - `json_extract(rs.review_target, '$.repository')`
  - `CAST(json_extract(rs.review_target, '$.pull_request_number') AS INTEGER)`

This is mitigated, not eliminated, by `idx_review_sessions_repository`.

`EXPLAIN QUERY PLAN` for `lookup_evidence_hits` on the migrated schema:

```text
SEARCH rs USING INDEX idx_review_sessions_repository (<expr>=?)
SEARCH f USING INDEX idx_findings_session_updated (session_id=?)
USE TEMP B-TREE FOR ORDER BY
```

Interpretation:

- SQLite can use the expression index to find sessions for one repository
- it then walks findings by `session_id`
- it still has to compute lexical scores row by row
- it still has to materialise a temp B-tree because `ORDER BY lexical_score DESC, rs.updated_at DESC, f.rowid DESC` is not index-covered

The JSON shape is therefore not catastrophic, but it is still part of the hot path and still blocks simpler composite indexes.

### Observation 2: the evidence query is repo-filtered but still fundamentally a scan inside that repo

`lookup_evidence_hits`:

- joins `findings` to `review_sessions`
- filters on repository only
- checks `fingerprint`, `title`, and `normalized_summary` with `lower(...)` and `instr(...)`
- orders by a computed score plus recency

Important consequences:

- there is no usable index on `title` or `normalized_summary` for this shape
- `idx_findings_fingerprint` exists, but the query wraps `fingerprint` in `lower(...)` and substring checks, so it does not buy much here
- the first-token lexical policy increases the chance of broad candidate sets on common leading terms

This matches the current product posture that canonical DB lexical scan is a truthful fallback path, but it also means repo-local evidence lookup still scales mostly with "number of findings in the repo scope" until Tantivy becomes the actual healthy lexical path.

### Observation 3: `lookup_memory_hits` is better indexed than evidence lookup, but still sorts through computed scores

`lookup_memory_hits` uses:

- `scope_key`
- `state IN (...)`
- score terms over `normalized_key`, `statement`, and `anchor_digest`
- `ORDER BY lexical_score DESC, updated_at DESC, rowid DESC`

`EXPLAIN QUERY PLAN`:

```text
SEARCH memory_items USING INDEX idx_memory_items_scope_state (scope_key=? AND state=?)
USE TEMP B-TREE FOR ORDER BY
```

Interpretation:

- the scope/state index helps
- SQLite still must compute lexical scores and sort the matches
- `idx_memory_items_scope_key(scope_key, normalized_key)` is not used by the current query shape because the SQL uses `lower(normalized_key)` and `instr(lower(normalized_key), ...)`

This matters for denormalisation discussion: the immediate issue is not that the memory table lacks columns, but that the current predicate/order shape prevents the most specific existing index from being used.

### Observation 4: the semantic merge path is still N point fetches, not one bulk hydrate

When semantic retrieval is operational:

- evidence semantic candidates are grouped into `HashMap<&str, i64>`
- memory semantic candidates are grouped into `HashMap<&str, i64>`
- for every semantic candidate not already present in lexical results:
  - `evidence_hit_by_id(repository, finding_id)` is called individually
  - `memory_hit_by_id(scope_key, memory_id)` is called individually

The point lookups themselves are cheap:

`EXPLAIN QUERY PLAN` for `evidence_hit_by_id`:

```text
SEARCH f USING INDEX sqlite_autoindex_findings_1 (id=?)
SEARCH rs USING INDEX sqlite_autoindex_review_sessions_1 (id=?)
```

`EXPLAIN QUERY PLAN` for `memory_hit_by_id`:

```text
SEARCH memory_items USING INDEX sqlite_autoindex_memory_items_1 (id=?)
```

The issue is not that each query is bad. The issue is that the current storage path can execute them hundreds of times per lookup.

### Observation 5: hybrid result sets are currently allowed to grow far beyond `limit`

The lexical SQL uses `LIMIT ?`.

The semantic merge does not re-apply that limit after individual re-hydration.

Observed directly from the existing ignored perf harness with one warmup and one measured iteration:

```text
evidence_hits=306 promoted_memory=128 tentative_candidates=80 semantic_candidates=416
```

This was with `limit = 100`.

That means:

- the current `limit` is only a lexical pre-limit
- semantic merge can inflate the in-memory result sets far above the caller's requested top-k
- any later flattening/truncation work in the CLI is operating after the expensive DB hydration already happened

This is the strongest direct evidence that the remaining hot-path cost is no longer mainly statement preparation.

### Observation 6: the current CLI search path still does work it cannot benefit from

`packages/cli/src/lib.rs:3351-3361` proves the current CLI search path always passes:

- `semantic_assets_verified = false`
- `semantic_candidates = Vec::new()`
- `include_tentative_candidates = false`

`packages/cli/tests/session_aware_cli_smoke.rs:1498-1546` then asserts degraded lexical-only output.

Implications:

- the user-facing CLI path never exercises the semantic merge code today
- the user-facing CLI path still pays for:
  - `index_state("lexical:*")`
  - `index_state("semantic:*")`
  - `verify_semantic_asset_manifest()`
  - one evidence DB scan
  - one promoted-memory DB scan
- the user-facing CLI path also over-fetches by lane because it asks storage for `limit + 1`, then storage applies that separately to evidence and promoted memory, and only the CLI applies the final global truncation

For the default `limit = 10`, the lexical-only CLI can fetch up to:

- 11 evidence rows
- 11 promoted-memory rows

before discarding extras.

That over-fetch is bounded, but it is still unnecessary DB work relative to the final user-visible top-10 list.

## Denormalisation tradeoffs

Denormalisation here should mean one narrow thing: add query-friendly projected columns while keeping the current JSON snapshot for full target truth and replay.

### Option A: keep JSON-only `review_target` in `review_sessions`

Pros:

- one obvious source of truth
- no migration cost
- preserves full target snapshot shape naturally

Cons:

- every hot repo / PR filter is an expression query
- every evidence lookup still extracts repository and PR from JSON
- composite indexing choices stay awkward
- adjacent paths like `find_sessions_by_target` keep paying the same JSON extraction pattern

Judgment:

- correct but not query-friendly

### Option B: add projected `repository` and `pull_request_number` columns to `review_sessions`, keep JSON

Pros:

- best balance of truthfulness and hot-path friendliness
- JSON snapshot remains available for replay, audit, and future fields
- repository / PR filters become ordinary columns
- simpler future indexes become possible, for example `(repository, updated_at DESC)`
- also improves non-search callers such as `find_sessions_by_target`

Cons:

- requires a real schema migration and backfill
- creates a dual-representation invariant that must be tested

Truth posture:

- JSON should remain the full review-target snapshot
- projected columns should be treated as hot-path query projections that must equal the JSON payload written in the same transaction

Migration burden:

- medium
- add nullable columns first or add with backfill
- backfill from `json_extract(review_target, ...)`
- add invariant tests on write and read paths

Replayability and auditability:

- unchanged if JSON is retained

Operator compatibility:

- compatible with the store migration contract if introduced as an additive migration

Query/index friendliness:

- materially better than status quo

Judgment:

- this is the only denormalisation option that looks justified in the near term

### Option C: duplicate repository / PR onto `findings`

Pros:

- could remove the evidence join for some queries
- could enable direct repo-local evidence indexes

Cons:

- duplicates session target identity onto every finding row
- larger write amplification and backfill
- broader correctness surface if the projection ever drifts
- much larger blast radius than denormalising only `review_sessions`

Replayability and auditability:

- still okay if session JSON remains, but the canonical-state surface becomes more complex

Judgment:

- not recommended for the next wave

## Bulk-fetch alternatives for semantic-only ids

This section treats only the missing-id hydration problem. It does not assume a change in ranking semantics.

### Shape 1: chunked `IN (...)` bulk hydrate by target kind

Evidence shape:

```sql
SELECT ...
FROM findings f
JOIN review_sessions rs ON rs.id = f.session_id
WHERE f.id IN (?, ?, ...)
  AND rs.repository-or-json-filter = ?
```

Memory shape:

```sql
SELECT ...
FROM memory_items
WHERE id IN (?, ?, ...)
  AND scope_key = ?
```

Observed planner shape on the current schema:

- evidence bulk `IN (...)` still uses the findings PK plus review_sessions PK
- memory bulk `IN (...)` still uses the memory_items PK

Ordering implications:

- SQL row order does not need to preserve semantic-candidate order
- the current Rust-side fused-score sort can remain authoritative
- the result can stay isomorphic if the final comparator stays unchanged

Fail-closed semantics:

- keep repository filter on evidence rows
- keep `scope_key` filter on memory rows
- silently drop missing ids or wrong-scope ids, exactly as the current per-id helpers do

Operational constraints:

- chunk to stay under SQLite parameter limits for larger future corpora

Judgment:

- best near-term fetch-shape improvement

### Shape 2: `VALUES` / CTE candidate staging and join in one SQL statement

Shape:

- create a `WITH semantic_candidates(target_id, score_milli) AS (VALUES ...)`
- join directly to `findings` or `memory_items`
- compute fused score in SQL

Ordering implications:

- can preserve stable ordering if the current comparator is encoded in SQL
- can also support post-merge top-k in SQL

Fail-closed semantics:

- naturally fail-closed if repository / scope filters remain in the join

Pros:

- fewer round trips than Rust-driven bulk hydrate
- opens the door to SQL-side top-k after semantic merge

Cons:

- higher implementation effort
- much larger dynamic SQL surface
- more brittle around parameter-count limits and SQL generation complexity

Judgment:

- plausible follow-on, not the first fetch-shape change

### Shape 3: temp-table candidate staging

Shape:

- write semantic candidates into a temp table, then join

Pros:

- clean SQL joins for very large candidate sets

Cons:

- more stateful machinery
- unnecessary for the current scale

Judgment:

- do not do this yet

## Candidate optimisation levers ranked

Scoring scale used here:

- Impact: 1 low, 5 high
- Confidence: 1 weak evidence, 5 strong code-and-measurement evidence
- Effort: 1 tiny diff, 5 broad refactor

### Ranking table

| Rank | Lever | Impact | Confidence | Effort | Score |
|---|---|---:|---:|---:|---:|
| 1 | Bulk hydrate semantic-only ids with chunked `IN (...)` helpers | 4 | 5 | 2 | 10.0 |
| 2 | Split out an explicit lexical-only fast path for current CLI search | 3 | 4 | 2 | 6.0 |
| 3 | Collapse promoted + candidate memory lookup into one scan when candidates are requested | 3 | 4 | 2 | 6.0 |
| 4 | Add projected `repository` / `pull_request_number` columns to `review_sessions` | 3 | 4 | 3 | 4.0 |
| 5 | Enforce post-merge top-k instead of allowing semantic merge to grow result sets unbounded | 4 | 3 | 4 | 3.0 |
| 6 | Denormalise repo / PR onto `findings` or add broader alternate text columns now | 2 | 2 | 5 | 0.8 |

### 1. Bulk hydrate semantic-only ids with chunked `IN (...)` helpers

What it is:

- replace the per-id `evidence_hit_by_id` loop with one bulk helper
- replace the per-id `memory_hit_by_id` loop with one bulk helper
- keep the current repository and `scope_key` fail-closed filters
- keep the current Rust-side score fusion and ordering

Expected win class:

- medium to large on the hybrid storage hot path
- negligible on the current lexical-only CLI path

Why it ranks first:

- it addresses the exact remaining DB fetch-shape cost exposed by the current perf harness and code path
- it is compatible with the existing result semantics
- `EXPLAIN QUERY PLAN` shows the bulk `IN (...)` form still hits the PK indexes

Blast radius:

- narrow to `prior_review_lookup` and new helper functions in `packages/storage/src/lib.rs`

Correctness oracle:

- same ordered hits and same fused scores for the existing smoke fixtures
- same inclusion/exclusion decisions for wrong-repo or wrong-scope semantic candidates

Evidence needed before implementation:

- add or reuse a seeded hybrid fixture with semantic-only ids in both lanes
- capture ordered ids and fused scores before the change
- rerun `prior_review_lookup_smoke.rs`
- rerun the ignored perf harness in release mode with the same corpus

### 2. Split out an explicit lexical-only fast path for current CLI search

What it is:

- when the caller already disables semantics and candidates, do not execute the semantic branch bookkeeping unconditionally
- avoid semantic candidate map work entirely
- potentially avoid the semantic `index_state` and manifest verification path if the retrieval contract allows it

Expected win class:

- small to medium on the current user-facing CLI path
- possibly larger on machines with a real installed semantic asset payload, because `verify_semantic_asset_manifest` currently reads and hashes the asset file on every call

Blast radius:

- moderate because retrieval-mode and degraded-reason truth are contract-visible

Correctness oracle:

- current lexical-only search results remain unchanged
- if degraded reasons change, the change must be treated as a contract decision, not as an isomorphic optimisation

Evidence needed before implementation:

- measure current CLI `rr search --query ... --robot` on a machine with and without installed semantic assets
- decide whether "semantic unavailable" is still a required degraded reason when the caller explicitly disables semantics

Important caution:

- this lever is attractive, but it is not purely a DB optimisation if it changes surfaced degraded reasons

### 3. Collapse promoted + candidate memory lookup into one scan when candidates are requested

What it is:

- when `include_tentative_candidates = true`, fetch `candidate`, `established`, and `proven` in one memory query
- partition the rows in Rust after retrieval

Expected win class:

- small to medium on hybrid or candidate-audit style paths
- no win on the current CLI path because `include_tentative_candidates = false`

Blast radius:

- narrow to `prior_review_lookup`

Correctness oracle:

- same memory ids per lane
- same ordering inside `promoted_memory` and `tentative_candidates`

Evidence needed before implementation:

- hybrid fixture proving candidate and promoted rows still land in the correct lanes
- perf harness rerun with candidates enabled

### 4. Add projected `repository` / `pull_request_number` columns to `review_sessions`

What it is:

- add ordinary columns to `review_sessions`
- keep the full `review_target` JSON snapshot
- rewrite hot-path filters to use the projected columns instead of `json_extract(...)`

Expected win class:

- small to medium
- helpful for search and adjacent session-resolution queries
- unlikely to be the dominant next win compared with bulk semantic hydration

Blast radius:

- medium
- affects schema, write path, read path, and migration proof

Correctness oracle:

- the projected columns must always equal the repository and PR embedded in the JSON snapshot

Evidence needed before implementation:

- additive migration with backfill
- tests proving JSON and projected columns stay in sync
- `EXPLAIN QUERY PLAN` before/after comparison on the rewritten queries

### 5. Enforce post-merge top-k

What it is:

- do not allow semantic merge to expand result sets arbitrarily beyond the requested `limit`
- either cap after hydration or move the cap into a more integrated merge plan

Expected win class:

- medium to large if result growth is a dominant source of extra work

Blast radius:

- high, because this may change which lower-ranked items remain present in `PriorReviewLookupResult`

Correctness oracle:

- must define whether `limit` is lane-local, storage-global, or CLI-global
- cannot be treated as isomorphic until that contract is explicit

Evidence needed before implementation:

- a written decision about intended `limit` semantics for storage and CLI
- golden fixtures showing the accepted final ordering and truncation rule

Judgment:

- this is probably worth doing later, but only after the product contract is explicit enough

### 6. Denormalise onto `findings` or broaden alternate indexed text columns now

What it is:

- duplicate repository / PR onto findings, or add broader alternate text/index columns before clarifying exact-match versus substring-match semantics

Expected win class:

- uncertain

Blast radius:

- high

Correctness oracle:

- difficult to state cleanly because this starts mixing performance work with query-semantics cleanup

Evidence needed before implementation:

- a stronger query planner contract than the repo currently has

Judgment:

- do not do this in the next wave

## Recommended next bead / implementation slice

Recommended next implementation slice:

**`prior_review_lookup` bulk semantic hydration without ranking changes**

Scope:

- add `evidence_hits_by_ids(repository, ids)` with chunked `IN (...)`
- add `memory_hits_by_ids(scope_key, ids)` with chunked `IN (...)`
- replace the one-by-one semantic-only fetch loops with bulk hydration maps
- keep the current fused-score formula
- keep the current sort comparator
- keep the current repository / scope fail-closed filters
- do not change `limit` semantics in this bead

Why this slice is the best next move:

- it directly attacks the remaining measured DB/fetch-shape cost in the hybrid path
- it is a small enough diff to keep rollback obvious
- it avoids mixing performance work with product-contract changes
- it leaves the denormalisation question and the top-k semantics question for a later, more explicit bead

Suggested validation contract for that bead:

- lane: `integration`
- suites:
  - `cargo test -p roger-storage prior_review_lookup_ -- --nocapture`
  - ignored perf harness rerun in release mode with the same seeded corpus and percentiles recorded
- proof artefacts:
  - before/after ordered ids for evidence and memory lanes on a seeded hybrid fixture
  - before/after perf output with p50/p95/p99 and result cardinalities

If the round wants one smaller, current-CLI-only slice before that, the best candidate is:

- **lexical-only fast path with no semantic hydration branch**

But that slice should be taken only after deciding whether degraded-reason semantics are allowed to change.

## What I would not optimise yet

I would not do the following in the next bead:

- I would not remove `review_target` JSON as canonical storage. The docs explicitly require canonical relational truth plus replayability and rebuildability from canonical state, and the JSON snapshot still serves that role cleanly.
- I would not denormalise repository / PR onto `findings` yet. The blast radius is too wide relative to the evidence we have.
- I would not add SQLite FTS, Tantivy wiring, or broader lexical-engine changes in this bead. That is a larger architectural move than the current fetch-shape problem requires.
- I would not change query semantics such as "first token only" versus full-query lexical matching inside an optimisation bead. That is planner behaviour, not a free perf win.
- I would not rely on `lower(normalized_key)` removal until the repo enforces a write-time normalisation invariant for `normalized_key`. The existing schema and write path do not prove that strongly enough yet.
- I would not tune PRAGMAs, concurrency, or caching layers before fixing the clearly unnecessary DB hydration work. The current evidence points to query shape and result growth, not to a low-level SQLite tuning ceiling.

## Bottom line

The next optimisation decision should distinguish between two truths that currently coexist in the repo:

- the shipped CLI search path is lexical-only and still does some unnecessary readiness work
- the measured hybrid storage path still spends real time on avoidable canonical-DB hydration after statement-cache wins

The highest-confidence next optimisation bead is therefore not a denormalisation bead. It is a fetch-shape bead:

- bulk hydrate semantic-only ids
- preserve current ranking semantics
- measure again

After that, the remaining denormalisation question will be easier to judge honestly because the fetch-path noise will no longer dominate the picture.
