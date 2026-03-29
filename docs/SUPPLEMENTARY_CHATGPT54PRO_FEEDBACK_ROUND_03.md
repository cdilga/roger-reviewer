Status: supplementary external feedback artifact. This document is research and
advice input for planning, not the canonical Roger spec. If anything here
conflicts with `AGENTS.md` or `PLAN_FOR_ROGER_REVIEWER.md`, those canonical
documents win.

Below is the design I would ship for Roger as of **2026-03-29**.

## 1. Executive summary

**Observed.** Long-term memory for LLM systems is still immature. LongMemEval reports a **30% accuracy drop** for commercial chat assistants and long-context LLMs across sustained interactions, and frames the problem as **indexing, retrieval, and reading**. MemoryAgentBench argues current methods still miss four core capabilities: **accurate retrieval, test-time learning, long-range understanding, and selective forgetting**. On the developer side, LoCoEval shows repository-oriented long-horizon context management is still under-benchmarked, and RepoReason finds that **integration width** is a primary bottleneck in repository-level reasoning. ([arXiv][1])

**Observed.** The newest code-agent papers do show that historical repository memory can help. *Improving Code Localization with Repository Memory* argues that agents should not solve each issue “from scratch” and uses commit history as memory. *MemGovern* reports gains from governed “experience cards,” and *Your Code Agent Can Grow Alongside You with Structured Memory* reports gains from structured history plus real-time feedback. But these are mostly **very recent 2025–2026 arXiv preprints**, and they target autonomous bug-fixing more than conservative human-in-the-loop review. Treat them as directional evidence, not settled product doctrine. ([arXiv][2])

**Recommendation.** Roger should **not** build a global assistant memory or a multi-agent flywheel. It should build a **scoped evidence system**: broad searchable history inside the active repo, a thin promoted layer of semantic/procedural memory above that history, and explicit project/org overlays that are never ambient. The canonical store should be local and relational. **Lexical retrieval should be primary. Semantic retrieval should ship on day 1, but only over a curated corpus and never as a gating dependency.** This is the architecture most consistent with Roger’s own constraints: local-first, review-safe, explicit approval before GitHub posting, no hidden daemon, and read-heavy low-latency review work. ([GitHub][3])

**Recommendation.** Roger’s current plan has a real contradiction: one part says to use a local SQLite-family store with Tantivy now and leave room for semantic search later; another says **“v1 — Tantivy + FastEmbed (full hybrid from day one)”**; a risk section separately suggests shipping SQLite+FTS first and deferring semantics. The defensible resolution is: **ship hybrid on day 1, but keep the semantic corpus narrow, local-only, and best-effort**. That satisfies the user requirement that semantic search is in scope from day 1 without letting embeddings become a brittle blocker. ([GitHub][4])

## 2. Research landscape table

| Paper/system                                                  |                                                          Date | Core idea                                                                                                                                  | What it improves                                    | Fit for Roger                            | Copy / defer / reject                                                                                                    | Sources        |
| ------------------------------------------------------------- | ------------------------------------------------------------: | ------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------- | ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------ | -------------- |
| LongMemEval                                                   |                                   2024-10-14; rev. 2025-03-04 | Benchmark memory as indexing/retrieval/reading; recommends session decomposition, fact-augmented key expansion, time-aware query expansion | Extraction, temporal reasoning, updates, abstention | **Very high** as a design/eval reference | **Copy** decomposition, key expansion, abstention thinking; **do not** optimize for chat benchmark alone                 | ([arXiv][5])   |
| MemoryAgentBench                                              |                                   2025-07-07; rev. 2026-03-17 | Interactive benchmark for retrieval, learning, long-range understanding, selective forgetting                                              | Better memory eval coverage                         | **High** for evaluation                  | **Copy** benchmark categories; **defer** dataset-specific tuning                                                         | ([arXiv][6])   |
| Mem0                                                          |                                                    2025-04-28 | Dynamic extraction, consolidation, retrieval; production-oriented framing                                                                  | Accuracy, latency, token cost                       | **Medium-high**                          | **Copy** dynamic extraction/consolidation idea; **defer** product stack assumptions                                      | ([arXiv][7])   |
| LightMem                                                      |                                   2025-10-21; rev. 2026-02-28 | Sensory filter → short-term topic memory → long-term memory with offline “sleep-time” update                                               | Efficiency, separation of online vs offline work    | **High**                                 | **Copy** offline consolidation; **reject** heavyweight online memory management                                          | ([arXiv][8])   |
| Beyond Static Summarization / ProMem                          |                                                    2026-01-08 | One-off summary is blind; use iterative, feedback-based extraction                                                                         | Extraction completeness and correction              | **High**                                 | **Copy** reflective extraction after sessions; **defer** expensive self-questioning loops everywhere                     | ([arXiv][9])   |
| Temporal Semantic Memory (TSM)                                |                                                    2026-01-12 | Store by occurrence time, not only dialogue time; use durative memory and temporal intent                                                  | Time-valid retrieval                                | **High**                                 | **Copy** occurrence/effective time fields and temporal filters                                                           | ([arXiv][10])  |
| SYNAPSE                                                       |                                   2026-01-06; rev. 2026-02-16 | Dynamic graph + spreading activation + temporal decay + hybrid retrieval                                                                   | Multi-hop associative retrieval                     | **Medium**                               | **Adapt** as second-hop local reranking; **reject** graph-first global memory                                            | ([arXiv][11])  |
| MIRIX                                                         |                                                    2025-07-10 | Six memory types + multi-agent controller + multimodal lifelogging                                                                         | Rich multimodal personal memory                     | **Low**                                  | **Copy** only the memory-type taxonomy; **reject** the multi-agent, screenshot-monitoring system                         | ([arXiv][12])  |
| Improving Code Localization with Repository Memory            |                                   2025-10-01; rev. 2026-02-06 | Mine commit history into reusable repository memory                                                                                        | Repository localization                             | **Very high**                            | **Copy** commit-history memory; **defer** ambitious bug-fixing integration                                               | ([arXiv][2])   |
| MemGovern                                                     |                                                    2026-01-11 | Govern issue/PR history into “experience cards” and search them                                                                            | Retrieval from historical human experience          | **Medium**                               | **Adapt** the card/provenance idea; **reject** large-scale GitHub mining for v1                                          | ([arXiv][13])  |
| Your Code Agent Can Grow Alongside You with Structured Memory |                                                    2026-02-25 | Structured project history plus reasoning trajectories and feedback                                                                        | Temporal project memory                             | **Medium-high**                          | **Adapt** trajectory/history lessons; **reject** full autonomous agent ambition                                          | ([arXiv][14])  |
| LoCoEval                                                      |                                                    2026-03-06 | First repo-oriented long-horizon conversational context benchmark                                                                          | Repo conversation memory evaluation                 | **High**                                 | **Copy** as external eval; **defer** benchmark-specific optimizations                                                    | ([arXiv][15])  |
| RepoReason                                                    |                                                    2026-01-07 | White-box repo reasoning benchmark using dynamic program slicing                                                                           | Failure analysis for reading/simulation/integration | **High**                                 | **Copy** diagnostic mindset; **defer** direct benchmark optimization                                                     | ([arXiv][16])  |
| CASS + cass-memory-system                                     |                                           accessed 2026-03-29 | Local searchable history, BM25/semantic/RRF hybrid, provenance, decay, maturity states, project overlay                                    | Practical local tooling pattern                     | **Very high**                            | **Copy** local-first search, provenance, decay, deterministic curation; **reject** cross-agent/global-memory assumptions | ([GitHub][17]) |
| TOON (repo/spec/benchmark)                                    | 2025-11-24 spec; 2026-01-17 benchmark; 2026-03-04 Rust v0.4.4 | Compact, schema-aware structured format for LLM prompts                                                                                    | Token efficiency for structured context             | **Medium**                               | **Adopt selectively** for prompt packing; **reject** as canonical storage or mandatory IPC                               | ([GitHub][18]) |

