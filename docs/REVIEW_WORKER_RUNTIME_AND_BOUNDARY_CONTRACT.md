# Review Worker Runtime And Boundary Contract

Status: Active implementation-facing support contract for `0.1.x`.

This document closes a missing planning seam in Roger Reviewer: the boundary
between Roger as the review manager and the agent/runtime that actually performs
review work inside a provider session.

Roger already has documents for harness linkage, prompt snapshots, findings
validation, and memory policy. What is still missing is the first-class contract
for the review worker itself:

- how Roger gives the worker bounded context
- how the worker asks Roger for memory, findings, and artifacts
- how the worker returns findings, clarification requests, and task outcomes
- where the semantic line sits between manager commands and worker tools
- whether this surface should be CLI-shaped, MCP-shaped, or transport-neutral

This contract makes that line explicit.

## Authority

- [`AGENTS.md`](/Users/cdilga/Documents/dev/roger-reviewer/AGENTS.md)
- [`docs/PLAN_FOR_ROGER_REVIEWER.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_ROGER_REVIEWER.md)
- [`docs/HARNESS_SESSION_LINKAGE_CONTRACT.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/HARNESS_SESSION_LINKAGE_CONTRACT.md)
- [`docs/PROMPT_PRESET_AND_OUTCOME_CONTRACT.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PROMPT_PRESET_AND_OUTCOME_CONTRACT.md)
- [`docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/SEARCH_MEMORY_LIFECYCLE_AND_SEMANTIC_ASSET_POLICY.md)
- [`docs/PLAN_FOR_TRANSACTIONAL_LAUNCH_LIFECYCLE_AND_BRIDGE_TRUTH.md`](/Users/cdilga/Documents/dev/roger-reviewer/docs/PLAN_FOR_TRANSACTIONAL_LAUNCH_LIFECYCLE_AND_BRIDGE_TRUTH.md)

If this document conflicts with the canonical plan, the canonical plan wins
until the plan is deliberately updated.

Normative-home rule:

- this is the normative home for the review-worker seam
- the canonical plan should summarize and point here
- other docs should reference this contract rather than redefining the same
  object model in parallel

## Why This Contract Exists

The current repo truth is directionally correct but still underspecified:

- `packages/cli/src/lib.rs` owns launch, resume, refresh, and store writes, but
  it does not define a first-class Roger-owned worker API
- `packages/prompt-engine/src/stage_execution.rs` currently models the worker as
  a very thin `StageHarness` that takes `prompt_text` and returns raw output
  plus an optional structured pack
- provider crates mostly model session launch/reopen/reseed, not a Roger tool
  surface that the worker can call during review
- the search/memory policy explains what Roger may retrieve, but not how a live
  review worker is supposed to ask for it

That leaves a large unresolved gap in the middle of the architecture.

Without this contract, Roger cannot answer these questions rigorously:

- Is the review worker just "whatever prompt is running in the provider"?
- Does the worker call the normal `rr` CLI, a separate machine-only CLI, or an
  MCP server?
- Can the worker ask for memory, findings, or artifacts directly, and if so in
  what schema?
- Can the worker mutate local review state directly, or only propose changes?
- How does Roger distinguish manager-facing commands from worker-facing tools?

That ambiguity is now costly enough to be treated as a product bug and planning
gap, not a later cleanup.

## Core Principle

Roger manages reviews. The review worker performs review tasks. The harness
hosts the worker session.

Those are three distinct roles.

## Boundary Delineation

| Surface | Owns | May do | Must not do |
| --- | --- | --- | --- |
| Roger review manager | session/run lifecycle, task scheduling, approval/posting, finding materialization, memory policy, audit trail | launch providers, assign review tasks, serve bounded tools/context, validate results, persist canonical state | delegate safety-critical truth or GitHub mutation to provider or worker |
| Review worker | analysis, evidence synthesis, bounded clarification, structured finding proposal | read Roger context, call Roger-owned review tools, return task results and finding proposals | write canonical review state directly, change triage/outbound state directly, approve/post to GitHub |
| Harness/provider | process/session hosting, reopen/reseed support, transcript/raw-output capture, optional in-session command transport | host the worker and any transport Roger uses | become the source of truth for Roger review state or bypass Roger guardrails |

