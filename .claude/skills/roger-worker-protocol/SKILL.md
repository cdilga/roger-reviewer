---
name: Roger Worker Protocol
description: Use when you are the review worker inside a Roger-managed provider session and need to call the rr agent worker transport — binding to a ReviewTask via a task file, reading context/memory/findings, submitting stage results, and staying inside Roger's boundary (no GitHub posting, no approval, no canonical writes).
---

# Roger Worker Protocol

You are the review worker, not the review manager. Roger schedules review
tasks and validates results; you perform bounded analysis and return
proposals. Full contract: `docs/REVIEW_WORKER_RUNTIME_AND_BOUNDARY_CONTRACT.md`.

If you are driving `rr` as the operator from outside a provider session, use
`roger-review-driver` instead. If you are in a bare-harness Roger-native
subset without a bound `rr agent` task, use `roger-inside-roger-agent`
instead. This skill is specifically for the worker side of an active
`rr agent worker.*` task binding.

## Task binding

You are bound to one `ReviewTask` via a task file — a JSON file whose path
the seed prompt or operator gives you. It carries `review_session_id`,
`review_run_id`, `stage`, `task_kind`, `task_nonce`, `objective`,
`allowed_scopes`, `allowed_operations`, and `expected_result_schema`. Every
`rr agent` call you make targets this file with `--task-file <path>`. The
`task_nonce` must round-trip through every response you submit; the
transport rejects a stale or mismatched nonce, and it rejects a result whose
session/run/task/stage does not match the active binding.

## Call sequence

```bash
rr agent worker.get_review_context   --task-file <path>
rr agent worker.search_memory        --task-file <path> --request-file <req.json>
rr agent worker.list_findings        --task-file <path>
rr agent worker.get_finding_detail    --task-file <path> --request-file <req.json>
rr agent worker.get_artifact_excerpt  --task-file <path> --request-file <req.json>
rr agent worker.get_status            --task-file <path>
# ... do the review ...
rr agent worker.submit_stage_result   --task-file <path> --request-file <result.json>
rr agent worker.request_clarification --task-file <path> --request-file <req.json>
rr agent worker.request_memory_review --task-file <path> --request-file <req.json>
rr agent worker.propose_follow_up     --task-file <path> --request-file <req.json>
```

Those ten operations are the whole live surface. Each call sends a
`WorkerOperationRequestEnvelope` (via stdin or `--request-file`) whose
`operation` field must match the `<operation>` argv token — a mismatch fails
closed. Responses are `rr.agent.response.v1` envelopes: a transport distinct
from `--robot`, and `rr agent` rejects `--robot` outright. `rr agent` is not a
general launch surface; it is only valid inside an active, bound
`ReviewSession`/`ReviewRun`/`ReviewTask`.

## Recommended order of operations

1. `worker.get_review_context` first. Returns the bounded
   `WorkerContextPacket`: review target identity, session/run identity, a
   baseline snapshot/reference, provider and transport identity, current
   stage and objective, allowed scopes and operations, mutation and GitHub
   posture, an unresolved-findings summary, stage/continuity summary, and any
   memory/evidence cards or artifact references Roger preloaded. It is
   bounded and explicit, not an ambient dump — broader scopes must be named
   in the packet or fetched through an explicit call below.
2. `worker.search_memory` before writing new analysis, so you don't
   re-derive what Roger already knows. Pass task binding, query text or
   anchor hints, `query_mode`, requested retrieval classes, and requested
   scopes. Results come back as three buckets — `promoted_memory`,
   `tentative_candidates`, `evidence_hits` — each a `RecallEnvelope`
   projection carrying `requested_query_mode`, `resolved_query_mode`,
   `retrieval_mode`, provenance, scope, trust, degraded flags, and citation
   ids. Candidate memory stays visibly tentative; never treat it as proven.
   A scope outside what the task allows fails closed with an explicit denial.
3. `worker.list_findings` / `worker.get_finding_detail` /
   `worker.get_artifact_excerpt` as needed for clarification or recheck work.
   Large payloads stay in cold artifacts; excerpts are budgeted.
4. Do the actual review — reading code, gathering evidence — using your
   provider-native tools. That work is not an `rr agent` call.
5. `worker.submit_stage_result` is the normal return path for completed work.
   It carries the `WorkerStageResult` envelope, optionally nesting a
   `StructuredFindingsPack`. Roger validates binding, schema, and nonce, then
   routes any nested findings pack through Roger's own structured-findings
   validation and repair loop — the worker never writes canonical `Finding`
   rows directly.
6. `worker.request_clarification`, `worker.request_memory_review`, and
   `worker.propose_follow_up` are advisory and available any time, not only
   at the end. None of them mutate finding, memory, approval, or posting
   state directly:
   - `request_clarification` opens or extends a clarification thread
     attached to Roger-owned finding/session lineage — it is not a
     finding-state change.
   - `request_memory_review` proposes `promote | demote | deprecate |
     restore | mark_anti_pattern` with evidence and rationale; Roger decides
     the resolution.
   - `propose_follow_up` suggests additional work and must cite the task or
     finding it derives from; Roger schedules any resulting `ReviewTask`, you
     do not schedule it yourself.

## `WorkerStageResult` shape (summary)

Required fields: `schema_id`, `review_session_id`, `review_run_id`,
`review_task_id`, `task_nonce`, `stage`, `task_kind`, `outcome`, `summary`.
Optional payload sections: `structured_findings_pack`,
`clarification_requests`, `memory_review_requests`, `follow_up_proposals`
(`follow_up_hints` remains an accepted legacy alias on ingest),
`memory_citations`, `artifact_refs`, `provider_metadata`, `warnings`.
`outcome` is one of `completed | completed_partial | needs_clarification |
needs_context | abstained | failed` — partial, repair-needed, and
clarification-needed outcomes are all valid so long as they are explicit.

## `StructuredFindingsPack` (summary)

The nested findings payload inside `WorkerStageResult` when the task
produced findings. It is validated exactly as Roger-owned schema, never as
provider-owned truth. Roger preserves the raw output, the submitted result
envelope, and the materialized normalized findings as three separate things
— that is the line between "the worker did some review work" and "Roger
accepted specific findings into canonical state."

## Hard boundaries — never do these

- Never post to GitHub, never approve or post a draft batch, never issue a
  raw `gh` review-communication write. Findings stay local until a human
  operator drives the outbound chain (`rr send ...`) from outside your
  session.
- Never mutate canonical Roger state directly. Findings, triage/outbound
  state, and durable memory are Roger-owned; you return proposals and
  citations, Roger decides what becomes durable truth.
- Never widen scope beyond `ReviewTask.allowed_scopes` /
  `WorkerContextPacket.allowed_scopes`. You may request a narrower scope than
  the task allows; you may not request a broader one.
- Never invent operations outside the ten listed above. Capability claims
  are transport- and provider-specific; Roger will not honor an operation it
  did not declare for this session.
- Never retry with a fabricated or reused nonce after a rejection. A nonce
  mismatch or stale task binding is a fail-closed signal, not a retry
  target — surface it and stop.
- No file mutation, shell execution, or external URL access by default in
  review mode, no provider-local memory write as a substitute for Roger
  memory, and no broad built-in MCP access by default — unless the task
  packet explicitly grants a wider elevated mode.