## 3. Jeffrey Emanuel / flywheel lessons

### What to copy

Jeff’s most useful contribution for Roger is **not** the flywheel narrative. It is the concrete local-systems pattern underneath it: a searchable local history, an authoritative local store plus fast indexes, hybrid lexical+semantic retrieval, explicit provenance, decay, anti-pattern capture, and “why did this memory surface?” introspection. CASS is especially relevant because it is a Rust/Tantivy/FastEmbed local tool for agent-session search, and cass-memory adds provenance fields, maturity states, staleness tools, and project-local overlays. ([Jeffrey Emanuel][19])

### What to strip away

Roger is **not** trying to be a 14-tool multi-agent ecosystem, and it is **not** a cross-agent memory commons. Strip away cross-agent enrichment, default server/`serve` assumptions, MCP/HTTP-first integration, global playbooks, and the idea that every agent should learn from every other agent automatically. That is solving a larger orchestration problem than Roger has. MIRIX has the same mismatch: useful taxonomy, wrong scale and modality. ([Jeffrey Emanuel][19])

### What to adapt

Two Jeffrey ideas deserve direct adaptation. First, the **project-local overlay** pattern: `.cass/` in-repo plus a separate global store is a good model for `.roger/` repo memory plus explicit project/org overlays. Second, the **deterministic curator** pattern: cass-memory explicitly says the LLM proposes patterns while a non-LLM curator manages them by explicit rules, which is exactly what Roger needs for auditability and review safety. Roger should also adopt the privacy-audit idea whenever non-repo memory is consulted. ([GitHub][20])

## 4. Recommended Roger memory architecture

**Four design axioms.**

1. **Searchability and promotability are different.** Roger should store broad evidence, but promote only a thin slice into reusable memory.
2. **Scope is a hard boundary, not a ranking hint.** Repo is the default namespace. Project/org are explicit overlays.
3. **Memory should be evidence-weighted and change-aware.** Time decay alone is not enough for code review.
4. **Memory freshness must never block review.** If memory is stale or absent, Roger still reviews. It just reviews with less help.

### 4.1 Data model

Roger’s canonical source of truth should remain a **local SQLite-family relational store**, with large artifacts stored in a local content-addressed directory so primary tables stay small and the TUI stays responsive. That is consistent with Roger’s own plan and with the CASS pattern of “authoritative DB + speed-layer index.” ([GitHub][4])

I would define these core entities:

| Entity                      | Purpose                                        | Must carry                                                                                             |
| --------------------------- | ---------------------------------------------- | ------------------------------------------------------------------------------------------------------ |
| `scope`                     | repo / project / org namespace                 | `scope_id`, `kind`, `parent_scope_id`, `binding_policy`                                                |
| `source`                    | raw provenance object                          | `source_kind`, `locator`, `version/hash`, `origin_scope_id`, `authored_at`, `observed_at`              |
| `episode`                   | durable review event, not full transcript dump | `event_type`, `session_id`, `paths[]`, `symbols[]`, `issue_refs[]`, `summary`                          |
| `memory_item`               | extracted semantic or procedural unit          | `memory_type`, `state`, `statement`, `normalized_key`, `confidence`, `trust_tier`                      |
| `evidence_link`             | why a memory exists                            | `memory_id`, `source_id/episode_id`, `relation`, `span/excerpt`                                        |
| `edge`                      | thin typed associations                        | `same_file`, `same_module`, `same_issue`, `supports`, `contradicts`, `supersedes`, `caused_regression` |
| `usage_event`               | feedback loop                                  | `surfaced_to`, `accepted/rejected`, `approved_for_posting`, `merged`, `regressed`                      |
| `index_job` / `index_state` | non-blocking indexing                          | `index_kind`, `scope_id`, `schema_hash`, `dirty`, `generation`                                         |

