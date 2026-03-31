CREATE TABLE IF NOT EXISTS memory_items (
    id TEXT PRIMARY KEY,
    scope_key TEXT NOT NULL,
    memory_class TEXT NOT NULL,
    state TEXT NOT NULL,
    statement TEXT NOT NULL,
    normalized_key TEXT NOT NULL,
    anchor_digest TEXT,
    source_kind TEXT NOT NULL,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_memory_items_scope_state
ON memory_items(scope_key, state, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_memory_items_scope_key
ON memory_items(scope_key, normalized_key);

CREATE INDEX IF NOT EXISTS idx_review_sessions_repository
ON review_sessions(json_extract(review_target, '$.repository'));

CREATE INDEX IF NOT EXISTS idx_findings_session_updated
ON findings(session_id, updated_at DESC);
