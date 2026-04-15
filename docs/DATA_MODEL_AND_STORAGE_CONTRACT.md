# Data Model and Storage Contract

This document narrows the canonical plan into an implementation-facing storage
contract for Roger `0.1.0`.

It exists to answer one practical question:

- what must Roger store relationally so review continuity, approval safety,
  resume, and later analytics all work without bloating the hot path

The canonical product plan remains
[`PLAN_FOR_ROGER_REVIEWER.md`](docs/PLAN_FOR_ROGER_REVIEWER.md).

## Posture

Roger is **analytics-second** and **operations-first**.

That means:

- store what is needed for durable review state, audit, safety, and resume first
- keep the hot relational model small and explicit
- preserve enough typed event history that later analytics can be derived
- keep large raw payloads and derived search assets out of hot tables
- assume one canonical Roger store per user profile by default; named
  instances and worktrees isolate repo-local mutable resources unless the user
  explicitly creates a separate profile

## Storage Layers

Roger should use three storage layers with different jobs.

### 1. Canonical relational store

Use the local SQLite-family store for:

- sessions, runs, findings, prompts, drafts, approvals, posted actions
- scope, provenance, usage events, and outcome signals
- config, launch profiles, worktree/instance state
- artifact metadata and content digests
- index metadata and generation state

This is the source of truth.

### 2. Cold artifact store

Use a sibling content-addressed artifact directory for:

- raw model output
- long prompt text and prompt logs
- transcripts and tool traces
- large diff/context payloads
- large excerpts kept for audit or selective reinjection

The relational DB stores artifact ids, hashes, sizes, mime/kind, and
provenance. It does not inline large opaque blobs by default.

### 3. Rebuildable search sidecars

Use lexical/vector sidecars for:

- Tantivy indices
- embedding/vector state
- tokenization/index generations

These are derived from the canonical DB plus cold artifacts and can be rebuilt.

## Concurrency Model

Roger must support concurrent review activity without corrupting one session.

Recommended rules:

- one logical writer per `ReviewSession` at a time
- multiple readers are always allowed
- background indexing workers may write only derived/index state, never mutate
  core review aggregates directly
- same-session writes should serialize through a Roger-owned session lease or
  optimistic version check
- cross-session writes may proceed concurrently

Recommended implementation shape:

- every mutable aggregate row carries `row_version`
- every important state transition also writes an append-only event row
- stale writers must fail with a truthful conflict result rather than silently
  winning

This prevents "last write wins" bugs when multiple agents or local surfaces
touch the same review session.

### Process boundary posture

Recommended `0.1.x` process model:

- a TUI process should host the Roger command router, domain access, and view
  model in-process rather than remoting every UI action across a mandatory local
  IPC boundary
- one primary `rr` binary should own TUI, CLI, bridge-host, and helper modes by
  default rather than assuming multiple cooperating local executables
- CLI invocations, browser-bridge host invocations, agent-owned commands, and
  other local entrypoints may run as separate Roger processes against the same
  canonical store
- same-process long-running work should move off the active TUI foreground loop
  onto a dedicated async executor thread for I/O-bound work plus bounded
  CPU-worker execution for indexing/search maintenance, with Roger-owned
  channels returning bounded results back to the UI loop
- cross-process coordination should rely on canonical-store state, append-only
  event rows, row-version checks, local wake signals, and a bounded
  refresh-by-event-cursor mechanism rather than a resident broker process

This keeps the hot path simple while still supporting multiple entrypoints and
concurrent review activity across PRs or agents.

## Canonical Aggregates

### Review continuity

- `ReviewSession`
- `ReviewRun`
- `ReviewRunState`
- `ReviewTarget`
- `SessionLocator`
- `ResumeBundle`
- `SessionBaselineSnapshot`
- `HarnessCapabilitySet`

### Worker task and execution activity

- `ReviewTask`
- `WorkerInvocation`
- `WorkerToolCallEvent`
- `WorkerStageResult`

### Prompt and command activity

- `PromptPreset`
- `PromptPresetUsage`
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

### Outcome events

- `OutcomeEvent`

### Scope, memory, and usage

- `Scope`
- `Source`
- `Episode`
- `MemoryItem`
- `MemoryEdge`
- `MemoryReviewRequest`
- `UsageEvent`