My synthesis: **do not make full prompt/tool transcripts the primary memory corpus.** Store them for audit in cold artifacts, but index the high-signal durable objects: findings, decisions, user notes, session summaries, commit/issue summaries, repo docs, and promoted rules. This directly avoids the “naive summarization / raw unstructured logs” problem called out by ProMem and cass-memory. ([arXiv][9])

### 4.2 Memory types

Roger should explicitly separate four memory classes:

* **Working memory**: current PR, diff, open files, current task state, unsaved notes. Ephemeral. Useful for the current step, not for long-term promotion.
* **Episodic memory**: past review sessions, findings, approvals/dismissals, linked commits/issues, outcome snapshots. Searchable immediately.
* **Semantic memory**: extracted facts and durable patterns like “touching module X often requires fixture Y” or “this test name refers to a flaky Windows path issue.” Candidate first, promoted later.
* **Procedural memory**: review playbook rules and policy constraints like “security review for auth paths must check token expiry before session invalidation.” Mostly human-authored or explicitly validated.

This is conceptually close to the recent memory literature’s separation of episodic/semantic/procedural memory, but Roger should **not** copy MIRIX’s full six-part multimodal taxonomy or A-MEM’s dynamic network as-is. ([arXiv][12])

### 4.3 Scope model

Roger’s hierarchy should look like this:

```text
org scope      = canonical company policies + explicitly approved cross-repo lessons
project scope  = explicitly related repos / shared subsystem knowledge
repo scope     = default active namespace for every review
```

The crucial rule is: **hierarchy is for provenance and optional overlay, not for automatic inheritance**.

My recommendation:

* **Default** search set = current repo only.
* **Project** search requires an explicit session toggle or command.
* **Org** search requires an explicit session toggle or binding.
* **No automatic fallback** from repo to project/org when repo search is weak.
* **Bindings** are the only exception: a repo may explicitly bind selected project/org policy sets into its review context. Bound items still carry their original scope and should be shown as overlays, not flattened into repo memory.

To prevent silent bleed, I would keep **separate lexical and vector indices per scope** and run an explicit union only when the session allows it. For a single developer, the duplication cost is acceptable; the safety gain is large. When broader scopes are enabled, the UI and the model context should keep them in separate buckets: `repo_memory`, `project_overlay`, `org_policy`. That avoids hidden mixing.

### 4.4 Temporal model

TSM’s key idea translates directly: Roger must store both **when it learned something** and **when that thing actually applied**. For developer memory, those are not the same. A rule might be observed during a review on March 20, but apply to a bug pattern that started after a dependency bump on February 02. Roger should therefore store `observed_at`, `effective_from`, and `effective_to` on memory items when possible. Temporal query expansion should activate on phrases like “last time,” “recent,” “after the migration,” “before commit X,” or “since this test started flaking.” ([arXiv][10])

### 4.5 Retrieval stack

LongMemEval’s decomposition ideas, CASS’s hybrid retrieval, SYNAPSE’s associative retrieval, and Roger’s latency-sensitive constraints point to the same pipeline: **hard filters first, lexical first, semantic second, graph expansion third, rerank last**. ([arXiv][5])

I would implement:

```text
query formation
  -> hard scope/trust/type filters
  -> lexical retrieval (Tantivy)
  -> semantic retrieval (local vectors, curated corpus only)
  -> weighted RRF fusion
  -> local typed-edge expansion on top seeds
  -> final rerank
  -> abstain or package small evidence packet
```

Concrete behavior:

1. **Query formation**

   * Start from the current review task, not from free-form chat alone.
   * Extract deterministic anchors first: file paths, symbols, test names, issue IDs, dependency names, finding categories.
   * Add LLM-generated semantic handles only as low-trust expansions.
   * Use LongMemEval-style **fact-augmented key expansion** and time-aware expansion, but scoped to the current repo/session. ([arXiv][5])

2. **Hard filter**

   * Allowed scopes.
   * Repo ID.
   * Trust floor.
   * Memory type whitelist.
   * Path/module anchor overlap if current diff gives you anchors.
   * For project/org overlays, search only **canonical / established / proven** items by default.

3. **Lexical retrieval**

   * This is the primary retriever.
   * Search fields should heavily boost exact `paths`, `symbols`, `finding_type`, `issue_id`, `title`, then `summary/body`.
   * Lexical should dominate because code review depends heavily on exact identifiers, file names, errors, and policy names. CASS’s default mode reflects that: lexical is the default for exact-term/code search. ([GitHub][17])

4. **Semantic retrieval**

   * Ship it on day 1.
   * Restrict the vector corpus to:

     * promoted semantic/procedural memory
     * repo docs and ADR excerpts
     * session summaries
     * commit/issue summaries
     * compact symbol/module descriptors
   * **Do not** embed raw full code files or raw chat transcripts in v1.
   * For a single-developer corpus, use a small local embedding model and a simple local vector store; brute-force or very light indexing is enough initially.

