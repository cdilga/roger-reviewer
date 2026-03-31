# Core Domain Schema and Finding Fingerprint

This document closes the remaining planning gap for `rr-013`. It does not
replace the canonical plan or the broader
[`docs/DATA_MODEL_AND_STORAGE_CONTRACT.md`](./DATA_MODEL_AND_STORAGE_CONTRACT.md)
contract. It freezes the missing domain invariants that later storage,
refresh, harness-linkage, and outbound-flow beads need before implementation.

Authority:

- `AGENTS.md` remains the operational contract.
- [`docs/PLAN_FOR_ROGER_REVIEWER.md`](./PLAN_FOR_ROGER_REVIEWER.md) remains the
  canonical product and architecture plan.
- [`docs/DATA_MODEL_AND_STORAGE_CONTRACT.md`](./DATA_MODEL_AND_STORAGE_CONTRACT.md)
  remains the broader storage contract.
- This document narrows the unresolved schema and lifecycle rules for `rr-013`.

## Scope of This Contract

This document freezes:

- the minimum first-class entity set Roger must treat as canonical
- the state splits that must remain distinct
- the append-only event history boundary
- finding fingerprint inputs and refresh classification rules
- approval invalidation inputs
- scope-boundary and write-ownership rules that prevent silent bleed or unsafe
  reuse

This document does not freeze the final SQL schema or storage engine details.

## Canonical Entities

### Review continuity

- `ReviewSession`
- `ReviewRun`
- `ReviewTarget`
- `ReviewRunState`
- `SessionLocator`
- `ResumeBundle`

### Prompt and command capture

- `PromptInvocation`
- `RogerCommandInvocation`
- `RogerCommandResult`

### Findings and evidence

- `Finding`
- `FindingFingerprint`
- `FindingStateSnapshot`
- `FindingDecisionEvent`
- `EvidenceLink`
- `CodeEvidenceLocation`
- `ClarificationThread`

### Outbound review communication

- `OutboundDraft`
- `OutboundDraftBatch`
- `OutboundApprovalToken`
- `PostedAction`
- `PostedActionItem`

### Scope and configuration

- `Scope`
- `Source`
- `Episode`
- `ConfigLayer`

## Aggregate Roles

### `ReviewSession`

The durable top-level container for one Roger review target plus the Roger-owned
continuity state needed to resume, refresh, triage, and audit work locally.

Required invariants:

- binds to exactly one `ReviewTarget`
- owns the stable Roger session id
- may contain multiple `ReviewRun` records over time
- is the root for approval invalidation, attention state, and continuity health

### `ReviewRun`

One concrete review pass within a session.

Required invariants:

- belongs to exactly one `ReviewSession`
- captures the repo snapshot or remote target snapshot it reviewed
- never becomes the sole source of truth for session state; later runs extend
  the session rather than replacing it blindly

### `SessionLocator`

Harness-specific reopen data.

Required invariants:

- may be stale without making the session invalid
- is provider-specific and reopen-oriented
- is never treated as the only continuity mechanism

### `ResumeBundle`

Harness-neutral Roger continuity packet.

Required invariants:

- contains only the bounded state needed to continue the review truthfully
- includes review target, active continuity summary, unresolved findings,
  follow-up state, and artifact references
- can outlive or replace a dead `SessionLocator` path without pretending to be
  a full transcript replay

### `PromptInvocation`

Append-only record of the exact prompt Roger resolved and sent.

Required invariants:

- stores the exact resolved prompt text or a content-addressed reference to it
- captures stage, model, provider, and originating surface
- is immutable once written

### `Finding`

Structured review result with evidence and lineage.

Required invariants:

- belongs to exactly one `ReviewSession`
- is first created in exactly one `ReviewRun`
- carries one canonical `FindingFingerprint`
- may survive across later runs through refresh reconciliation
- may have zero or more `EvidenceLink` rows and zero or more
  `CodeEvidenceLocation` rows

### `FindingFingerprint`

Deterministic or near-deterministic identity used to reconcile findings across
runs.

Required invariants:

- remains stable across non-material wording drift
- changes when Roger no longer believes two findings refer to the same issue
- is derived from normalized issue identity inputs, not from opaque row ids

### `CodeEvidenceLocation`

Normalized code anchor attached to a finding.

Required invariants:

- stores repo-relative path plus normalized range data when available
- carries an explicit evidence role such as `primary`, `supporting`, or
  `contradicting`
- has its own anchor-validity state separate from the parent finding's triage
  or outbound state

### `OutboundDraft`

Local proposal for one outbound GitHub action or one item in a grouped action.

Required invariants:

- binds to one immutable remote target tuple
- carries a payload digest for approval safety
- never silently retargets to a different remote object

### `OutboundDraftBatch`

Approval and posting unit for one review target.

Required invariants:

- belongs to exactly one `ReviewSession`
- groups one or more `OutboundDraft` items for the same remote target
- owns approval invalidation for its payload set

### `PostedAction`

Immutable audit record after Roger posts or partially posts a draft batch.

Required invariants:

- records the remote identifiers and posted payload digest
- never rewrites the original approval decision or draft payload history

### `Scope`

Boundary object for repo, project, and future org overlays.

Required invariants:

- every promotable or searchable durable object has an explicit scope identity
- cross-scope references may exist, but silent cross-scope merge is forbidden

### `ConfigLayer`

One additive configuration layer.

Required invariants:

- later layers may override only through explicit precedence
- the resolved effective config must stay inspectable
- ambient hidden replacement is not allowed

## State Machines That Must Stay Separate

### Finding triage state

Allowed values:

- `new`
- `accepted`
- `ignored`
- `needs_follow_up`
- `resolved`
- `stale`

Rules:

- triage is a reviewer judgment about the finding itself
- triage transitions are append-only events plus a current materialized state
- `resolved` and `stale` are not synonyms: `resolved` means later evidence shows
  the issue is addressed; `stale` means the finding no longer maps cleanly after
  refresh and needs explicit review or retirement

### Finding outbound state

Allowed values:

- `not_drafted`
- `drafted`
- `approved`
- `posted`
- `failed`

Rules:

- outbound state is about communication, not issue truth
- outbound state must not be inferred from triage state alone
- a finding may be `accepted` yet remain `not_drafted`
- a finding may remain `resolved` while an old outbound attempt is still
  `failed` for audit reasons

### Code-anchor validity state

Allowed values:

- `valid`
- `stale`
- `relocated`
- `missing`
- `contradictory`

Rules:

- anchor validity belongs to `CodeEvidenceLocation`, not the entire finding
- invalid or stale anchors must not destroy the rest of the finding's evidence
- `relocated` means Roger can still map the same evidence confidently after
  refresh
- `contradictory` means competing repair or refresh signals disagree and Roger
  must preserve both history and the unresolved conflict

### Continuity state

Allowed values:

- `attached`
- `reopened`
- `reseeded`
- `dropped_out`
- `degraded`
- `failed`

Rules:

- continuity state belongs to the session or active run, not to individual
  findings
- `reseeded` means Roger continued from `ResumeBundle`
- `degraded` means Roger can continue truthfully only with reduced capabilities

## Append-Only Event History

Roger must store both current materialized state and append-only event history.

Append-only event categories:

- prompt invocations
- command invocations and results
- finding creation and refresh carry-forward decisions
- triage changes
- outbound draft creation, approval, invalidation, posting, and failure
- continuity changes such as reopen, reseed, dropout, and recovery

Rules:

- event rows are never rewritten to hide prior states
- current state is a projection over the event history plus bounded current
  snapshot tables
- analytics, repair, and audit must be answerable from event history without
  transcript scraping

## Finding Fingerprint Contract

`FindingFingerprint` must be derived from normalized issue identity inputs rather
than presentation-only text.

Minimum input classes:

- canonical review target identity
- normalized issue class or issue-code family
- normalized primary code evidence when available
- normalized summary or claim text with volatile wording stripped
- optional supporting evidence discriminators when needed to avoid collisions

Rules:

- raw line numbers alone are insufficient; the fingerprint must survive small
  diff drift when Roger still means the same issue
- repo target identity is required so similar findings from different scopes do
  not collide
- if two findings cannot be distinguished safely, Roger must prefer different
  fingerprints over unsafe merge
- fingerprint derivation must be deterministic for the same normalized inputs

## Refresh Classification

Refresh must classify prior findings rather than duplicating them blindly.

Allowed classification outcomes:

- `carried_forward`
- `superseded`
- `resolved`
- `stale`
- `new`

Rules:

- `carried_forward` means the finding remains materially the same across runs
- `superseded` means the old finding is replaced by a newer finding with better
  evidence, tighter phrasing, or narrowed scope
- `resolved` means later code or evidence indicates the issue is no longer
  present
- `stale` means Roger cannot currently map the old finding with enough
  confidence to resolve or carry it forward safely
- every refresh classification must preserve lineage from prior finding snapshot
  to later finding snapshot or terminal state

## Approval Invalidation Inputs

Previously approved outbound drafts must be invalidated when any of the
following changes materially alter what Roger intends to send:

- remote review target tuple changes
- draft payload digest changes
- grouped batch membership changes
- primary evidence anchor changes in a way that affects the claim or suggested
  comment location
- finding refresh classification becomes `superseded`, `resolved`, or `stale`
- repo or PR head changes after approval and before posting when revalidation no
  longer proves the payload still matches the current state

Rules:

- invalidation is explicit and recorded as an append-only event
- invalidation may revoke posting authority, but may not silently rewrite prior
  approval history
- approval tokens bind to payload digest plus target tuple, not to mutable row
  ids alone

## Scope Boundaries

Roger must fail closed on scope bleed.

Rules:

- `ReviewSession`, `Finding`, `OutboundDraftBatch`, and `PostedAction` are
  review-target-scoped objects, not global free-floating records
- `Scope` must be explicit on searchable or promotable durable objects
- cross-scope references require explicit provenance; Roger must not silently
  collapse repo and project objects into one canonical row
- review findings from one repo or PR may inform later search, but they cannot
  be reused as if they were produced in a different target context

## Write Ownership

Write ownership must stay simple before multiple surfaces are implemented.

Rules:

- the Roger app core owns canonical writes for session, finding, outbound, and
  continuity state
- CLI, TUI, extension, and future editor surfaces are adapters over the same
  core rules
- derived search indices, editor integrations, and extension-local caches are
  never allowed to become the source of truth for mutable review state

## Acceptance Mapping for `rr-013`

- Core entities and invariants: frozen in the canonical-entities and aggregate
  sections.
- Triage, outbound, and anchor-validity states plus append-only event history:
  frozen in the state-machine and event-history sections.
- Finding fingerprint, refresh classification, approval invalidation, and scope
  boundaries: frozen in dedicated contract sections.
