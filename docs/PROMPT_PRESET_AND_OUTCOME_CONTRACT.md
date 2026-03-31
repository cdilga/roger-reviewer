# Prompt Preset And Outcome Contract

## Purpose

This document freezes the `0.1.0` contract for prompt preset reuse, exact
prompt invocation snapshots, and the typed outcome events Roger should persist
for later audit and usefulness analysis.

It narrows the canonical plan without introducing a heavyweight prompt-pack
versioning system or a user-facing analytics product in `0.1.0`.

## Authority And Scope

- `AGENTS.md` remains the operating authority for repo work.
- `docs/PLAN_FOR_ROGER_REVIEWER.md` remains the canonical product plan.
- This document is an implementation-facing support contract for bead
  `rr-004.2`.
- If this document conflicts with the canonical plan, the canonical plan wins
  until the plan is deliberately updated.

## Non-Goals

- full prompt release management
- arbitrary prompt text injection from browser surfaces
- a prompt marketplace or prompt-sharing product
- a user-facing analytics dashboard in `0.1.0`

## Design Rules

- Preset reuse is by stable preset ID.
- Audit and replay are by immutable invocation snapshot, not by whatever the
  preset definition becomes later.
- Roger may evolve preset text over time without introducing formal prompt
  version numbers in `0.1.0`.
- Typed outcome events must be append-only and safe to use for later analytics.
- Reuse projections such as recent, frequent, or last-used prompts are derived
  convenience state, not the canonical history.

## Core Objects

### `PromptPreset`

`PromptPreset` is the reusable named prompt definition selected during intake
or local review operations.

Required fields:

- `id`: stable machine identifier such as `default-pr-review` or
  `security-deep-review`
- `name`: human-readable display label
- `scope`: where the preset is defined and allowed to apply, such as `global`,
  `repo`, `project`, or another Roger-defined scope
- `template_text`: the prompt template before runtime interpolation
- `tags`: zero or more stable tags for filtering and discovery

Optional fields:

- `description`: short operator-facing summary
- `active`: boolean flag for whether the preset is offered in normal selection
- `review_modes`: optional declared compatibility such as `review_only` or
  `clarification_only`

Rules:

- `id` must remain stable once published inside a scope.
- Presets may be superseded or hidden, but older invocation snapshots remain
  valid audit records.
- Roger must reject unknown preset IDs rather than silently substituting a
  nearby preset.
- Preset lookup follows Roger's additive config layering rules; later layers may
  override visible preset definitions, but the resolved invocation must still
  record exactly what text ran.

### `PromptInvocation`

`PromptInvocation` is the immutable runtime record of one resolved prompt use.

Required fields:

- `id`
- `review_session_id`
- `review_run_id`
- `prompt_preset_id`
- `source_surface`: such as `cli`, `tui`, `extension`, or `external-link`
- `resolved_text_digest`
- `used_at`

Required runtime capture:

- exact resolved prompt text, stored inline when small or by artifact reference
  when large
- accepted `explicit_objective` or other bounded user-supplied prompt inputs
- provider and model identity when known
- resolved scope context such as repo, project, or profile selector when
  available

Recommended fields:

- `resolved_text_artifact_id` when the full text is stored out of row
- `resolved_text_inline_preview` for hot-path inspection
- `user_override_text` for bounded objective or local override text
- `config_layer_digest` or equivalent resolved-config fingerprint
- `launch_intake_id` when the invocation came from a persisted intake record

Rules:

- Every run must store the exact resolved prompt text it actually used.
- Replay and audit operate from the invocation snapshot, not from the current
  preset definition.
- Roger does not need formal preset version numbers in `0.1.0` because the
  immutable invocation snapshot is the real execution record.
- If the resolved prompt text is too large for the hot table, the row keeps the
  digest and artifact reference while the cold artifact stores the full text.
- Browser and deep-link surfaces may contribute only the bounded prompt-ingress
  fields already allowed by the canonical plan.

## Preset Reuse Projections

Roger should provide lightweight prompt reuse without introducing a separate
prompt-versioning product.

### `recent_prompts`

Purpose:

- show the most recently used preset selections for the current operator

Rules:

- derived from `PromptInvocation` history
- scoped at least by Roger profile and repo
- ordered by most recent `used_at`
- stores or renders the preset ID plus a short label, not a second mutable
  prompt definition
- may include the last accepted short explicit objective only if Roger can show
  it clearly as invocation history rather than as canonical preset text