This is the key semantic split the repo must now preserve everywhere.

## Non-Goals

This contract does not authorize:

- direct worker writes to the Roger database
- direct worker promotion/demotion of durable memory
- direct GitHub review posting from the worker
- provider-specific ad hoc tool APIs as the canonical Roger worker surface
- mandatory MCP adoption in `0.1.x`
- silent widening into fix mode, write mode, or "YOLO" review mode

## Architectural Decision

Roger should define the review worker surface as a transport-neutral contract,
then expose it through one or more adapters.

Recommended decision for `0.1.x`:

1. define a Roger-owned worker contract above any transport
2. make a dedicated agent-only command transport the first real adapter
3. keep MCP optional and secondary unless a provider or client proves it is
   materially better than the dedicated local transport

This mirrors the right lesson from Agent Mail:

- the stable thing is the operation model and envelope contract
- transports are adapters over that model
- transport choice must not define core semantics

## Canonical Worker Objects

### `ReviewTask`

One manager-scheduled unit of review work.

Required fields:

- `id`
- `review_session_id`
- `review_run_id`
- `stage`
- `task_kind`
- `task_nonce`
- `objective`
- `turn_strategy`
- `allowed_scopes`
- `allowed_operations`
- `expected_result_schema`
- `prompt_invocation_id` when the task is backed by a resolved Roger prompt

Suggested `task_kind` values for `0.1.x`:

- `exploration_pass`
- `deep_review_pass`
- `follow_up_pass`
- `refresh_compare`
- `clarification_pass`
- `recheck_finding`

Suggested `turn_strategy` values:

- `single_turn_report`
- `configured_multi_turn_program`
- `manual_follow_up`

Rules:

- a `ReviewTask` is created by Roger, not by the provider
- the worker may request more context or propose follow-up work, but it does
  not schedule new canonical tasks directly
- `task_nonce` must round-trip through every worker result so Roger can reject
  stale or cross-session submissions

### Prompt-program and turn model

Roger should make the initial-turn model explicit instead of leaving it as
prompt glue.

Required rules:

- the default `0.1.x` path is `single_turn_report`
- the manager resolves one preset-backed prompt
- the worker returns findings or an explicit non-finding outcome
- later operator-directed work from the TUI palette or freehand input creates a
  new `ReviewTask` bound to the same `ReviewSession`

Roger should also support `configured_multi_turn_program` tasks.

Required behavior:

- config may define an ordered turn plan for a task
- a turn plan may perform several exploration or codebase-learning turns before
  a final reporting turn
- a common first-class example is `explore x3 -> report findings`
- every turn still records its own `PromptInvocation`
- only the terminal reporting turn is required to emit a findings pack unless
  the task configuration says otherwise
- the operator may abandon the Roger-managed task flow and drop to the bare
  provider session at any point where the harness contract allows it

This keeps the default simple while making multi-turn review programs explicit,
configurable, and auditable.

### `WorkerContextPacket`

Bounded manager-owned context given to the worker for one `ReviewTask`.

Required payload classes:

- review target identity
- session/run identity
- provider and transport identity
- current stage and task objective
- allowed scopes and allowed operations
- mutation posture and GitHub posture
- current unresolved findings summary
- stage summary / continuity summary
- selected memory cards or evidence cards that Roger preloaded
- artifact references needed for the task

Rules:

- the packet is bounded and explicit, not an ambient dump of Roger state
- broader memory scopes must be named in the packet or acquired through an
  explicit worker tool call
- the packet must be reproducible for audit from Roger-owned state plus cold
  artifacts

