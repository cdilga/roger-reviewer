PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS schema_migrations (
    version INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    applied_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS review_sessions (
    id TEXT PRIMARY KEY,
    review_target TEXT NOT NULL,
    session_locator TEXT,
    resume_bundle_artifact_id TEXT,
    attention_state TEXT NOT NULL,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS review_runs (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    run_kind TEXT NOT NULL,
    repo_snapshot TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_review_runs_session ON review_runs(session_id);

CREATE TABLE IF NOT EXISTS findings (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    first_run_id TEXT NOT NULL REFERENCES review_runs(id) ON DELETE CASCADE,
    fingerprint TEXT NOT NULL,
    title TEXT NOT NULL,
    triage_state TEXT NOT NULL,
    outbound_state TEXT NOT NULL,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_findings_session ON findings(session_id);
CREATE INDEX IF NOT EXISTS idx_findings_fingerprint ON findings(fingerprint);

CREATE TABLE IF NOT EXISTS finding_decision_events (
    id TEXT PRIMARY KEY,
    finding_id TEXT NOT NULL REFERENCES findings(id) ON DELETE CASCADE,
    triage_state TEXT NOT NULL,
    outbound_state TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS outbound_drafts (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    finding_id TEXT NOT NULL REFERENCES findings(id) ON DELETE CASCADE,
    target_locator TEXT NOT NULL,
    payload_digest TEXT NOT NULL,
    body TEXT NOT NULL,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_outbound_drafts_session ON outbound_drafts(session_id);

CREATE TABLE IF NOT EXISTS outbound_approval_tokens (
    id TEXT PRIMARY KEY,
    draft_id TEXT NOT NULL REFERENCES outbound_drafts(id) ON DELETE CASCADE,
    payload_digest TEXT NOT NULL,
    target_locator TEXT NOT NULL,
    approved_at INTEGER NOT NULL,
    revoked_at INTEGER,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_outbound_approvals_draft ON outbound_approval_tokens(draft_id);

CREATE TABLE IF NOT EXISTS posted_actions (
    id TEXT PRIMARY KEY,
    draft_id TEXT NOT NULL REFERENCES outbound_drafts(id) ON DELETE CASCADE,
    remote_locator TEXT NOT NULL,
    payload_digest TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_posted_actions_draft ON posted_actions(draft_id);

CREATE TABLE IF NOT EXISTS local_launch_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    repo_root TEXT NOT NULL,
    worktree_strategy TEXT NOT NULL,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS index_states (
    scope_key TEXT PRIMARY KEY,
    generation INTEGER NOT NULL,
    status TEXT NOT NULL,
    artifact_digest TEXT,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT PRIMARY KEY,
    digest TEXT NOT NULL UNIQUE,
    budget_class TEXT NOT NULL,
    storage_kind TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    inline_bytes BLOB,
    relative_path TEXT,
    created_at INTEGER NOT NULL
);