5. **Fusion**

   * Use **weighted RRF**, with lexical weighted higher than semantic.
   * Initial default:

     * `w_lex = 1.0`
     * `w_sem = 0.65`
   * Then add bonuses/penalties for scope proximity, trust, outcome success, and path overlap.

6. **Associative / graph expansion**

   * Only after you have top lexical/semantic seeds.
   * One or two hops max.
   * Only over typed relations Roger actually understands.
   * This is where SYNAPSE is useful: not as Roger’s primary memory architecture, but as inspiration for **localized spreading activation** over `supports`, `same_module`, `same_issue`, `supersedes`, and `contradicts`. ([arXiv][11])

7. **Rerank**

   * Prefer:

     * repo over project over org
     * canonical over inferred
     * exact anchor overlap over generic similarity
     * accepted/merged history over unused candidates
     * recently validated over stale
   * Penalize contradictions unless the task is explicitly asking for conflict history.

8. **Abstain**

   * If the top results are weak or out-of-scope, return nothing.
   * LongMemEval explicitly treats abstention as a memory capability. Roger should prefer **no memory** to wrong memory. ([arXiv][5])

### 4.6 Promotion model

cass-memory’s maturity states, provenance fields, and decay ideas are the right template, but Roger should tune them around **review outcomes**, not general agent success. ([GitHub][20])

I would use this state machine:

| State          | Meaning                                                | Default surfacing                                               |
| -------------- | ------------------------------------------------------ | --------------------------------------------------------------- |
| `observed`     | Raw durable evidence exists                            | Searchable in history only                                      |
| `candidate`    | Extracted fact/rule with at least one evidence link    | Hidden from default model context; visible in low-confidence UI |
| `established`  | Repeatedly supported or explicitly promoted            | Eligible for retrieval                                          |
| `proven`       | Repeatedly useful with no harmful signal, or canonical | Preferred default retrieval                                     |
| `deprecated`   | Stale, contradicted, or harmful                        | Hidden unless asked                                             |
| `anti_pattern` | Harmful lesson to avoid                                | Retrieved as warning only                                       |

Concrete initial promotion rules:

* `observed -> candidate`

  * after a session checkpoint/end, if extraction produces a structured fact/rule with at least one evidence link.
* `candidate -> established`

  * if it appears in **2 independent episodes**, or is **explicitly promoted by the user**, or is backed by a **canonical checked-in doc**.
* `established -> proven`

  * if it is used successfully in **3 approved findings/comments**, or linked to a **merged fix** without later contradiction/regression, or imported from a bound project/org canonical policy.
* `any -> deprecated`

  * if contradicted by a newer canonical source, repeatedly rejected, or invalidated by major code/module change.
* `any -> anti_pattern`

  * if it causes a false-positive cascade, bad review guidance, or a regression-associated recommendation.

My synthesis: **negative evidence should count more than positive evidence** for Roger. A harmful review heuristic is more costly than a merely helpful one is beneficial.

### 4.7 Forgetting, pruning, decay, demotion

MemoryAgentBench is right to treat selective forgetting as a first-class capability, and cass-memory is right that time decay matters because codebases and tools evolve. But for Roger, decay must be **change-aware**, not just date-aware. ([arXiv][6])

I would use:

* **Candidates**

  * expire fast if never validated: e.g. 21–30 days.
* **Established heuristics**

  * 90-day half-life unless recently validated.
* **Proven heuristics**

  * 180-day half-life unless attached to canonical docs.
* **Canonical docs/policies**

  * no time decay; invalidate on source revision instead.
* **Episodic history**

  * never hard-delete by default if tied to audit; archive/cold-rank instead.

And add **change-aware demotion**:

* if the memory’s anchor files/modules changed materially,
* if a dependency major version changed,
* if a linked policy doc was revised,
* if the same advice starts being dismissed.

This is the right place to model repo “epochs.”

### 4.8 Duplicate suppression and conflict resolution

Do not silently overwrite memory.

* Deduplicate **within scope and type** using normalized text + anchor sets + near-duplicate similarity.
* Across scopes, link duplicates as aliases; do **not** merge them. The same statement can mean different things in repo vs org context.
* Conflicts become explicit `contradicts` or `supersedes` edges.
* Canonical docs beat inferred rules.
* Newer higher-trust items beat older lower-trust items.
* If two high-trust items still conflict, show both with provenance.

### 4.9 Non-blocking indexing model

Roger’s own principles already say review is read-heavy and latency-sensitive, sessions must degrade gracefully, and no hidden daemon should sit at the center. Tantivy also makes newly indexed docs visible only after `commit` and reader reload, while LightMem argues consolidation should move offline. Put those together and the right architecture is: **synchronous DB writes, asynchronous index/consolidation work, old index stays serving until new one is ready.** ([GitHub][4])

I would implement:

* **Foreground path**

  * write durable state to DB
  * mark dirty rows / dirty ranges
  * optionally update a tiny in-memory “dirty overlay”
* **Background path (same process, worker threads, no daemon)**

  * lexical reindex of dirty docs
  * embeddings for newly eligible docs
  * candidate extraction and consolidation
  * dedupe / decay / promotion evaluation
* **Query path**

  * search committed index
  * fuse in small dirty overlay scan
  * if semantic unavailable, return lexical-only
  * if index missing/corrupt, fall back to DB scan + file/doc search
* **Full rebuild path**

  * build new index generation from DB snapshot
  * atomically swap generations
  * keep old generation serving until swap succeeds

That is non-blocking, local-first, and daemonless.

## 5. Reindexing and memory lifecycle

