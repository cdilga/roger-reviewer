CREATE TABLE IF NOT EXISTS posted_action_items (
    id TEXT PRIMARY KEY,
    posted_action_id TEXT NOT NULL REFERENCES posted_batch_actions(id) ON DELETE CASCADE,
    draft_id TEXT NOT NULL REFERENCES outbound_draft_items(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    remote_identifier TEXT,
    failure_code TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_posted_action_items_action_draft_unique
ON posted_action_items(posted_action_id, draft_id);

CREATE INDEX IF NOT EXISTS idx_posted_action_items_draft
ON posted_action_items(draft_id);