### `WorkerCapabilityProfile`

The declared worker-facing capability set for one provider/transport pair.

Required fields:

- `transport_kind`
- `supports_context_reads`
- `supports_memory_search`
- `supports_finding_reads`
- `supports_artifact_reads`
- `supports_stage_result_submission`
- `supports_clarification_requests`
- `supports_follow_up_hints`
- `supports_fix_mode` boolean, false by default in review mode

Rules:

- capability claims are transport-specific and provider-specific
- Roger must not claim a worker affordance exists unless the provider and
  transport actually expose it truthfully

### `WorkerInvocation`

Append-only audit record for one execution attempt of one `ReviewTask`.

Required fields:

- `id`
- `review_session_id`
- `review_run_id`
- `review_task_id`
- `provider`
- `provider_session_id`
- `transport_kind`
- `started_at`
- `completed_at` nullable
- `outcome_state`
- `prompt_invocation_id` when applicable
- `raw_output_artifact_id` nullable
- `result_artifact_id` nullable

Rules:

- worker execution attempts are auditable independently from `ReviewRun`
- a failed or partial worker execution is still a first-class event
- a `WorkerInvocation` owns zero or more `PromptInvocation` turns
- a successful task attempt produces exactly one terminal `WorkerStageResult`

### `WorkerToolCallEvent`

Append-only record for one Roger-owned tool or context call made by the worker.

Required fields:

- `id`
- `review_task_id`
- `worker_invocation_id`
- `operation`
- `request_digest`
- `response_digest`
- `occurred_at`

Rules:

- Roger should persist enough to audit which context the worker actually asked
  for, without turning every response into hot-row bloat
- inline previews may exist, but full payloads belong in bounded artifacts when
  large

### `WorkerStageResult`

The canonical result envelope returned by the worker for one `ReviewTask`.

Required fields:

- `schema_id`
- `review_session_id`
- `review_run_id`
- `review_task_id`
- `task_nonce`
- `stage`
- `task_kind`
- `outcome`
- `summary`

Optional payload sections:

- `structured_findings_pack`
- `clarification_requests`
- `follow_up_hints`
- `memory_citations`
- `artifact_refs`
- `provider_metadata`
- `warnings`

Suggested `outcome` values:

- `completed`
- `completed_partial`
- `needs_clarification`
- `needs_context`
- `abstained`
- `failed`

Rules:

- the worker never returns canonical `Finding` rows directly
- the worker returns proposals and structured payloads that Roger validates and
  materializes
- Roger must reject a result whose session/run/task/stage/nonce does not match
  the active task binding

## Manager-Owned Worker API

Roger should define a logical worker API independent of CLI syntax or MCP tool
registration.

## Execution-Record Precedence Rule

Roger should use this precedence chain:

1. `ReviewIntake`
   - records the operator or surface trigger
2. `ReviewRun`
   - records the durable Roger-visible pass created from that trigger
3. `ReviewTask`
   - records one manager-scheduled unit of work inside the run
4. `WorkerInvocation`
   - records one attempt to execute that task
5. `PromptInvocation`
   - records each exact prompt turn sent during that task attempt
6. `WorkerToolCallEvent`
   - records Roger-owned tool/context calls made during that task attempt
7. `WorkerStageResult`
   - records the terminal task result proposal returned to Roger
8. `Finding` / `ClarificationThread` / `OutcomeEvent`
   - record canonical Roger state materialized from the accepted result

Required mapping rules:

- one `ReviewRun` may own one or more `ReviewTask` rows
- the default path is one run, one task, one worker invocation, one prompt
  invocation, one terminal result
- a `configured_multi_turn_program` task still owns one terminal result but may
  own several prompt invocations
- `PromptInvocation` remains the exact prompt-text record and does not replace
  the task/result ledger
- `WorkerStageResult` is the terminal result record and does not replace the
  prompt-text ledger

This is the canonical answer to the overlapping-ledger problem.