**Observed.** CASS’s schema-hash rebuild pattern is sound: the DB is authoritative, and index generations can be rebuilt when schema or integrity changes. Tantivy’s commit/reload model also means you should not pretend reindexing is free or instantaneous. Roger should adopt those patterns explicitly. ([GitHub][17])

### What is searchable immediately vs candidate vs promoted

| Class                                                  | Searchable immediately?               | Candidate memory? | Promotion requirement                                   |
| ------------------------------------------------------ | ------------------------------------- | ----------------- | ------------------------------------------------------- |
| Repo docs / ADRs / CONTRIBUTING / SECURITY             | Yes                                   | No                | Canonical by source                                     |
| Roger findings / decisions / notes / session summaries | Yes                                   | No                | Episodic evidence only                                  |
| Commit messages + touched paths + linked issue IDs     | Yes                                   | No                | Episodic evidence only                                  |
| Raw prompt/tool transcripts                            | Not by default; cold archive          | No                | Manual pin only                                         |
| LLM-extracted facts                                    | Low-confidence only                   | Yes               | Repetition, human promotion, or canonical corroboration |
| Review heuristics / procedures                         | Not by default                        | Yes               | Approval, merge, or repeated successful use             |
| Project/org policies                                   | Only when bound or explicitly enabled | Sometimes         | Canonical import or strong repeated validation          |

### Exact trigger points

| Trigger                                                | Foreground work                      | Background work                                            | Incremental or full rebuild?                |
| ------------------------------------------------------ | ------------------------------------ | ---------------------------------------------------------- | ------------------------------------------- |
| Finding created/edited/dismissed/approved              | DB write, dirty mark                 | Reindex finding text, refresh candidate links              | Incremental                                 |
| User note or manual rule edit                          | DB write                             | Reindex note/rule; reevaluate duplicates                   | Incremental                                 |
| Session checkpoint/end                                 | Finalize episodic summary + outcomes | Extract candidates, decay, dedupe, vectorize               | Incremental                                 |
| New commit / rebase / merge-base change                | Record commit metadata               | Reindex commit/path summaries; invalidate affected anchors | Incremental unless history identity changes |
| Repo doc/policy file change                            | New source version                   | Rechunk/reembed changed doc; revalidate dependent memory   | Incremental                                 |
| Repo/project/org binding change                        | Update allowed scopes/bindings       | Build missing overlay indices if needed                    | Incremental                                 |
| Tokenizer / indexed fields / chunking change           | Record new schema                    | Rebuild affected lexical index                             | **Full lexical rebuild**                    |
| Embedding model / vector dimension change              | Record model/version/hash            | Rebuild affected vector store                              | **Full vector rebuild**                     |
| Index corruption / missing meta / schema hash mismatch | Degraded mode continues              | Rebuild from DB snapshot                                   | **Full rebuild**                            |
| Explicit `rr memory rebuild`                           | None except status                   | Fresh generation build + atomic swap                       | **Full rebuild**                            |

### Background vs foreground

Foreground work should be limited to **durable state changes** and extremely cheap metadata updates. Background work should handle everything that can lag without harming the review loop: embeddings, consolidation, graph edges, and promotion evaluation.

### Promotion and demotion rules

Initial defaults I would ship:

* **Promote to repo-established**

  * 2 independent supporting episodes, or explicit user promote.
* **Promote to repo-proven**

  * 3 successful approved/merged uses, or canonical doc backing.
* **Promote repo -> project**

  * explicit user action **and** validation across at least 2 repos or repeated use across a project boundary.
* **Promote project -> org**

  * explicit import from canonical org docs, or explicit human decision. Never automatic.
* **Demote**

  * canonical contradiction, repeated dismissals, harmful outcome, or anchor invalidation after major repo change.

## 6. TOON / structured-context guidance

**Observed.** The official TOON repo positions TOON as a compact, schema-aware JSON encoding for LLM prompts and claims mixed-structure benchmark gains with fewer tokens. The official spec is **TOON v3.0, dated 2025-11-24, status “Working Draft.”** The benchmark paper, however, reports a real trade-off: TOON can be more compact and lower-emission, but may have **lower structural correctness when models lack native support**. The official repo also explicitly says TOON is a bad fit for deeply nested/non-uniform data, loses against CSV for pure flat tables, and may lose to compact JSON on latency-critical local/quantized setups. ([GitHub][18])

**Recommendation.** Roger should **adopt TOON selectively**, not universally.

### Where TOON helps

TOON is a good fit when Roger passes large, mostly tabular structured packets to an LLM, such as:

* findings table
* similar historical findings table
* retrieved memory cards with fields like `scope`, `trust`, `evidence_count`, `why`
* commit/issue summary tables
* evidence matrices linking finding ↔ file ↔ rule ↔ outcome

That is exactly the kind of “uniform arrays of objects” the TOON repo optimizes for. ([GitHub][18])

### Where TOON hurts

Use compact JSON instead for:

* deeply nested AST/config trees
* heterogenous objects with lots of optional substructures
* very small payloads
* low-latency local-model runs where TTFT matters more than raw token count
* internal Rust/TS IPC where ordinary typed structs already work cleanly

The official TOON materials explicitly warn about these cases. ([GitHub][18])

### Roger adoption decision

My recommendation:

* **Canonical storage**: JSON / rows in DB, not TOON.
* **Internal IPC**: compact JSON, not TOON.
* **Prompt packaging to LLMs**: TOON as an **optional packer** behind a feature flag.
* **Default**: compact JSON.
* **Auto-selection heuristic**:

  * if tabular eligibility is high and payload is large, try TOON
  * otherwise use compact JSON
  * only enable TOON for a model/backend after passing a model-specific smoke test for structure correctness

