# ADR 009: Prompt Preset, Invocation Snapshot, and Outcome-Event Model

- Status: accepted
- Date: 2026-03-29

## Context

The canonical plan defines a `PromptPreset` and `PromptInvocation` minimum model
and specifies that Roger should collect outcome data sufficient for later
analytics. The data model contract already captures `PromptInvocation` relational
fields, but neither document freezes:

- the `PromptPreset` relational shape
- the persistence rules for recent, frequent, last-used, and favorite prompt reuse
- the typed outcome-event model Roger must write so later analytics can answer
  which prompts, findings, and review paths were actually useful

Without an explicit contract here, implementation will either invent these shapes
ad hoc or skip the analytics-ready capture entirely. Either outcome makes it
harder to build usefulness scoring later without a costly migration.

The goal is not a full analytics product in `0.1.0`. The goal is to make sure
the foundation does not need to be redesigned when analytics becomes a priority.

## Decision

### `PromptPreset` shape

Roger should treat `PromptPreset` as a shared, user-editable registry of
reusable prompt templates scoped to `global`, `project`, or `repo`.

Minimum relational fields:

- `id` — stable string identifier, used for reuse tracking and invocation linkage
- `name` — human-readable display name
- `scope` — `global`, `project`, or `repo`; `repo` is the default for
  user-created presets
- `scope_key` nullable — normalized repo id or project id when scope is `repo`
  or `project`
- `template_text` — the raw prompt template text; may include bounded
  placeholder tokens for objective, PR context, and config-driven insertions
- `tags` — optional free-form labels for categorization and filter UI
- `is_builtin` — boolean; builtin presets ship with Roger and are not
  user-editable directly, though they may be copied to a `repo`-scoped preset
- `is_favorite` — boolean; user-marked shortcut
- `created_at`
- `updated_at`
- `row_version`

Presets should use stable string ids rather than integer primary keys so that
references survive export, transfer, and schema evolution.

### Reuse rules

Roger must track three reuse signals per preset, stored as separate rows in a
`PromptReuse` or `PromptPresetUsage` table rather than as mutable columns on
`PromptPreset`. This prevents concurrent writes from corrupting aggregate counts.

Required reuse signals:

- **recent**: the last N invocation timestamps per preset per `scope_key`
  (default N=10; user-configurable up to 50); used to surface `recently used`
  in the preset picker
- **frequency**: a rolling invocation count per preset per `scope_key` within
  a configurable window (default 90 days); used to surface `frequently used`
- **last-used per repo**: the single most recent `used_at` timestamp for a
  preset in a given repo; used to restore the `last used` preset across sessions

Favorites are user-set boolean flags on the preset row itself (`is_favorite`).
Favorites take priority in the preset picker order before recent or frequent.

Scope and persistence expectations:

- reuse signals are repo-local by default when the preset scope is `repo` or
  the session has a bound repo; otherwise they accumulate globally
- reuse counts and recency data should be kept for at least 90 days; older rows
  are eligible for pruning and do not need permanent retention
- reuse signals are informational; their loss or staleness should never block a
  review

### `PromptInvocation` shape

The data model contract already captures this shape. This ADR confirms the
required fields and adds two:

- `id`
- `review_session_id`
- `review_run_id`
- `prompt_preset_id` nullable — null when the user supplies a fully ad hoc
  prompt without selecting a preset
- `resolved_text_digest` — content-addressed hash of the exact resolved prompt
  text used at runtime
- `resolved_text_artifact_id` nullable — reference to cold artifact when the
  resolved prompt text exceeds inline storage threshold (default 4KB)
- `user_override_text` nullable — the short explicit objective or other
  user-supplied override injected at invocation time
- `source_surface` — `cli`, `tui`, `extension`, or `direct` so auditors can
  see where the invocation originated
- `provider` — the harness provider used for this invocation
- `model_id` — model identifier used, as reported by the harness
- `stage` — `exploration`, `deep_review`, or `follow_up`; allows later grouping
  by pipeline position without joining back through session/run tables
- `used_at`

Rules:

- `PromptInvocation` is append-only; do not update rows after creation
- the resolved text snapshot is always written, even when it matches a prior run;
  reuse tracking uses preset id and reuse signals, not deduplication of
  invocation rows