### Required read operations

#### `worker.get_review_context`

Returns the current `WorkerContextPacket` for a bound task.

Required behavior:

- reject unbound or stale task references
- include allowed operation set and policy posture
- include only bounded hot-path context inline

#### `worker.search_memory`

Returns explicit retrieval buckets for the current task.

Required inputs:

- task binding
- query text or anchor hints
- `query_mode`
- requested retrieval classes
- requested scopes

Required output buckets:

- `promoted_memory`
- `tentative_candidates`
- `evidence_hits`

Rules:

- every returned item is a `RecallEnvelope` projection from the canonical
  search/memory contract
- the output must preserve `query_mode`, `retrieval_mode`, provenance, scope,
  trust, degraded flags, citation posture, and citation ids
- candidate memory must remain visibly tentative
- out-of-scope or policy-disallowed requests must fail closed
- `promotion_review` is a retrieval posture only; it does not mutate memory
  state directly

#### `worker.list_findings`

Returns finding summaries for the current session or selected subset.

Required data:

- finding id
- fingerprint
- triage state
- outbound state
- summary
- primary evidence reference

#### `worker.get_finding_detail`

Returns the bounded detail needed for clarification or recheck work.

Required data:

- finding summary
- evidence locations
- clarification lineage when relevant
- outbound linkage when relevant

#### `worker.get_artifact_excerpt`

Returns a bounded excerpt or digest-backed reference for a requested artifact.

Rules:

- large payloads stay in cold artifacts
- excerpts must be budgeted and auditable

#### `worker.get_status`

Returns the manager-owned session/status summary relevant to task execution.

This is worker-facing status, not human-oriented CLI prose.

### Required write/proposal operations

#### `worker.submit_stage_result`

The normal return path for completed review work.

Required behavior:

- accept `WorkerStageResult`
- validate binding, schema, and nonce
- preserve the raw submitted envelope as an artifact when needed
- route any nested findings pack through Roger's structured-findings validation
  and repair loop

#### `worker.request_clarification`

Lets the worker ask Roger to open or extend a clarification thread without
directly mutating finding truth.

Rules:

- clarification requests are attached to Roger-owned finding/session lineage
- a clarification request is not a finding-state change

#### `worker.request_memory_review`

Lets the worker propose a `MemoryReviewRequest` without mutating durable memory.

Rules:

- requests may ask Roger to `promote`, `demote`, `deprecate`, `restore`, or
  `mark_anti_pattern`
- the worker submits evidence and rationale, not a direct state transition
- accepted or rejected resolution remains Roger-owned review logic and operator
  visibility

#### `worker.propose_follow_up`

Lets the worker suggest additional work without scheduling it directly.

Rules:

- suggestions are advisory until Roger schedules a canonical `ReviewTask`
- follow-up proposals must cite the task or finding they derive from

## Memory And Finding Semantics For The Worker

The worker does not get "memory" as a loose provider feature. It gets Roger
retrieval results through Roger's scope and trust policy.

Required rules:

- worker memory access goes through Roger-owned retrieval operations
- memory results must be provenance-tagged and citation-capable
- candidate or contradicted memory must not silently behave like proven memory
- the worker must be able to cite memory or evidence ids in returned findings
  and summaries
- direct worker writes to durable memory are out of scope for `0.1.x`

### Scope-authority chain

Allowed worker scope is derived in this order:

1. config and policy baseline
2. `ReviewIntake` and launch-context selectors
3. `ReviewSession` allowed-scope baseline
4. `ReviewTask.allowed_scopes`
5. `WorkerContextPacket.allowed_scopes`
6. worker-requested scopes as a subset only

Rules:

- the worker may request narrower scopes than the task allows
- the worker may not widen scopes beyond the task packet
- any requested scope outside the allowed set must fail closed with an explicit
  denial result
- broader overlays such as `project` or `org` must be granted by session/task
  policy rather than by worker preference alone

