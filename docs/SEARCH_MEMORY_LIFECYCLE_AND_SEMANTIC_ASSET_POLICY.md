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
- The configured model identity, asset version, source, expected digests, and
  target install path must be inspectable in local config or status output.
- Asset install must fail closed on digest mismatch, partial download, or
  incompatible metadata.
- Roger must allow semantic search to remain disabled when assets are absent or
  verification fails.

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
- If lexical sidecars are also unavailable, Roger may fall back to canonical-DB
  scan and bounded file/doc search.
- Roger must never silently present a lexical-only or DB-scan result set as if
  full hybrid retrieval were active.

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