- when `resolved_text_artifact_id` is set the inline digest still must be stored
  so integrity checks do not require reading the cold artifact

### Typed outcome events

Roger must emit typed outcome events for findings, drafts, approvals, postings,
and usefulness signals. These events are append-only rows in an `OutcomeEvent`
table. They are not a replacement for the `FindingDecisionEvent` and
`OutboundDraft` state rows; they are a structured analytics-ready layer on top.

Required event kinds:

| Kind | When emitted | Required fields |
|------|-------------|-----------------|
| `finding_created` | new finding normalized from a run | `finding_id`, `prompt_invocation_id`, `review_session_id`, `review_run_id`, `fingerprint` |
| `finding_triage_changed` | triage state transition | `finding_id`, `from_state`, `to_state`, `actor` |
| `finding_draft_created` | outbound draft materialized from finding | `finding_id`, `draft_id`, `review_session_id` |
| `draft_approved` | user grants approval | `draft_batch_id`, `approval_token_id`, `actor` |
| `draft_invalidated` | approval revoked before post | `draft_batch_id`, `invalidation_reason_code` |
| `draft_posted` | successful post to GitHub | `draft_batch_id`, `posted_action_id`, `remote_identifier` |
| `draft_post_failed` | post failed | `draft_batch_id`, `failure_code` |
| `usefulness_labeled` | user provides explicit signal | `finding_id` nullable, `draft_id` nullable, `label` (`useful`, `not_useful`, `harmful`), `actor` |
| `pr_merged` | PR merged signal received (best-effort) | `review_session_id`, `remote_pr_id` |
| `pr_closed_unmerged` | PR closed without merge (best-effort) | `review_session_id`, `remote_pr_id` |

Shared fields on every outcome event:

- `id`
- `kind`
- `created_at`
- `review_session_id`
- any entity ids listed in the Required fields column above

Rules:

- outcome events are append-only and must never be updated or deleted
- every event kind is typed; Roger must not store untyped string-blob event rows
- `pr_merged` and `pr_closed_unmerged` are best-effort signals; their absence
  must not break any core workflow
- `usefulness_labeled` may be attached to a finding, a draft, or a session; at
  least one of those ids must be non-null
- the outcome event table must carry enough denormalized fields that a basic
  analytics query does not require joining more than two additional tables

### Analytics capture boundary

Roger does not need a user-facing analytics dashboard in `0.1.0`. But it must
preserve enough evidence for these queries without a schema redesign:

- which presets and resolved prompt texts were used in a session or run
- which findings per session/run survived to `accepted` vs `ignored` vs `stale`
- which accepted findings produced drafts
- which drafts were approved and posted vs invalidated or failed
- what usefulness labels exist for a finding or draft

These queries must be answerable from `PromptInvocation` + `OutcomeEvent` rows
plus the existing `Finding` and `OutboundDraft` state rows. No additional
tables are required in `0.1.0`.

## Consequences

- Roger gains an explicit, frozen contract for prompt reuse so the preset picker
  can be implemented without inventing the rules ad hoc
- Invocation snapshots guarantee exact repro and audit without requiring a
  prompt-pack versioning system
- Typed outcome events make the analytics foundation non-optional while keeping
  it out of the hot UI path
- The separate `PromptPresetUsage` signal table avoids write contention on
  mutable aggregate counts during concurrent reviews

## Open Questions

- What is the exact inline threshold (suggested 4KB) for offloading resolved
  prompt text to cold artifacts? Should this be user-configurable or hardcoded?
- Should `stage` on `PromptInvocation` be a structured enum in the Rust domain
  or stored as a string with a validation check?
- Should `PromptPresetUsage` accumulate globally or be strictly per-profile
  when Roger later supports multiple profiles?

## Follow-up

- Add `PromptPreset` and `PromptPresetUsage` minimal relational fields to
  `DATA_MODEL_AND_STORAGE_CONTRACT.md`
- Implement `PromptPreset` registry and reuse signals as part of rr-016
- Implement `OutcomeEvent` table and emission as part of rr-016 and rr-017
- Add smoke tests for reuse-signal accumulation and outcome-event completeness
