# ADR 004: Scope and Memory Promotion Policy

- Status: accepted
- Date: 2026-03-29

## Context

Roger now has the right memory/search direction in the canonical plan, but
implementation still needs a tighter policy for:

- repo/project/org boundaries
- what is searchable immediately versus promotable
- how promotion, demotion, and anti-pattern capture work
- what outcome labels actually mean

Without this ADR, implementation will either over-promote noisy material or let
cross-scope memory bleed silently.

## Decision

Recommended policy:

- `repo` is the default search and memory scope
- `project` and `org` are explicit overlays only
- no automatic repo-to-project-to-org fallback when repo results are weak
- searchable evidence and promoted reusable memory are different layers

Recommended boundary rule:

- `project` means an explicit Roger-managed allowlist of repos that belong to
  the same review context or service family
- Roger should not infer `project` membership from remote origin similarity,
  directory layout, naming conventions, or weak semantic similarity
- repo-local review should remain the default even when no project overlay is
  configured

Recommended memory states:

- `observed`
- `candidate`
- `established`
- `proven`
- `deprecated`
- `anti_pattern`

Recommended promotion rules:

- raw findings, summaries, notes, and commit/issue summaries are searchable
  evidence first
- extracted facts and procedures become `candidate` only with evidence links
- broader scopes require explicit binding or enablement
- harmful or contradicted lessons should demote faster than helpful lessons
  promote

Required flow split:

- `Source` and `Episode` are evidence/history objects and are searchable by
  default
- `MemoryItem` is reusable memory and must be promoted explicitly through the
  Roger policy below
- candidate memory must not silently behave like promoted memory

Required retrieval lanes:

- `promoted_memory`: `established` and `proven` items eligible for normal
  retrieval and prompt injection
- `tentative_candidates`: `candidate` items shown only when anchor overlap is
  high or the user explicitly asks for tentative memory
- `evidence_hits`: raw findings, docs, notes, and episodic artifacts returned as
  searchable evidence with provenance

This keeps Roger from laundering weak evidence into strong guidance just because
it was embedded or retrieved.

### UsageEvent model

`helpful`, `approved`, `merged`, and `harmful` should be derived outcome labels,
not the only raw event records.

Recommended atomic `UsageEvent` kinds:

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

Recommended usage outcome vocabulary:

- `helpful`: Roger surfaced the item and it materially improved the review loop
  without yet proving long-term correctness; examples include faster triage,
  better clarification, or a finding the reviewer kept rather than discarded
- `approved`: a human explicitly approved an outbound draft, recommendation, or
  durable promotion that materially depended on the item
- `merged`: the advice or draft backed by the item corresponds to a change that
  later landed upstream, making it a strong ex-post correctness signal
- `harmful`: the item caused a false positive, misleading recommendation,
  wasted investigation, or otherwise pushed the review in the wrong direction

Recommended derivation rules:

- `helpful` is earned when a surfaced item is kept in the working set, cited in
  reasoning, survives clarification, or materially contributes to a finding that
  the reviewer does not discard
- `approved` is earned when a human approves an outbound draft or explicit
  durable promotion that materially depended on the item
- `merged` is earned when the approved/posted advice, or a local-only Roger
  recommendation traceably tied to the item, aligns with a merged upstream
  change
- `harmful` is earned when the item is dismissed as wrong/outdated/noisy,
  invalidates a draft for correctness reasons, is contradicted by newer
  evidence, or is explicitly marked as misleading

Required merged-validation rule:

- `merged` should require a first-class Roger resolution link to merged outcome
  evidence
- that link may originate from a posted draft, or from a local-only Roger
  recommendation/finding that can be traceably associated with the merged change
- Roger should not require GitHub posting as the only path to learning that a
  recommendation was right

What these power:

- retrieval weighting and trust display
- promotion from `candidate` to `established` to `proven`
- demotion to `deprecated` or `anti_pattern`
- conflict surfacing and anti-pattern warnings
- evaluation of whether Roger is learning useful review behavior or just
  accumulating noise

### Promotion thresholds

Recommended state transitions:

- `observed -> candidate`
  requires a normalized claim or procedure plus at least one evidence link
- `candidate -> established`
  requires any one of:
  - two independent `helpful` episodes across separate review runs or sources
  - one `helpful` episode plus explicit human promotion
  - conservative import from a bound canonical source that is not auto-proven
- `established -> proven`
  requires any one of:
  - two independent `approved` episodes
  - one `approved` episode plus one `merged` validation
  - import from an allowlisted canonical policy source marked auto-proven

Recommended demotion rules:

- a `candidate` with a strong contradiction or one clearly `harmful` outcome
  should demote quickly to `deprecated` or `anti_pattern`
- an `established` item with one `harmful` outcome should fall at least one
  level and require fresh supporting evidence before regaining trust
- a `proven` item contradicted by updated canonical policy, changed anchors, or
  repeated harmful outcomes should demote to `deprecated` pending revalidation
- `anti_pattern` items should remain searchable only as warnings and should not
  auto-inject into review prompts as positive guidance

Recommended canonical-doc rule:

- a checked-in document may auto-seed `proven` memory only when it is inside the
  active repo or an explicitly bound project overlay and is marked or configured
  as canonical policy/process guidance
- examples when explicitly bound: ADRs, repo review policy, security review
  guide, and architecture rules
- canonical-doc auto-proven means Roger does not wait for repeated episodic use
  before trusting the extracted rule at a high level
- auto-proven is still reversible if the source changes, is contradicted, or is
  later removed from the canonical allowlist

Recommended default canonical classes:

- auto-proven by default only for:
  - repo `AGENTS.md`
  - repo-local Roger policy/config docs
  - explicitly bound ADR directories or policy directories in Roger config
- not auto-proven by default:
  - generic `CONTRIBUTING.md`
  - generic `README.md`
  - broad architecture notes
  - generic issue templates or PR templates

Those can still be searchable evidence or promoted later, but they should not
silently become high-trust memory unless the repo owner binds them as canonical.

## Consequences

- scope becomes an implementation filter boundary, not only a ranking hint
- review-safe abstention remains possible when no good memory should surface
- project/org overlays can exist without turning into ambient company memory
- retrieval can surface tentative and promoted material differently instead of
  flattening everything into one confidence bucket

## Open Questions

- what exact storage shape should represent merged-resolution links and
  `UsageEvent` derivation jobs?

## Follow-up

- define the exact `UsageEvent` storage shape and derivation jobs
- add a test matrix for scope bleed, stale-memory suppression, and conflict
  surfacing
