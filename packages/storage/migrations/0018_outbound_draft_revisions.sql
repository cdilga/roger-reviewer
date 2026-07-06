-- Outbound draft body revision lineage (Track 3, bead
-- rr-send-edit-draft-revisions-txnp). Additive Class A migration:
--   * a new outbound_draft_revisions table that records the audit lineage of
--     every draft body, with revision_index 0 reserved for the original
--     generated body and each later revision superseding its predecessor
--   * a nullable revocation_reason_code column on the batch approval token so a
--     body edit can revoke a live approval with a typed, queryable reason
--
-- The canonical outbound_draft_items.body column keeps mirroring the newest
-- revision (updated in the same transaction as the revision insert), so the
-- existing posting/readback path over outbound_draft_items needs no change to
-- observe the latest body. The revisions table is the durable lineage that
-- preserves the original generated body.

CREATE TABLE IF NOT EXISTS outbound_draft_revisions (
    id TEXT PRIMARY KEY,
    draft_id TEXT NOT NULL REFERENCES outbound_draft_items(id) ON DELETE CASCADE,
    revision_index INTEGER NOT NULL,
    body TEXT NOT NULL,
    author_kind TEXT NOT NULL,
    supersedes_revision_id TEXT REFERENCES outbound_draft_revisions(id) ON DELETE SET NULL,
    row_version INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_outbound_draft_revisions_draft_index_unique
ON outbound_draft_revisions(draft_id, revision_index);

CREATE INDEX IF NOT EXISTS idx_outbound_draft_revisions_draft
ON outbound_draft_revisions(draft_id, revision_index DESC);

ALTER TABLE outbound_batch_approval_tokens
ADD COLUMN revocation_reason_code TEXT;