### Runtime and isolation

- `ConfigLayer`
- `LocalLaunchProfile`
- `NamedInstance`
- `WorktreeInstance`

### Search/index bookkeeping

- `IndexJob`
- `IndexState`

## Event History Versus Current State

Roger should store both:

- current materialized state for fast UI and CLI queries
- append-only events for audit, repair, and later analytics

Recommended event-first areas:

- finding triage/outbound changes
- prompt invocations
- approval/rejection decisions
- posted-action outcomes
- merge/usefulness/harmful outcome signals
- instance/worktree retargeting that invalidates approvals

Recommended materialized-state areas:

- current finding state
- current draft state
- current session/run state
- current index generation state

## Required Invariants

### Prompt invariants

- each `PromptInvocation` belongs to one canonical task/invocation attempt even
  when the task program contains multiple turns
- each `PromptInvocation` stores the exact resolved prompt text used at runtime
- preset reuse is by stable preset id, but audit/repro uses the invocation
  snapshot
- large prompt text may move to cold artifacts while the invocation row keeps
  digest, metadata, and bounded inline summary
- `source_surface` is a Roger-owned normalized enum:
  `cli`, `tui`, `extension`, `external_link`, `harness_command`, `agent`, or
  `system`; legacy spellings such as `direct`, `bridge`, or `external-link`
  normalize into this enum at the boundary

### Worker execution invariants

- each `ReviewTask` belongs to exactly one `ReviewSession` and one `ReviewRun`
- each `ReviewTask` stores the allowed scopes, allowed operations, and
  turn-strategy Roger assigned to that unit of work
- each `WorkerInvocation` belongs to exactly one `ReviewTask`
- each `WorkerInvocation` may own zero or more `PromptInvocation` turns
- each successful or terminal task attempt stores exactly one
  `WorkerStageResult`
- nonce mismatch, stale task binding, or cross-session result submission must
  fail closed

### Finding invariants

- each finding has a stable fingerprint or near-stable normalized identity
- triage state and outbound state are distinct
- each finding's evidence should distinguish generic artifact links from
  normalized repo code locations when code-backed evidence exists
- code-evidence locations should preserve repo-relative path plus normalized
  range data so the same anchors can support TUI inspection, refresh
  reconciliation, and local editor handoff
- invalid anchors or contradictory repairs never silently overwrite prior valid
  evidence
- editor-open actions should be derived from stored finding/code-location state
  plus local editor configuration, not stored as a second source of truth

### Outbound invariants

- each `OutboundDraft` belongs to one immutable review target tuple
- each `OutboundDraftBatch` belongs to exactly one `ReviewSession` and one
  remote review target
- each `OutboundApprovalToken` binds to the exact payload hash plus target tuple
- post-time revalidation may revoke approval, but may not silently retarget a
  draft

### Scope invariants

- every searchable/promotable item has explicit scope identity
- cross-scope aliasing is allowed; silent cross-scope merge is not

### Baseline and memory-review invariants

- each `SessionBaselineSnapshot` belongs to exactly one `ReviewSession`
- a baseline snapshot records Roger's stable session posture for allowed
  scopes, default recall mode, candidate visibility, and policy epochs before
  task-level overrides are applied
- `ReviewTask` and `PromptInvocation` may narrow or specialize a baseline, but
  they do not overwrite baseline history
- each `MemoryReviewRequest` belongs to one canonical memory subject and one
  Roger session lineage
- `MemoryReviewRequest` transitions are auditable and non-mutating until an
  accepted resolution is applied through Roger-owned memory-state logic

## Suggested Minimal Relational Fields

Do not treat this as the final SQL schema. Treat it as the minimum contract.

### `PromptPreset`

- `id` — stable string identifier
- `name`
- `scope` — `global`, `project`, or `repo`
- `scope_key` nullable — repo id or project id when scope is `repo` or `project`
- `template_text`
- `tags` — optional labels, stored as JSON array or normalized tag rows
- `is_builtin` — boolean; builtin presets are not directly user-editable
- `is_favorite` — boolean; user-marked shortcut; ordered favorites are deferred
  beyond `0.1.0`
