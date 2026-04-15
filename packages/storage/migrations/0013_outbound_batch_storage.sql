CREATE TABLE IF NOT EXISTS outbound_draft_batches (
    id TEXT PRIMARY KEY,
    review_session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    review_run_id TEXT NOT NULL REFERENCES review_runs(id) ON DELETE CASCADE,
    repo_id TEXT NOT NULL,
    remote_review_target_id TEXT NOT NULL,
    payload_digest TEXT NOT NULL,
    approval_state TEXT NOT NULL,
    approved_at INTEGER,
    invalidated_at INTEGER,
    invalidation_reason_code TEXT,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_outbound_draft_batches_session
ON outbound_draft_batches(review_session_id);

CREATE INDEX IF NOT EXISTS idx_outbound_draft_batches_run
ON outbound_draft_batches(review_run_id);

CREATE TABLE IF NOT EXISTS outbound_draft_items (
    id TEXT PRIMARY KEY,
    review_session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    review_run_id TEXT NOT NULL REFERENCES review_runs(id) ON DELETE CASCADE,
    finding_id TEXT REFERENCES findings(id) ON DELETE CASCADE,
    draft_batch_id TEXT NOT NULL REFERENCES outbound_draft_batches(id) ON DELETE CASCADE,
    repo_id TEXT NOT NULL,
    remote_review_target_id TEXT NOT NULL,
    payload_digest TEXT NOT NULL,
    approval_state TEXT NOT NULL,
    anchor_digest TEXT NOT NULL,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_outbound_draft_items_session
ON outbound_draft_items(review_session_id);

CREATE INDEX IF NOT EXISTS idx_outbound_draft_items_batch
ON outbound_draft_items(draft_batch_id);

CREATE INDEX IF NOT EXISTS idx_outbound_draft_items_finding
ON outbound_draft_items(finding_id);

CREATE TABLE IF NOT EXISTS outbound_batch_approval_tokens (
    id TEXT PRIMARY KEY,
    draft_batch_id TEXT NOT NULL REFERENCES outbound_draft_batches(id) ON DELETE CASCADE,
    payload_digest TEXT NOT NULL,
    target_tuple_json TEXT NOT NULL,
    approved_at INTEGER NOT NULL,
    revoked_at INTEGER,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_outbound_batch_approvals_batch_unique
ON outbound_batch_approval_tokens(draft_batch_id);

CREATE TABLE IF NOT EXISTS posted_batch_actions (
    id TEXT PRIMARY KEY,
    draft_batch_id TEXT NOT NULL REFERENCES outbound_draft_batches(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    remote_identifier TEXT NOT NULL,
    status TEXT NOT NULL,
    posted_payload_digest TEXT NOT NULL,
    posted_at INTEGER NOT NULL,
    failure_code TEXT
);

CREATE INDEX IF NOT EXISTS idx_posted_batch_actions_batch
ON posted_batch_actions(draft_batch_id);
