CREATE TABLE IF NOT EXISTS launch_attempts (
    id TEXT PRIMARY KEY,
    action TEXT NOT NULL CHECK (
        action IN (
            'start_review',
            'resume_review',
            'return_to_roger'
        )
    ),
    provider TEXT NOT NULL,
    source_surface TEXT NOT NULL CHECK (
        source_surface IN (
            'cli',
            'tui',
            'extension',
            'bridge'
        )
    ),
    review_target TEXT NOT NULL,
    requested_session_id TEXT REFERENCES review_sessions(id) ON DELETE SET NULL,
    final_session_id TEXT REFERENCES review_sessions(id) ON DELETE SET NULL,
    launch_binding_id TEXT REFERENCES session_launch_bindings(id) ON DELETE SET NULL,
    state TEXT NOT NULL CHECK (
        state IN (
            'pending',
            'dispatching',
            'awaiting_provider_verification',
            'committing',
            'verified_started',
            'verified_reopened',
            'verified_reseeded',
            'failed_preflight',
            'failed_spawn',
            'failed_provider_verification',
            'failed_session_binding',
            'failed_commit',
            'abandoned'
        )
    ),
    provider_session_id TEXT,
    verified_locator TEXT,
    failure_reason TEXT,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    finalized_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_launch_attempts_requested_session
ON launch_attempts(requested_session_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_launch_attempts_final_session
ON launch_attempts(final_session_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_launch_attempts_state_updated
ON launch_attempts(state, updated_at DESC);
