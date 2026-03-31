CREATE TABLE IF NOT EXISTS schema_migrations (
    version TEXT PRIMARY KEY,
    applied_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT PRIMARY KEY,
    digest TEXT NOT NULL UNIQUE,
    kind TEXT NOT NULL,
    media_type TEXT NOT NULL,
    storage_class TEXT NOT NULL,
    byte_size INTEGER NOT NULL,
    inline_bytes BLOB,
    file_rel_path TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS local_launch_profiles (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    source_surface TEXT NOT NULL,
    repo_root TEXT NOT NULL,
    worktree_root TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    row_version INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS review_sessions (
    id TEXT PRIMARY KEY,
    review_target_json TEXT NOT NULL,
    provider TEXT NOT NULL,
    continuity_state TEXT NOT NULL,
    resume_bundle_artifact_id TEXT,
    launch_profile_id TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    row_version INTEGER NOT NULL,
    FOREIGN KEY(resume_bundle_artifact_id) REFERENCES artifacts(id),
    FOREIGN KEY(launch_profile_id) REFERENCES local_launch_profiles(id)
);

CREATE TABLE IF NOT EXISTS review_runs (
    id TEXT PRIMARY KEY,
    review_session_id TEXT NOT NULL,
    stage TEXT NOT NULL,
    continuity_quality TEXT NOT NULL,
    session_locator_artifact_id TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY(review_session_id) REFERENCES review_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY(session_locator_artifact_id) REFERENCES artifacts(id)
);

CREATE TABLE IF NOT EXISTS findings (
    id TEXT PRIMARY KEY,
    review_session_id TEXT NOT NULL,
    review_run_id TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    title TEXT NOT NULL,
    normalized_summary TEXT NOT NULL,
    severity TEXT NOT NULL,
    confidence TEXT NOT NULL,
    triage_state TEXT NOT NULL,
    outbound_state TEXT NOT NULL,
    first_seen_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    row_version INTEGER NOT NULL,
    FOREIGN KEY(review_session_id) REFERENCES review_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY(review_run_id) REFERENCES review_runs(id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_findings_session_fingerprint
ON findings(review_session_id, fingerprint);

CREATE TABLE IF NOT EXISTS outbound_draft_batches (
    id TEXT PRIMARY KEY,
    review_session_id TEXT NOT NULL,
    review_run_id TEXT NOT NULL,
    repo_id TEXT NOT NULL,
    remote_review_target_id TEXT NOT NULL,
    payload_digest TEXT NOT NULL,
    approval_state TEXT NOT NULL,
    approved_at INTEGER,
    invalidated_at INTEGER,
    invalidation_reason_code TEXT,
    row_version INTEGER NOT NULL,
    FOREIGN KEY(review_session_id) REFERENCES review_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY(review_run_id) REFERENCES review_runs(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS outbound_drafts (
    id TEXT PRIMARY KEY,
    review_session_id TEXT NOT NULL,
    review_run_id TEXT NOT NULL,
    finding_id TEXT,
    draft_batch_id TEXT NOT NULL,
    repo_id TEXT NOT NULL,
    remote_review_target_id TEXT NOT NULL,
    payload_digest TEXT NOT NULL,
    approval_state TEXT NOT NULL,
    anchor_digest TEXT NOT NULL,
    row_version INTEGER NOT NULL,
    FOREIGN KEY(review_session_id) REFERENCES review_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY(review_run_id) REFERENCES review_runs(id) ON DELETE CASCADE,
    FOREIGN KEY(finding_id) REFERENCES findings(id),
    FOREIGN KEY(draft_batch_id) REFERENCES outbound_draft_batches(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS outbound_approval_tokens (
    id TEXT PRIMARY KEY,
    draft_batch_id TEXT NOT NULL,
    payload_digest TEXT NOT NULL,
    target_tuple_json TEXT NOT NULL,
    approved_at INTEGER NOT NULL,
    revoked_at INTEGER,
    FOREIGN KEY(draft_batch_id) REFERENCES outbound_draft_batches(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS posted_actions (
    id TEXT PRIMARY KEY,
    draft_batch_id TEXT NOT NULL,
    provider TEXT NOT NULL,
    remote_identifier TEXT NOT NULL,
    status TEXT NOT NULL,
    posted_payload_digest TEXT NOT NULL,
    posted_at INTEGER NOT NULL,
    failure_code TEXT,
    FOREIGN KEY(draft_batch_id) REFERENCES outbound_draft_batches(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS index_state (
    index_name TEXT PRIMARY KEY,
    generation INTEGER NOT NULL,
    status TEXT NOT NULL,
    last_built_at INTEGER,
    artifact_id TEXT,
    row_version INTEGER NOT NULL,
    FOREIGN KEY(artifact_id) REFERENCES artifacts(id)
);