### `frequent_prompts`

Purpose:

- surface prompt presets that are repeatedly chosen in the same working scope

Rules:

- derived from invocation counts, not manually curated
- default aggregation scope is repo within the active Roger profile
- Roger may expose wider rollups later, but `0.1.0` should not silently mix
  unrelated repos or organizations
- ties may fall back to most recent use

### `last_used_prompt`

Purpose:

- remember the default preset Roger should preselect for the current repo when
  no explicit preset was provided

Rules:

- persisted per repo within the active Roger profile
- stores preset ID only
- may be ignored when the stored preset is no longer valid in the resolved
  config scope
- must never invent an `explicit_objective`

### `favorite_prompts`

Purpose:

- optional operator-pinned shortcuts for commonly preferred presets

Rules:

- optional in `0.1.0`; the storage contract should allow it, but Roger does not
  need to treat favorites as a launch blocker
- persisted per Roger profile by default
- favorites point to preset IDs and optional display ordering only
- favorites do not override scope or permission checks

## Typed Outcome Event Model

Roger should keep prompt-use and review-outcome history in a typed event stream
that supports later analysis without forcing analytics to become a first-class
runtime subsystem.

### Common Envelope

Every outcome event should include:

- `id`
- `event_type`
- `occurred_at`
- `review_session_id`
- `review_run_id` when applicable
- `prompt_invocation_id` when the event is attributable to a prompt pass
- `actor_kind`: such as `agent`, `human`, or `system`
- `actor_id` when known
- `source_surface`
- `payload`

Rules:

- event types are append-only
- later schema growth should add fields or new event types rather than changing
  the meaning of existing ones
- materialized summaries may be rebuilt from the event stream

### Required `0.1.0` Event Types

#### `finding_emitted`

Use when a prompt pass or repair path creates a normalized finding candidate.

Required payload:

- `finding_id`
- `finding_fingerprint`
- `severity`
- `confidence`
- `stage`

#### `finding_state_changed`

Use when triage or outbound state changes on an existing finding.

Required payload:

- `finding_id`
- `from_triage_state`
- `to_triage_state`
- `from_outbound_state`
- `to_outbound_state`
- `reason_code` when present

#### `draft_materialized`

Use when Roger creates a local outbound draft from one or more findings.

Required payload:

- `draft_id`
- `draft_batch_id`
- `finding_ids`
- `payload_digest`

#### `approval_state_changed`

Use when a draft or batch moves through approval, rejection, or invalidation.

Required payload:

- `draft_batch_id`
- `from_approval_state`
- `to_approval_state`
- `approval_token_id` when present
- `reason_code` when present

#### `posted_action_recorded`

Use when Roger records a post attempt or completed post against GitHub.

Required payload:

- `draft_batch_id`
- `posted_action_id`
- `remote_provider`
- `remote_identifier` when known
- `status`
- `failure_code` when present

#### `usefulness_labeled`

Use when a human explicitly labels the usefulness or noisiness of a prompt path,
finding, or review outcome.

Required payload:

- `target_kind`: `prompt_invocation`, `finding`, `draft_batch`, or
  `review_session`
- `target_id`
- `label`: Roger-defined values such as `useful`, `mixed`, or `low_value`

Optional payload:

- `note_artifact_id`

### Optional `0.1.0` Event Types

Roger may also store the following if they are cheap and clean to capture:

- `prompt_invoked`
- `review_completed`
- `review_merge_outcome_recorded`
- `posted_action_reconciled`

These are optional because `rr-004.2` only needs the minimum event set required
to support future usefulness analysis and review audit.

## Analytics Boundary

`0.1.0` should capture typed evidence for later analysis, but it should not
turn outcome reporting into a product surface of its own.

That means:

- the event stream exists for audit, rebuild, and later heuristics
- Roger may expose narrow operator-visible summaries such as recent prompt use
  or explicit usefulness labels
- Roger should not require dashboards, cohort tooling, or prompt-scoring UIs to
  satisfy this contract

## Implementation Notes For Later Beads

- Storage schema should treat `PromptInvocation` as canonical history and reuse
  projections as rebuildable state.
- Prompt selection UI should read preset IDs plus reuse projections rather than
  duplicating prompt text catalogs.
- Future prompt-pack versioning can be added later by extending
  `PromptPreset`, but it should not invalidate stored `PromptInvocation`
  snapshots from `0.1.0`.