Promotion, demotion, and usefulness remain Roger-owned outcomes derived from:

- worker citations
- finding lineage
- approval/posting outcomes
- merged-resolution links
- later human review actions

The worker can propose and cite. Roger decides what becomes durable truth.

## Findings Return Contract

The current `StructuredFindingsPack` contract remains necessary but is not
sufficient on its own.

Roger now needs two layers:

1. `WorkerStageResult` as the task/result envelope
2. `StructuredFindingsPack` as the nested structured findings payload when the
   task produced findings

Rules:

- every findings-producing worker task returns a `WorkerStageResult`
- the findings pack, when present, is validated exactly as Roger-owned schema,
  not as provider-owned truth
- Roger preserves raw output, the submitted result envelope, and the
  materialized normalized findings separately
- partial findings, repair-needed results, or clarification-needed results are
  all valid outcomes so long as they are explicit

This is the line between "the worker did some review work" and "Roger accepted
specific findings into canonical state."

## Review Mode Policy Boundary

The review worker runs under Roger-owned policy, not provider-default ambient
power.

Default review-mode rules:

- no direct GitHub write path
- no raw `gh` review communication path
- no direct finding-state mutation
- no direct approval/posting operations
- no file mutation by default
- no shell execution by default
- no external URL access by default
- no provider-local memory write as a substitute for Roger memory
- no broad built-in MCP server access by default when Roger is in review mode

Fix mode, mutation-capable tools, or wider provider capabilities require an
explicit elevated mode and must remain visibly distinct.

## Transport Strategy

The worker contract must not be defined by one transport, but Roger still needs
to choose a first real transport.

### Option A: Reuse the existing human CLI surface

This is not sufficient as the canonical answer.

Why it is weak:

- the existing CLI verbs are manager/operator oriented
- high-frequency worker tool calls have different semantics from human session
  management
- overloading `rr status`, `rr findings`, and `rr search` as the whole worker
  protocol blurs review-manager actions with review-worker tool access

Short-term reuse is acceptable for narrow proof slices, but it should not be
the long-term worker contract.

### Option B: Dedicated agent-session command transport

This is the recommended `0.1.x` baseline.

Recommended shape:

- a dedicated `rr agent ...` family for in-session agent calls
- transport maps directly onto the logical worker operations above
- all responses are stable machine-readable envelopes
- human-facing prose stays out of the worker path
- calls require Roger-owned session/run/task binding plus task nonce
- the surface is valid only inside an active Roger-managed agentic session

Why this is the right default:

- semantically clean separation from manager-facing CLI commands
- smaller scope than MCP
- easier to validate deterministically
- works for provider-hosted workers without making Roger protocol-first
- makes the distinction from `--robot` explicit rather than implicit

### `rr agent` versus `--robot`

These are different surfaces and must stay different.

- `rr --robot`
  - machine-readable transport over operator-facing commands
  - used by automation, bridge flows, and external tooling that needs ordinary
    Roger command semantics
- `rr agent`
  - agent-only in-session transport
  - used by the Roger-managed review worker during an active task
  - exposes worker context/tool calls and result submission

Rules:

- `rr agent` is not a general launch surface
- `rr agent` is not a substitute for `--robot`
- `rr agent` requires a valid bound `ReviewSession`, `ReviewRun`,
  `ReviewTask`, and `task_nonce`
- human help text should clearly demote `rr agent` relative to ordinary
  operator commands

### Option C: MCP adapter

MCP is a valid future transport, but it should be optional in `0.1.x`, not the
foundational answer.

Use MCP only after the following are true:

- the canonical worker objects and operations are fixed
- at least one real provider or editor client materially benefits from MCP over
  the dedicated worker transport
- Roger can enforce the same policy boundary, session binding, and audit
  semantics through the MCP adapter

If MCP lands, it must be:

- a Roger-owned local adapter over the same worker operations
- read-mostly in review mode
- unable to bypass finding validation, approval gates, or posting controls
- unable to widen provider power silently through ambient tool registration

### MCP decision for now

Do not make MCP the required first implementation.

First make the worker contract transport-neutral and real. Then add an MCP
adapter only if the dedicated worker transport proves insufficient or a client
like GitHub Copilot clearly justifies the extra surface area.

## Continuity And Recovery Rule

Worker audit history should survive recovery, but it should not bloat
`ResumeBundle`.

Required rules:

- `ResumeBundle` should carry compact worker-continuity summary only
- full `WorkerInvocation`, `WorkerToolCallEvent`, and submitted-result history
  remains in canonical rows plus cold artifacts
- reseed/recovery reconstructs worker history from Roger-owned state rather
  than serializing full tool-call history into the bundle
- if a pending task was interrupted, recovery must surface whether Roger is
  resuming the same task, replacing it with a new task, or abandoning it

## Internal Refactor Direction

The current `StageHarness` abstraction is too thin to model the worker boundary
cleanly.

Recommended split:

- `ReviewWorkerTransport`
  - provider-facing/session-facing runtime edge
  - hosts the worker inside OpenCode, Copilot, or another harness
- `ReviewWorkerGateway`
  - Roger-owned tool/context edge used by the worker
  - exposes the logical worker operations
- `WorkerStageResult`
  - replaces the current "raw output plus optional structured pack" shape as
    the canonical worker return envelope
- `rr agent`
  - first concrete agent-session transport over the gateway
  - distinct from `--robot` and from optional harness-native `roger-*`
    commands

Practical consequence:

- prompt execution stops being a black box
- worker tool use becomes auditable
- memory/finding access becomes explicit and policy-governed
- provider transport and Roger review logic stop bleeding into each other

## Validation Requirements

This contract is important enough that it needs its own proof story.

### Unit / schema validation

- `ReviewTask`, `WorkerContextPacket`, `WorkerInvocation`,
  `WorkerToolCallEvent`, and `WorkerStageResult` round-trip cleanly
- nonce mismatch, stale task binding, and schema mismatch fail closed
- capability-profile gating is deterministic

### Integration validation

- a worker can retrieve bounded review context through the manager-owned worker
  transport
- a worker can search memory and receive explicit provenance buckets
- a worker can read finding detail and artifact excerpts without bypassing
  policy
- a worker can submit a stage result and have Roger materialize structured
  findings truthfully
- partial/repair-needed/clarification-needed outcomes surface explicitly
- forbidden operations are denied explicitly

### Provider acceptance

For any provider that claims serious worker support:

- at least one support-appropriate acceptance path must show the worker calling
  Roger-owned context or retrieval operations
- at least one support-appropriate acceptance path must show the worker
  returning a `WorkerStageResult` that Roger validates and materializes

### Negative-path validation

- worker requests broader memory scope than allowed
- worker returns a result bound to the wrong task/session/run
- worker attempts a forbidden mutation-capable operation
- worker submits malformed findings pack inside an otherwise valid result
- worker transport disappears mid-task and Roger preserves failure truth

## Delivery Slices

Recommended implementation order:

1. freeze the worker objects and transport-neutral worker operations
2. add manager-owned worker gateway types in `packages/app-core`
3. add a dedicated machine-facing worker transport in `packages/cli`
4. refactor `packages/prompt-engine` to consume `WorkerStageResult` rather than
   the current thin `StageHarnessOutput`
5. update provider packages to host the worker through the new boundary
6. extend validation and support matrices to defend the new claim

## Immediate Consequences For The Canonical Plan

The canonical plan should now treat this as a core architectural contract, not
an implied behavior hidden inside harness linkage or prompt execution.

The README should also present this document as one of the repo's core
contracts, because it defines the most important semantic split in the product:

- Roger manages the review
- the worker performs bounded review tasks
- the provider hosts the worker but does not own Roger truth
