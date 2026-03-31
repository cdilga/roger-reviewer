CREATE TABLE IF NOT EXISTS merged_resolution_links (
    id TEXT PRIMARY KEY,
    prompt_invocation_id TEXT NOT NULL REFERENCES prompt_invocations(id) ON DELETE CASCADE,
    review_session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    review_run_id TEXT REFERENCES review_runs(id) ON DELETE SET NULL,
    source_outcome_event_id TEXT REFERENCES outcome_events(id) ON DELETE SET NULL,
    resolution_kind TEXT NOT NULL,
    source_kind TEXT NOT NULL,
    source_id TEXT NOT NULL,
    remote_identifier TEXT,
    resolved_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_merged_resolution_links_prompt
ON merged_resolution_links(prompt_invocation_id, resolved_at DESC);

CREATE INDEX IF NOT EXISTS idx_merged_resolution_links_source_event
ON merged_resolution_links(source_outcome_event_id);

CREATE TABLE IF NOT EXISTS usage_event_derivation_jobs (
    id TEXT PRIMARY KEY,
    prompt_invocation_id TEXT NOT NULL REFERENCES prompt_invocations(id) ON DELETE CASCADE,
    review_session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    review_run_id TEXT REFERENCES review_runs(id) ON DELETE SET NULL,
    seed_outcome_event_id TEXT REFERENCES outcome_events(id) ON DELETE SET NULL,
    job_kind TEXT NOT NULL,
    status TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    started_at INTEGER,
    completed_at INTEGER,
    failure_reason TEXT,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_usage_event_derivation_jobs_prompt
ON usage_event_derivation_jobs(prompt_invocation_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_usage_event_derivation_jobs_status
ON usage_event_derivation_jobs(status, updated_at DESC);
