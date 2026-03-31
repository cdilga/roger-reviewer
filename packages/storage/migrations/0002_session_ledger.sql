ALTER TABLE review_sessions ADD COLUMN provider TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE review_sessions ADD COLUMN continuity_state TEXT NOT NULL DEFAULT '{}';
ALTER TABLE review_sessions ADD COLUMN launch_profile_id TEXT;

ALTER TABLE review_runs ADD COLUMN continuity_quality TEXT NOT NULL DEFAULT 'degraded';
ALTER TABLE review_runs ADD COLUMN session_locator_artifact_id TEXT;

CREATE TABLE IF NOT EXISTS session_launch_bindings (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    surface TEXT NOT NULL,
    launch_profile_id TEXT,
    cwd TEXT,
    worktree_root TEXT,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_session_launch_bindings_session
    ON session_launch_bindings(session_id);

