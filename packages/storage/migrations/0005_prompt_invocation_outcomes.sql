CREATE TABLE IF NOT EXISTS prompt_invocations (
    id TEXT PRIMARY KEY,
    review_session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    review_run_id TEXT NOT NULL REFERENCES review_runs(id) ON DELETE CASCADE,
    stage TEXT NOT NULL,
    prompt_preset_id TEXT NOT NULL,
    source_surface TEXT NOT NULL,
    resolved_text_digest TEXT NOT NULL,
    resolved_text_artifact_id TEXT REFERENCES artifacts(id) ON DELETE SET NULL,
    resolved_text_inline_preview TEXT,
    explicit_objective TEXT,
    provider TEXT,
    model TEXT,
    scope_context_json TEXT,
    config_layer_digest TEXT,
    launch_intake_id TEXT,
    used_at INTEGER NOT NULL,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX idx_prompt_invocations_session
ON prompt_invocations(review_session_id, used_at DESC);

CREATE INDEX idx_prompt_invocations_run
ON prompt_invocations(review_run_id, used_at DESC);

CREATE INDEX idx_prompt_invocations_preset
ON prompt_invocations(prompt_preset_id, used_at DESC);

CREATE TABLE IF NOT EXISTS outcome_events (
    id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    occurred_at INTEGER NOT NULL,
    review_session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    review_run_id TEXT REFERENCES review_runs(id) ON DELETE SET NULL,
    prompt_invocation_id TEXT REFERENCES prompt_invocations(id) ON DELETE SET NULL,
    actor_kind TEXT NOT NULL,
    actor_id TEXT,
    source_surface TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_outcome_events_session
ON outcome_events(review_session_id, occurred_at DESC);

CREATE INDEX idx_outcome_events_run
ON outcome_events(review_run_id, occurred_at DESC);

CREATE INDEX idx_outcome_events_invocation
ON outcome_events(prompt_invocation_id, occurred_at DESC);