A simple rule is enough for v1:

```text
if payload_is_large
   and arrays_are_mostly_uniform
   and model_has_passed_toon_smoke_tests
then use TOON
else use compact JSON
```

Given that the Rust implementation is already spec-compliant and at **v0.4.4 on 2026-03-04**, Roger can experiment with this safely as a leaf dependency. It just should not bet the whole architecture on it. ([GitHub][21])

## 7. Dependency and toolchain recommendations

### Justified dependencies

**Tantivy: yes.** Tantivy is unusually well-matched to Roger: stable Rust, **<10 ms startup**, BM25, phrase queries, incremental indexing, mmap directories, JSON fields, and distributed search explicitly out of scope. For a local CLI/TUI product, that last point is a feature. ([GitHub][22])

**FastEmbed-class local embeddings: yes, but narrow.** `fastembed` is explicitly a fast local retrieval-embedding library, and `fastembed-rs` supports synchronous local usage with ONNX/tokenizers and local reranking. CASS demonstrates the right operational posture for Roger: local-only semantics, no cloud calls, and explicit/manual model install instead of surprise downloads. ([Docs.rs][23])

**Chrome Native Messaging: yes, if Roger wants deeper browser integration.** Chrome starts the native host in a **separate process** over `stdin/stdout`, and `runtime.sendNativeMessage()` starts a new host process per message while `connectNative()` keeps it alive only for the lifetime of the port. That matches Roger’s daemonless constraint far better than a local HTTP/WebSocket service. ([Chrome for Developers][24])

### Risky dependencies

**FrankenSQLite / fsqlite: promising, but do not hard-couple Roger to it yet.** Official materials emphasize concurrent writers, self-healing storage, page-level encryption, and a rusqlite-compatible adapter. But the repo/docs also say SQL execution parity is still incomplete, the public API is still growing, and **Rust nightly** is required. For Roger—a single-user, read-heavy, latency-sensitive review tool—the concurrency upside is real but not central. My recommendation is to keep Roger’s domain/storage interface portable and treat fsqlite as a backend choice, not an architectural premise. ([FrankenSQLite][25])

**FSQLite FTS5: optional fallback, not primary lexical engine.** It is useful that `fsqlite_ext_fts5` exists and supports BM25, phrase/prefix/NEAR queries, snippets, and highlights. But Roger should not run two primary lexical engines on the hot path. Use Tantivy as the primary search layer; keep FTS5 as a fallback/debug/admin capability only if needed. ([Docs.rs][26])

**TOON as core protocol: no.** Use it as a leaf packer, not as canonical storage, not as required IPC.

### Avoid

* hosted vector DBs
* hosted graph DBs
* a full graph database
* an always-on daemon / local HTTP server
* automatic model downloads by default
* mandatory FSVI/frankensearch adoption before measured need
* ambient org-wide semantic search

### Pinning / toolchain guidance

Pin separately:

* lexical index schema version
* vector schema version
* embedding model name, checksum, dimension, and feature flags
* tokenizer/chunking/extractor versions
* trust/promotion rules version
* TOON spec / crate version if used
* repo/project/org binding configuration

And if you do adopt fsqlite, isolate the nightly requirement to a backend crate; keep the core memory domain portable.

## 8. Evaluation plan

### Benchmarks to use

| Benchmark        | Why use it                                                                   | Roger-specific interpretation                                                                          | Sources        |
| ---------------- | ---------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ | -------------- |
| LongMemEval      | Best early benchmark for extraction, updates, temporal reasoning, abstention | Good for memory retrieval quality and abstention behavior                                              | ([arXiv][5])   |
| MemoryAgentBench | Covers retrieval, learning, long-range understanding, selective forgetting   | Good for change-aware forgetting and update tests                                                      | ([arXiv][6])   |
| LoCoEval         | Repo-oriented long-horizon conversational memory benchmark                   | Good for session continuity and repo-memory packaging                                                  | ([arXiv][15])  |
| RepoReason       | White-box repo reasoning benchmark                                           | Good for diagnosing whether memory helps integration width vs only retrieval                           | ([arXiv][16])  |
| SWE-QA-Pro       | Long-tail repository QA with executable environments                         | Good for evaluating repo understanding and history usefulness                                          | ([arXiv][27])  |
| SecRepoBench     | Secure repo-level tasks in real repos                                        | Good proxy for security-review memory value                                                            | ([arXiv][28])  |
| SWE-bench-Live   | Fresh, contamination-resistant downstream issue benchmark                    | External validity check; not Roger’s primary benchmark because it is fix-oriented, not review-oriented | ([GitHub][29]) |

### Product-specific tests to add

Roger needs tests that the public benchmarks do not cover:

* **Scope-bleed@K**: repo-only queries should return **zero** project/org items in top-K unless scope is explicitly widened.
* **Exact-anchor recall**: file paths, symbols, test names, issue IDs, error strings.
* **Accepted-finding recall**: similar previously approved findings should surface quickly.
* **Stale-memory suppression**: outdated heuristics should stop surfacing after source/doc/module change.
* **Conflict surfacing**: contradictory memories should be exposed, not flattened.
* **Promotion precision**: promoted memories should have a high later-helpful / low harmful rate.
* **Abstention quality**: when Roger lacks good memory, it should say so.
* **Degraded-mode correctness**: review should still work during rebuilds or with semantic search unavailable.
* **Packaging evaluation**: JSON vs TOON on token count, TTFT, parse success, answer quality.

### Failure modes to measure

