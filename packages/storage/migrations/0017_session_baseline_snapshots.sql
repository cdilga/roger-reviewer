CREATE TABLE IF NOT EXISTS session_baseline_snapshots (
    id TEXT PRIMARY KEY,
    review_session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    review_run_id TEXT REFERENCES review_runs(id) ON DELETE SET NULL,
    baseline_generation INTEGER NOT NULL,
    review_target_snapshot TEXT NOT NULL,
    allowed_scopes_json TEXT NOT NULL,
    default_query_mode TEXT NOT NULL,
    candidate_visibility_policy TEXT NOT NULL,
    prompt_strategy TEXT NOT NULL,
    policy_epoch_refs_json TEXT NOT NULL,
    degraded_flags_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_session_baseline_snapshots_session_generation
ON session_baseline_snapshots(review_session_id, baseline_generation);

CREATE INDEX IF NOT EXISTS idx_session_baseline_snapshots_session_created_at
ON session_baseline_snapshots(review_session_id, created_at DESC);