- `created_at`
- `updated_at`
- `row_version`

### `PromptPresetUsage`

Append-only reuse signal rows. Do not accumulate counts as mutable columns on
`PromptPreset`.

- `id`
- `prompt_preset_id`
- `scope_key` nullable
- `used_at`

Reuse signals are derived from these rows:

- **recent**: last N `used_at` per preset per `scope_key` (default N=10)
- **frequent**: rolling invocation count within a window (default 90 days)
- **last-used per repo**: single max `used_at` per preset + `scope_key`

### `ReviewTask`

- `id`
- `review_session_id`
- `review_run_id`
- `stage`
- `task_kind`
- `task_nonce`
- `objective`
- `turn_strategy`
- `allowed_scopes_json`
- `allowed_operations_json`
- `expected_result_schema`
- `prompt_preset_id` nullable
- `created_at`

### `WorkerInvocation`

- `id`
- `review_session_id`
- `review_run_id`
- `review_task_id`
- `provider`
- `provider_session_id` nullable
- `transport_kind`
- `started_at`
- `completed_at` nullable
- `outcome_state`
- `prompt_invocation_id` nullable
- `raw_output_artifact_id` nullable
- `result_artifact_id` nullable

### `WorkerToolCallEvent`

- `id`
- `review_task_id`
- `worker_invocation_id`
- `operation`
- `request_digest`
- `response_digest` nullable
- `outcome_state`
- `occurred_at`

### `WorkerStageResult`

- `schema_id`
- `review_session_id`
- `review_run_id`
- `review_task_id`
- `worker_invocation_id` nullable
- `task_nonce`
- `stage`
- `task_kind`
- `outcome_kind`
- `summary`
- `submitted_result_artifact_id` nullable
- `structured_findings_pack_artifact_id` nullable
- `clarification_requests_json` nullable
- `memory_review_requests_json` nullable
- `follow_up_proposals_json` nullable
- `memory_citations_json` nullable
- `artifact_refs_json` nullable
- `provider_metadata_json` nullable
- `warnings_json` nullable
- `created_at`

### `SessionBaselineSnapshot`

- `id`
- `review_session_id`
- `review_run_id` nullable
- `baseline_generation`
- `review_target_snapshot_json`
- `allowed_scopes_json`
- `default_query_mode`
- `candidate_visibility_policy`
- `prompt_strategy`
- `policy_epoch_refs_json`
- `degraded_flags_json`
- `created_at`

### `PromptInvocation`

- `id`
- `review_session_id`
- `review_run_id`
- `review_task_id`
- `worker_invocation_id`
- `prompt_preset_id` nullable — null for fully ad hoc prompts
- `turn_index`
- `resolved_text_digest`
- `resolved_text_artifact_id` nullable — cold artifact reference when resolved
  text exceeds inline threshold (suggested 4KB); digest still stored inline
- `user_override_text` nullable
- `source_surface` — Roger-owned origin enum such as `cli`, `tui`, `extension`,
  `external_link`, `harness_command`, `agent`, or `system`; legacy `direct`,
  `bridge`, or `external-link` values normalize into this enum
- `provider`
- `model_id`
- `stage` — `exploration`, `deep_review`, or `follow_up`
- `used_at`

### `OutcomeEvent`

Append-only typed events for analytics capture. Event kind taxonomy and naming
authority live in [`adr/009-prompt-preset-and-outcome-events.md`](./adr/009-prompt-preset-and-outcome-events.md).

- `id`
- `kind` — typed enum, not a free-form string
- `review_session_id`
- `review_run_id` nullable
- `prompt_invocation_id` nullable
- `actor_kind` nullable
- `actor_id` nullable
- `source_surface` — same normalized enum used by `PromptInvocation`
- `entity_id` nullable — primary entity id useful for indexing this event kind
- `entity_kind` nullable — discriminator for `entity_id`
- `payload_json` nullable — bounded structured payload for event-kind-specific
  fields
- `created_at`

Rules:

- `entity_id` and `entity_kind` are indexing aids, not substitutes for the
  canonical event kind plus payload
- storage should persist enough structured payload that later analytics and
  audit do not need to reconstruct meaning from free-form text blobs

### `MemoryReviewRequest`

