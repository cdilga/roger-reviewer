# Search Memory Lifecycle and Semantic Asset Policy

This document narrows the canonical plan for Roger Reviewer `0.1.0`.

Authority:

- [`AGENTS.md`](/Users/cdilga/Documents/dev/roger-reviewer/AGENTS.md)
- [`PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md)
- [`adr/004-scope-and-memory-promotion-policy.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/adr/004-scope-and-memory-promotion-policy.md)
- [`DATA_MODEL_AND_STORAGE_CONTRACT.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/DATA_MODEL_AND_STORAGE_CONTRACT.md)

It exists to make the search and memory policy executable before `rr-024`
implements prior-review lookup and memory hooks.

## Purpose

Roger `0.1.0` needs one explicit policy for:

- scope partitioning and retrieval lanes
- promotion, demotion, and conflict handling
- duplicate handling and rebuild semantics
- semantic asset install, verification, and degraded-mode rules
- usage and outcome vocabulary that is sufficient for later usefulness signals
  without turning analytics into a primary product surface

This document does not expand Roger's product scope. It freezes the policy that
implementation must follow.

## Source-of-Truth Boundary

- The canonical SQLite-family store is the only source of truth for `Source`,
  `Episode`, `MemoryItem`, `MemoryEdge`, `UsageEvent`, and index metadata.
- Tantivy lexical indices and semantic/vector sidecars are derived state.
- Roger must never treat an embedding file, vector index, or lexical sidecar as
  authoritative if it disagrees with the canonical store.
- Rebuilds must be possible from canonical relational state plus cold artifacts
  without reconstructing meaning from chat transcripts alone.

## Scope Partitioning

Roger must keep search and memory scope explicit.

### Scope classes

- `repo`: default active scope for all review and lookup flows
- `project`: explicit Roger-managed overlay across an allowlisted repo set
- `org`: explicit policy overlay only; not ambient company memory

### Scope rules

- There is no automatic `repo -> project -> org` fallback when repo results are
  weak.
- Broader scopes must be explicitly enabled by config or launch context.
- Lexical and semantic indices must remain partitioned by scope even when a
  query spans multiple allowed scopes.
- Cross-scope duplicates may be linked, but they must not collapse into one
  anonymous memory row.
- Query results must preserve provenance buckets such as `repo_memory`,
  `project_overlay`, and `org_policy`.

### Searchable versus promotable lanes

Roger must keep evidence retrieval separate from reusable memory retrieval.

- `evidence_hits`: raw findings, summaries, notes, docs, and episodic history
  searchable immediately with provenance
- `tentative_candidates`: `candidate` memory surfaced only for high anchor
  overlap or explicit user request
- `promoted_memory`: `established` and `proven` memory eligible for normal
  retrieval and prompt injection

Candidate memory must never silently behave like promoted memory.

## Canonical Search Operating Model

Roger should copy QMD-grade retrieval mechanics without copying QMD's authority
model. The canonical Roger contract separates search intent from executed engine
path.

### `query_mode`

`query_mode` describes why the operator or worker is searching.

Canonical values:

- `auto`: compatibility ingress only; Roger derives the narrowest truthful
  concrete search posture from current session context, anchors, and the
  supplied query before retrieval executes
- `exact_lookup`: direct lookup for a known object, locator, path, symbol, or
  prior finding identity
- `recall`: retrieve relevant prior evidence and promoted memory for the active
  review target
- `related_context`: expand around the current finding, artifact, or anchor set
  to pull nearby evidence and supporting memory
- `candidate_audit`: deliberately inspect tentative or contradicted memory that
  ordinary retrieval would normally suppress
- `promotion_review`: inspect candidate memory plus open review requests for
  promotion, demotion, deprecation, restoration, or anti-pattern marking

Rules:

- `query_mode` is the search-intent contract. It is not the same thing as the
  retrieval engine path.
- `auto` must resolve to one of the concrete planned modes above before
  retrieval executes or results are surfaced
- result envelopes and logs should preserve both the requested ingress mode and
  the resolved concrete mode when `auto` was supplied or implied
- omitted intent is only a compatibility ingress; the accepted steady-state
  planner contract is explicit intent plus explicit resolution truth
- new docs, tests, and bead acceptance should treat explicit intent selection as
  the normal front door rather than polishing omitted/implicit `auto`
- explicit candidate or promotion-review behavior must never be inferred from an
  ordinary `auto` query just because a candidate ranks well

Execution expectations by mode:

- `auto`
  - ingress convenience only; Roger resolves to the narrowest concrete mode
    supported by the active `SessionBaselineSnapshot`, current anchor set, and
    operator/task context
  - both requested and resolved mode must be surfaced in the result envelope
- `exact_lookup`
  - prioritize direct identifiers such as finding fingerprint, path, symbol,
    memory id, prompt invocation id, or review-run locator
  - do not widen silently into broad semantic recall when the exact object is
    missing; degrade truthfully instead
- `recall`
  - repo-first retrieval of prior evidence plus promoted memory relevant to the
    active review target or task
  - candidates stay suppressed unless explicit policy or baseline allows them
- `related_context`
  - anchor-centered expansion around the current finding, artifact, file,
    symbol, diff hunk, or prompt-stage result
  - should favor neighbor evidence, linked memory, and conflict/support edges
    over broad whole-repo recall
- `candidate_audit`
  - deliberately includes `tentative_candidates`, contradicted memory, or
    fragile heuristics that ordinary recall would hide
  - surfaced items default to `inspect_only` or `warning_only`, never ordinary
    cite posture
- `promotion_review`
  - joins candidate recall with open `MemoryReviewRequest` objects so the
    operator can review promotion, demotion, deprecation, restoration, and
    anti-pattern proposals in one place
  - result ordering should cluster duplicates, contradictions, and authority
    references rather than behaving like a plain relevance list

### `retrieval_mode`

`retrieval_mode` describes the engine posture Roger actually executed.

Required `0.1.x` values:

- `hybrid`
- `lexical_only`
- `recovery_scan`

Rules:

- `hybrid` means lexical retrieval plus verified semantic retrieval over the
  curated corpus
- `lexical_only` means Roger completed the request without semantic retrieval
  and must preserve explicit degraded reasons when hybrid was requested or would
  normally be available
- `recovery_scan` means Roger could not rely on the normal lexical sidecar path
  and is using a bounded canonical-DB or file/doc recovery path instead
- `recovery_scan` is a recovery-only posture, not a healthy-path default for
  ordinary review/search
- Roger must never silently present `recovery_scan` as if normal planned search
  were active

### `RecallEnvelope`

Every surfaced search or recall item should resolve to one canonical envelope,
even when a specific surface projects only a subset of fields inline.

Required fields:

- `item_kind`
- `item_id`
- `requested_query_mode`
- `resolved_query_mode`
- `retrieval_mode`
- `scope_bucket`
- `memory_lane`: `evidence_hits`, `tentative_candidates`, or
  `promoted_memory`
- `trust_state` nullable
- `source_refs`
- `locator`
- `snippet_or_summary`
- `anchor_overlap_summary`
- `degraded_flags`
- `explain_summary`
- `citation_posture`
- `surface_posture`

Required posture values:

- `citation_posture`
  - `cite_allowed`
  - `inspect_only`
  - `warning_only`
- `surface_posture`
  - `ordinary`
  - `candidate_review`
  - `operator_review_only`

Rules:

- every returned item must preserve enough information to answer "why did this
  surface now" without forcing the operator or worker to infer it from score
  alone
- `tentative_candidates` should normally use `inspect_only` or
  `operator_review_only` rather than `cite_allowed`
- contradicted or `anti_pattern` memory may surface only as `warning_only`
- CLI, TUI, and worker retrieval are projections of the same
  `RecallEnvelope`; they must not drift into separate semantics

### `MemoryReviewRequest`

Roger needs an explicit non-mutating request object for durable memory review.

Required fields:

- `id`
- `review_session_id`
- `review_run_id` nullable
- `source_surface`
- `request_kind`
- `subject_memory_id`
- `requested_target_state`
- `reason_summary`
- `supporting_refs`
- `status`
- `requested_by_actor_kind`
- `requested_by_actor_id` nullable
- `created_at`
- `resolved_at` nullable
- `resolved_by_actor_kind` nullable
- `resolved_by_actor_id` nullable
- `resolution_summary` nullable

Required `request_kind` values:

- `promote`
- `demote`
- `deprecate`
- `restore`
- `mark_anti_pattern`

Required `status` values:

- `pending_review`
- `accepted`
- `rejected`
- `superseded`
- `withdrawn`

Rules:

- a `MemoryReviewRequest` is auditable and non-mutating until accepted by
  Roger-owned review logic
- workers may propose memory review; they do not mutate durable memory directly
- the TUI is the dense operator review surface for accepting or rejecting these
  requests
- the extension must not become an independent promotion-review surface

### `SessionBaselineSnapshot`

Roger should model baseline search posture explicitly rather than deriving it
from the latest prompt turn heuristically.

Required fields:

- `id`
- `review_session_id`
- `review_run_id` nullable
- `baseline_generation`
- `review_target_snapshot`
- `allowed_scopes`
- `default_query_mode`
- `candidate_visibility_policy`
- `prompt_strategy`
- `policy_epoch_refs`
- `degraded_flags`
- `created_at`

Rules:

- the session baseline records the stable context Roger resolved for the current
  review lane before task-specific overrides are applied
- `ReviewTask` and `PromptInvocation` may narrow or specialize this baseline,
  but they do not replace it
- dropout, return, reseed, and refresh flows must be able to explain the
  current default search posture by resolving the active baseline snapshot

### Surface semantics

- `rr search` is the canonical operator and robot query plane. The accepted
  contract is explicit `query_mode`, explicit `retrieval_mode`, and explicit
  `RecallEnvelope` projection truth. Omitted intent may exist only as a
  compatibility ingress that Roger resolves before execution.
- the TUI `Search/History` destination is the dense operator workbench for
  recall inspection, candidate audit, and promotion review using the same
  underlying retrieval contract
- `rr agent ...` and `worker.search_memory` use the same recall contract but
  remain scoped to the current task/session envelope and cannot promote memory
  directly
- the extension may launch or mirror bounded search state later, but it is not
  an independent recall or promotion-review authority

### Related feature mode bindings

The search contract must stay coherent across adjacent surfaces.

- CLI `rr search`
  - the direct operator and robot ingress for all six canonical `query_mode`
    values
  - must surface requested mode, resolved mode, retrieval mode, scope bucket,
    lane counts, and degraded reasons
- TUI `Search/History`
  - the dense projection of the same contract
  - must let the operator pivot between `recall`, `related_context`,
    `candidate_audit`, and `promotion_review` without inventing a second search
    model
- TUI promotion-review workbench
  - a specialized projection of `promotion_review`
  - owns acceptance/rejection of `MemoryReviewRequest` objects, not independent
    memory mutation rules
- Worker retrieval
  - `worker.search_memory` and related in-session commands use the same
    `query_mode`/`retrieval_mode` semantics
  - worker results may propose follow-up or memory review, but they do not
    bypass Roger-owned promotion review
- Extension
  - may launch a bounded search or mirror resolved recall state
  - must not become a separate search planner, memory promoter, or authority
    source

## Memory Classes and States

### Memory classes

- `working`: ephemeral current-session state; never indexed as durable memory
- `episodic`: sessions, findings, approvals, dismissals, notes, and outcome
  snapshots; searchable immediately
- `semantic`: extracted facts and durable patterns; candidate first
- `procedural`: review rules and policy constraints; mostly human-authored or
  explicitly validated

### Memory states

- `observed`
- `candidate`
- `established`
- `proven`
- `deprecated`
- `anti_pattern`

### State meaning

- `observed`: evidence exists, but Roger has not yet extracted a reusable claim
- `candidate`: Roger extracted a reusable claim with evidence, but it is still
  tentative
- `established`: evidence is repeated enough for ordinary retrieval
- `proven`: high-trust memory backed by repeated success or canonical authority
- `deprecated`: previously useful or plausible material that should no longer
  be trusted by default
- `anti_pattern`: harmful or misleading guidance that should surface only as a
  warning

## Promotion and Demotion Rules

### Promotion

- `observed -> candidate`
  requires a normalized fact, heuristic, or procedure plus at least one
  evidence link
- `candidate -> established`
  requires one of:
  - two independent `helpful` episodes across separate review runs or sources
  - one `helpful` episode plus explicit human promotion
  - conservative import from a bound canonical source
- `established -> proven`
  requires one of:
  - two independent `approved` episodes
  - one `approved` episode plus one `merged` validation
  - import from an allowlisted canonical policy source marked auto-proven

### Demotion

- `candidate` should demote quickly after a strong contradiction or one clearly
  `harmful` outcome
- `established` should fall at least one level after a `harmful` outcome and
  require fresh supporting evidence before regaining trust
- `proven` should demote to `deprecated` when contradicted by newer canonical
  policy, repeated harmful outcomes, or major anchor invalidation
- `anti_pattern` items remain searchable only as warnings and must not be
  injected as positive review guidance

### Canonical-source rule

- Repo `AGENTS.md`, repo-local Roger policy/config docs, and explicitly bound
  ADR or policy directories may auto-seed high-trust memory.
- Generic `README.md`, `CONTRIBUTING.md`, issue templates, and broad notes must
  stay as searchable evidence unless explicitly promoted later.
- Auto-proven status is reversible when the source changes, is contradicted, or
  leaves the canonical allowlist.

## Duplicate, Alias, and Conflict Handling

Roger must preserve explicit provenance instead of flattening competing memory.

### Duplicate handling

- Deduplicate only within the same scope and memory class.
- Use normalized text, anchor set, source identity, and near-duplicate
  similarity when deciding same-scope duplicates.
- When a new item is a same-scope duplicate, Roger should attach new evidence to
  the existing memory item rather than creating an unbounded duplicate set.

### Cross-scope aliasing

- If two items appear semantically equivalent across scopes, Roger should link
  them with an alias or support edge rather than merge them into one row.
- Cross-scope aliasing must preserve source scope and trust level on both sides.

### Conflict handling

- Roger must model substantive disagreement explicitly through edges such as
  `contradicts` or `supersedes`.
- If two high-trust items conflict, Roger must surface both with provenance
  instead of selecting one silently.
- Contradiction should bias demotion faster than support biases promotion.

## Retrieval and Rebuild Lifecycle

### Retrieval order

For `0.1.0`, Roger should use this query posture:

1. derive anchors from the active review target
2. hard-filter by allowed scopes, trust floor, memory class, and anchor overlap
3. run lexical retrieval as the primary retriever
4. run semantic retrieval only over the curated semantic corpus
5. fuse results with lexical-biased weighting
6. surface promoted memory first, tentative candidates only when policy allows,
   and raw evidence separately
7. abstain when results are weak, out of scope, or verification state is not
   trustworthy enough

### Dirtying and rebuild triggers

- finding created, edited, dismissed, approved, or resolved
- user note or manual rule edit
- session checkpoint or end
- new commit, rebase, or merge-base change
- repo policy or canonical doc change
- scope binding change
- tokenizer, embedding, schema, or sidecar generation change
- sidecar corruption or verification failure

### Rebuild rules

- Foreground writes land in the canonical DB first and mark dirty rows or dirty
  ranges.
- Background workers rebuild lexical/vector generations from canonical state.
- Rebuilds must create a fresh generation and swap it in atomically.
- Roger may serve the last committed generation plus a bounded dirty overlay,
  but must not pretend a stale sidecar is current.
- A failed rebuild must leave the previous committed generation readable when it
  is still valid.
- A corrupt or unverifiable sidecar must be quarantined and excluded from
  retrieval until rebuild succeeds.

## Semantic Asset Policy

Roger `0.1.0` should ship a narrow local semantic slice, but it must remain
best-effort and explicitly verifiable.

### Curated semantic corpus

Allowed for semantic indexing:

- promoted semantic and procedural memory
- accepted findings and session summaries
- repo docs, ADRs, and bound policy excerpts
- compact commit and issue summaries
- bounded path or symbol descriptors

Not allowed for semantic indexing in `0.1.0`:

- raw full code files
- raw prompt or tool transcripts
- arbitrary large artifacts
- generic ambient repo dumps used only because they are available

### Asset install policy

- Roger owns semantic asset installation through an explicit local command or
  install step rather than hidden lazy downloads during review.
- The base `rr` installer should install Roger itself, not silently download
  semantic models as part of ordinary product bootstrap.
- The canonical first semantic install path is a Roger-owned asset command such
  as `rr assets install --asset semantic-default`.
- `rr init` or `rr doctor` may recommend that command when hybrid retrieval is
  desired, but they must not silently trigger model download behind the
  operator's back.
- The configured model identity, asset version, source, expected digests, and
  target install path must be inspectable in local config or status output.
- The default install root should remain Roger-owned, for example under
  `<store.root>/assets/semantic/<asset-id>/<version>/`, rather than in a
  provider-specific or QMD-specific cache directory.
- Asset fetches should resolve through Roger-owned release metadata or an
  explicitly configured mirror/root, not through ad hoc runtime downloader
  logic hidden inside the retrieval engine.
- Asset install must fail closed on digest mismatch, partial download, or
  incompatible metadata.
- Roger must allow semantic search to remain disabled when assets are absent or
  verification fails.
- `rr assets status`, `rr assets verify`, `rr status`, and `rr doctor` should
  all be able to surface the installed/verified/degraded asset state
  truthfully.

### Verification policy

At minimum, semantic asset verification must confirm:

- expected model or asset identifier
- expected content digest or manifest digest
- compatible tokenizer or embedding metadata for the active sidecar generation
- readable local files with complete size or manifest checks

Roger should record verification status in index metadata rather than burying it
in transient logs.

### Degraded-mode rule

- If semantic assets are missing, invalid, or unverified, Roger must say that
  semantic retrieval is unavailable and continue with lexical-only retrieval.
- If lexical sidecars are also unavailable, Roger may enter explicit
  `recovery_scan` mode over canonical DB and bounded file/doc search.
- Roger must never silently present a lexical-only or `recovery_scan` result
  set as if full hybrid retrieval were active.

## Usage and Outcome Vocabulary

Roger needs atomic events plus derived outcomes.

### Atomic `UsageEvent` kinds

- `surfaced`
- `opened`
- `cited`
- `applied_to_finding`
- `applied_to_draft`
- `approved`
- `posted`
- `merged`
- `dismissed`
- `contradicted`
- `marked_harmful`

### Derived outcomes

- `helpful`
- `approved`
- `merged`
- `harmful`

### Outcome rules

- `helpful` means the item materially improved the review loop without yet
  proving long-term correctness
- `approved` means a human approved an outbound draft or explicit durable
  promotion that materially depended on the item
- `merged` means the advice or recommendation traceably tied to the item aligns
  with a merged upstream change
- `harmful` means the item caused a false positive, misleading recommendation,
  wasted investigation, or correctness invalidation

`merged` must require a first-class Roger resolution link to merged outcome
evidence. Roger must not require GitHub posting as the only path to learning
that a recommendation was correct.

## Implementation Guardrails for `0.1.0`

- Keep lexical retrieval primary and semantic retrieval supplemental.
- Keep scope as a hard filter boundary, not a ranking hint.
- Keep candidate memory visibly tentative.
- Keep harmful memory available only as warning material.
- Keep semantic search best-effort and non-gating for the core review loop.
- Keep all search/index state rebuildable from canonical Roger data.
- Keep usefulness vocabulary sufficient for later weighting and contradiction
  handling without building a full analytics product in `0.1.0`.

## Result

For `0.1.0`, Roger now has an implementation-facing policy for search and
memory that is narrow enough for the first slice, explicit enough to constrain
`rr-024`, and honest about degraded semantic behavior.