The failure modes that matter most are:

* out-of-scope leakage
* stale advice after refactors or dependency upgrades
* duplicate-memory loops
* silent conflict overwrites
* over-promotion from one-off sessions
* semantic retrieval outranking exact lexical evidence
* indexing lag becoming user-visible friction
* TOON format regressions on unsupported models

## 9. Concrete implementation recommendation for Roger

### Build now

* A **local relational canonical store** with explicit `scope`, `source`, `episode`, `memory_item`, `evidence_link`, `usage_event`, and `index_state`.
* **Repo-first default scope** with separate per-scope indices and explicit project/org overlays.
* **Tantivy lexical retrieval** as the primary engine.
* **Local semantic retrieval on day 1**, but only over curated/promoted text plus selected episodic summaries and doc excerpts.
* **Event-based session decomposition** around review events, not giant transcript summaries.
* **Deterministic curator logic** for promotion/demotion; LLMs only propose candidate facts/rules.
* **Outcome-aware promotion** using approval, merge, repeated usefulness, and harmful feedback.
* **Non-blocking index generations** with background rebuilds and atomic swaps.
* **JSON as canonical context packer**, with a TOON packer behind a feature flag.
* **Audit-friendly UI surfacing** of `scope`, `trust`, `why`, and evidence links for every retrieved item.

### Build later

* Localized typed-edge spreading activation on top of seed results.
* A lightweight reranker on top 20–30 candidates.
* Code-aware or sparse embedding experiments if the first dense model underperforms. `fastembed-rs` already exposes code-oriented and sparse embedding options, so this can be evaluated later without changing the architecture. ([GitHub][30])
* Better repo-epoch detection for change-aware decay.
* Manual cross-repo/project promotion workflows.
* Optional TOON auto-selection once model-specific benchmarks are in place.

### Explicitly reject

* Ambient global memory.
* Silent repo → project → org inheritance.
* Multi-agent flywheel architecture.
* Always-on daemon / local HTTP server at the center.
* Hosted vector or graph infrastructure.
* Full graph DB before the typed relational graph is exhausted.
* Automatic GitHub posting.
* Automatic org-level promotion from one repo’s inferred memory.
* Indexing every raw model/tool trace as first-class reusable memory.

## 10. Open risks and unresolved questions

1. **Project boundary definition** is still a human product decision. If “project” is vague, the scope model will leak or under-share.
2. **Embedding model choice** is still open. A small local dense model is sufficient to start, but mixed code/NL corpora may eventually favor a code-oriented or sparse model.
3. **FrankenSQLite maturity** may improve quickly, but Roger should not assume it. The nightly requirement and still-growing API surface are real present-day costs. ([Docs.rs][31])
4. **TOON viability depends on the actual model mix** Roger will use. The official benchmark shows the trade-off is real. ([arXiv][32])
5. **Outcome labeling** is harder in review than in patch-generation. “Helpful,” “approved,” and “merged without later regret” need careful product definitions.
6. **Org/company memory ingestion** needs an allowlist. Roger should ingest only explicitly approved company sources, not ambient unrelated data.
7. **Repo history ingestion depth** is a product trade-off. I would start with repo docs, Roger history, and a bounded slice of commit/issue history, not giant backfills.
8. **Most code-memory research here is fresh preprint work.** Roger should borrow the ideas, not the hype. ([arXiv][2])

## Source index: exact links and dates

Dates below are source dates where available; for rolling repos/docs, I note **accessed 2026-03-29**.

```text
2024-10-14 (rev. 2025-03-04)  LongMemEval
https://arxiv.org/abs/2410.10813

2025-07-07 (rev. 2026-03-17)  Evaluating Memory in LLM Agents via Incremental Multi-Turn Interactions (MemoryAgentBench)
https://arxiv.org/abs/2507.05257

2025-04-28  Mem0: Building Production-Ready AI Agents with Scalable Long-Term Memory
https://arxiv.org/abs/2504.19413

2025-10-21 (rev. 2026-02-28)  LightMem: Lightweight and Efficient Memory-Augmented Generation
https://arxiv.org/abs/2510.18866

2026-01-08  Beyond Static Summarization: Proactive Memory Extraction for LLM Agents
https://arxiv.org/abs/2601.04463

2026-01-12  Beyond Dialogue Time: Temporal Semantic Memory for Personalized LLM Agents
https://arxiv.org/abs/2601.07468

2026-01-06 (rev. 2026-02-16)  SYNAPSE
https://arxiv.org/abs/2601.02744

2025-07-10  MIRIX
https://arxiv.org/abs/2507.07957

2025-02-17 (rev. 2025-10-08)  A-MEM
https://arxiv.org/abs/2502.12110

2025-10-01 (rev. 2026-02-06)  Improving Code Localization with Repository Memory
https://arxiv.org/abs/2510.01003

2026-01-11  MemGovern
https://arxiv.org/abs/2601.06789

2026-02-25  Your Code Agent Can Grow Alongside You with Structured Memory
https://arxiv.org/abs/2603.13258

2026-03-06  LoCoEval
https://arxiv.org/abs/2603.06358

2026-01-07  RepoReason
https://arxiv.org/abs/2601.03731

2025-04-29 (rev. 2026-02-14)  SecRepoBench
https://arxiv.org/abs/2504.21205

2026-03-17  SWE-QA-Pro
https://arxiv.org/abs/2603.16124

2025-05-29  SWE-bench Goes Live! / SWE-bench-Live
https://www.microsoft.com/en-us/research/publication/swe-bench-goes-live/
https://github.com/microsoft/SWE-bench-Live

2025-11-24  TOON specification v3.0 (Working Draft)
https://github.com/toon-format/spec

2026-01-17  Are LLMs Ready for TOON?
https://arxiv.org/abs/2601.12014

2026-03-04  toon-rust v0.4.4
https://github.com/toon-format/toon-rust

accessed 2026-03-29  TOON official repo
https://github.com/toon-format/toon

accessed 2026-03-29  Jeffrey Emanuel TLDR
https://jeffreyemanuel.com/tldr

accessed 2026-03-29  CASS repo
https://github.com/Dicklesworthstone/coding_agent_session_search

accessed 2026-03-29  cass-memory-system repo
https://github.com/Dicklesworthstone/cass_memory_system

accessed 2026-03-29  Roger Reviewer repo
https://github.com/cdilga/roger-reviewer

accessed 2026-03-29  Roger plan doc
https://github.com/cdilga/roger-reviewer/blob/main/docs/PLAN_FOR_ROGER_REVIEWER.md

accessed 2026-03-29  Tantivy README
https://github.com/quickwit-oss/tantivy/blob/main/README.md

accessed 2026-03-29  Tantivy query docs
https://docs.rs/tantivy/latest/tantivy/query/

accessed 2026-03-29  fastembed docs
https://docs.rs/fastembed/latest/fastembed/

accessed 2026-03-29  fastembed-rs repo
https://github.com/anush008/fastembed-rs

accessed 2026-03-29  FrankenSQLite site
https://frankensqlite.com/

accessed 2026-03-29  FrankenSQLite repo
https://github.com/Dicklesworthstone/frankensqlite

accessed 2026-03-29  fsqlite docs
https://docs.rs/fsqlite/latest/fsqlite/

accessed 2026-03-29  fsqlite_ext_fts5 docs
https://docs.rs/fsqlite-ext-fts5/latest/fsqlite_ext_fts5/

accessed 2026-03-29  Chrome Native Messaging docs
https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging
```