- `id`
- `review_session_id`
- `review_run_id` nullable
- `subject_memory_id`
- `request_kind`
- `requested_target_state`
- `status`
- `reason_summary`
- `supporting_refs_json`
- `source_surface`
- `requested_by_actor_kind`
- `requested_by_actor_id` nullable
- `resolved_by_actor_kind` nullable
- `resolved_by_actor_id` nullable
- `resolution_summary` nullable
- `created_at`
- `resolved_at` nullable

### `Finding`

- `id`
- `review_session_id`
- `review_run_id`
- `fingerprint`
- `title`
- `normalized_summary`
- `severity`
- `confidence`
- `triage_state`
- `outbound_state`
- `first_seen_at`
- `last_seen_at`
- `row_version`

### `CodeEvidenceLocation`

- `id`
- `finding_id`
- `evidence_role`
- `repo_rel_path`
- `start_line`
- `start_column` nullable
- `end_line` nullable
- `end_column` nullable
- `excerpt_artifact_id` nullable
- `anchor_digest`
- `anchor_state`
- `created_at`

### `FindingDecisionEvent`

- `id`
- `finding_id`
- `from_triage_state`
- `to_triage_state`
- `from_outbound_state`
- `to_outbound_state`
- `actor`
- `reason_code`
- `note_artifact_id` when needed
- `created_at`

### `OutboundDraft`

- `id`
- `review_session_id`
- `review_run_id`
- `finding_id` nullable when one draft summarizes many findings
- `draft_batch_id`
- `repo_id`
- `remote_review_target_id`
- `payload_digest`
- `approval_state`
- `anchor_digest`
- `row_version`

### `OutboundDraftBatch`

- `id`
- `review_session_id`
- `review_run_id`
- `repo_id`
- `remote_review_target_id`
- `payload_digest`
- `approval_state`
- `approved_at`
- `invalidated_at`
- `invalidation_reason_code`

### `PostedAction`

- `id`
- `draft_batch_id`
- `provider`
- `remote_identifier`
- `status`
- `posted_payload_digest`
- `posted_at`
- `failure_code`

## Analytics-Ready Capture

Analytics should come from event history and outcome signals, not from scraping
mutable current state later.

Roger emits typed outcome events into the `OutcomeEvent` table as findings are
created, triaged, drafted, approved, posted, and labeled. See
`009-prompt-preset-and-outcome-events.md` for the full event kind taxonomy.

Minimum outcome-ready data:

- which task and turn strategy were used for each run (from `ReviewTask`)
- which prompt preset and resolved prompt text were used (from `PromptInvocation`)
- which findings were accepted, ignored, resolved, or left stale (from `OutcomeEvent` + `Finding`)
- which findings produced drafts (from `OutcomeEvent` `finding_draft_created`)
- which drafts were approved, invalidated, or posted (from `OutcomeEvent`)
- which posted actions map to remote review ids (from `OutcomeEvent` `draft_posted`)
- PR outcome state and merge outcome when available (from `OutcomeEvent` `pr_merged` / `pr_closed_unmerged`)
- explicit human usefulness labels when provided (from `OutcomeEvent` `usefulness_labeled`)
- which memory review requests were raised and how they resolved (from
  `MemoryReviewRequest` plus `OutcomeEvent` `memory_review_requested` /
  `memory_review_resolved` when emitted)

These queries must be answerable from `ReviewTask`, `PromptInvocation`, and
`OutcomeEvent` rows plus the existing `Finding` and `OutboundDraft` state rows
without requiring more than two additional joins.

This is enough to derive later usefulness scoring without turning `0.1.0` into
an analytics product.

## Migration and Rebuild Rules

- schema migrations apply to the canonical DB first
- sidecar indices must carry generation metadata and be rebuildable
- tokenizer, embedding-model, or schema changes should invalidate affected
  sidecars rather than forcing unsafe in-place mutation
- cold artifacts should be content-addressed and schema-version aware where
  format interpretation matters

## What Not To Do

- do not use full transcripts as the canonical operational state model
- do not make search sidecars the source of truth
- do not store giant prompt blobs or raw diffs inline in hot UI tables
- do not rely on ambient "current PR" state for posting
- do not let concurrent writers silently overwrite the same review session
