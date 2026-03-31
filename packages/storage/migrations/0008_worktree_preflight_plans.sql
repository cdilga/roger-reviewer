CREATE TABLE IF NOT EXISTS launch_preflight_plans (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    launch_binding_id TEXT REFERENCES session_launch_bindings(id) ON DELETE SET NULL,
    result_class TEXT NOT NULL CHECK (
        result_class IN (
            'ready',
            'ready_with_actions',
            'profile_required',
            'unsafe_default_blocked',
            'verification_failed'
        )
    ),
    selected_mode TEXT NOT NULL CHECK (
        selected_mode IN (
            'current_checkout',
            'named_instance',
            'worktree'
        )
    ),
    resource_decisions_json TEXT NOT NULL,
    required_operator_actions_json TEXT NOT NULL,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_launch_preflight_plans_session_updated
ON launch_preflight_plans(session_id, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_launch_preflight_plans_binding_updated
ON launch_preflight_plans(launch_binding_id, updated_at DESC);
