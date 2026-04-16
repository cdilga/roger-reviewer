CREATE TABLE IF NOT EXISTS worker_invocations (
    id TEXT PRIMARY KEY,
    review_session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    review_run_id TEXT NOT NULL REFERENCES review_runs(id) ON DELETE CASCADE,
    review_task_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    provider_session_id TEXT,
    transport_kind TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    outcome_state TEXT NOT NULL,
    prompt_invocation_id TEXT REFERENCES prompt_invocations(id) ON DELETE SET NULL,
    raw_output_artifact_id TEXT REFERENCES artifacts(id) ON DELETE SET NULL,
    result_artifact_id TEXT REFERENCES artifacts(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_worker_invocations_session
ON worker_invocations(review_session_id, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_worker_invocations_run
ON worker_invocations(review_run_id, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_worker_invocations_task
ON worker_invocations(review_task_id, started_at DESC);

CREATE INDEX IF NOT EXISTS idx_worker_invocations_prompt
ON worker_invocations(prompt_invocation_id, started_at DESC);

CREATE TABLE IF NOT EXISTS worker_tool_call_events (
    id TEXT PRIMARY KEY,
    review_task_id TEXT NOT NULL,
    worker_invocation_id TEXT NOT NULL REFERENCES worker_invocations(id) ON DELETE CASCADE,
    operation TEXT NOT NULL,
    request_digest TEXT NOT NULL,
    response_digest TEXT,
    outcome_state TEXT NOT NULL,
    occurred_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_worker_tool_call_events_invocation
ON worker_tool_call_events(worker_invocation_id, occurred_at ASC);

CREATE INDEX IF NOT EXISTS idx_worker_tool_call_events_task
ON worker_tool_call_events(review_task_id, occurred_at ASC);

CREATE TABLE IF NOT EXISTS worker_stage_results (
    review_session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    review_run_id TEXT NOT NULL REFERENCES review_runs(id) ON DELETE CASCADE,
    review_task_id TEXT NOT NULL,
    worker_invocation_id TEXT REFERENCES worker_invocations(id) ON DELETE SET NULL,
    schema_id TEXT NOT NULL,
    task_nonce TEXT NOT NULL,
    stage TEXT NOT NULL,
    task_kind TEXT NOT NULL,
    outcome_kind TEXT NOT NULL,
    summary TEXT NOT NULL,
    submitted_result_artifact_id TEXT REFERENCES artifacts(id) ON DELETE SET NULL,
    structured_findings_pack_artifact_id TEXT REFERENCES artifacts(id) ON DELETE SET NULL,
    clarification_requests_json TEXT,
    memory_review_requests_json TEXT,
    follow_up_proposals_json TEXT,
    memory_citations_json TEXT,
    artifact_refs_json TEXT,
    provider_metadata_json TEXT,
    warnings_json TEXT,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_worker_stage_results_run
ON worker_stage_results(review_run_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_worker_stage_results_task
ON worker_stage_results(review_task_id, created_at DESC);

CREATE UNIQUE INDEX IF NOT EXISTS idx_worker_stage_results_invocation
ON worker_stage_results(worker_invocation_id)
WHERE worker_invocation_id IS NOT NULL;