[1]: https://arxiv.org/abs/2410.10813?utm_source=chatgpt.com "[2410.10813] LongMemEval: Benchmarking Chat Assistants on Long-Term Interactive Memory"
[2]: https://arxiv.org/abs/2510.01003 "https://arxiv.org/abs/2510.01003"
[3]: https://github.com/cdilga/roger-reviewer "https://github.com/cdilga/roger-reviewer"
[4]: https://github.com/cdilga/roger-reviewer/blob/main/docs/PLAN_FOR_ROGER_REVIEWER.md "https://github.com/cdilga/roger-reviewer/blob/main/docs/PLAN_FOR_ROGER_REVIEWER.md"
[5]: https://arxiv.org/abs/2410.10813 "https://arxiv.org/abs/2410.10813"
[6]: https://arxiv.org/abs/2507.05257 "https://arxiv.org/abs/2507.05257"
[7]: https://arxiv.org/abs/2504.19413 "https://arxiv.org/abs/2504.19413"
[8]: https://arxiv.org/abs/2510.18866 "https://arxiv.org/abs/2510.18866"
[9]: https://arxiv.org/abs/2601.04463 "https://arxiv.org/abs/2601.04463"
[10]: https://arxiv.org/abs/2601.07468 "https://arxiv.org/abs/2601.07468"
[11]: https://arxiv.org/abs/2601.02744 "https://arxiv.org/abs/2601.02744"
[12]: https://arxiv.org/abs/2507.07957 "https://arxiv.org/abs/2507.07957"
[13]: https://arxiv.org/abs/2601.06789 "https://arxiv.org/abs/2601.06789"
[14]: https://arxiv.org/abs/2603.13258 "https://arxiv.org/abs/2603.13258"
[15]: https://arxiv.org/abs/2603.06358 "https://arxiv.org/abs/2603.06358"
[16]: https://arxiv.org/abs/2601.03731 "https://arxiv.org/abs/2601.03731"
[17]: https://github.com/Dicklesworthstone/coding_agent_session_search "https://github.com/Dicklesworthstone/coding_agent_session_search"
[18]: https://github.com/toon-format/toon "https://github.com/toon-format/toon"
[19]: https://jeffreyemanuel.com/tldr "https://jeffreyemanuel.com/tldr"
[20]: https://github.com/Dicklesworthstone/cass_memory_system "https://github.com/Dicklesworthstone/cass_memory_system"
[21]: https://github.com/toon-format/toon-rust "https://github.com/toon-format/toon-rust"
[22]: https://github.com/tantivy-search/tantivy/blob/master/README.md "https://github.com/tantivy-search/tantivy/blob/master/README.md"
[23]: https://docs.rs/fastembed/ "https://docs.rs/fastembed/"
[24]: https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging "https://developer.chrome.com/docs/extensions/develop/concepts/native-messaging"
[25]: https://frankensqlite.com/ "https://frankensqlite.com/"
[26]: https://docs.rs/fsqlite-ext-fts5 "https://docs.rs/fsqlite-ext-fts5"
[27]: https://arxiv.org/abs/2603.16124 "https://arxiv.org/abs/2603.16124"
[28]: https://arxiv.org/abs/2504.21205 "https://arxiv.org/abs/2504.21205"
[29]: https://github.com/microsoft/SWE-bench-Live "https://github.com/microsoft/SWE-bench-Live"
[30]: https://github.com/anush008/fastembed-rs "https://github.com/anush008/fastembed-rs"
[31]: https://docs.rs/fsqlite "https://docs.rs/fsqlite"
[32]: https://arxiv.org/abs/2601.12014 "https://arxiv.org/abs/2601.12014"
