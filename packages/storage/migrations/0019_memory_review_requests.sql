-- Durable memory review requests (bead rr-memory-reality-slice-o0go).
-- Additive Class A migration: a new memory_review_requests table that makes the
-- previously write-only worker memory-review proposals durable and auditable,
-- plus the first production write path into memory_items when a request is
-- accepted.
--
-- A request is non-mutating until an operator (or Roger-owned review logic)
-- resolves it: acceptance materializes/updates a memory_items row (dedup by
-- normalized_key within scope), rejection just resolves the request. The
-- resulting_memory_item_id column links an accepted request to the memory item
-- it materialized so provenance stays inspectable.

CREATE TABLE IF NOT EXISTS memory_review_requests (
    id TEXT PRIMARY KEY,
    review_session_id TEXT NOT NULL,
    review_run_id TEXT,
    source TEXT NOT NULL,             -- worker | operator
    request_kind TEXT NOT NULL,       -- promote | demote | deprecate | restore | mark_anti_pattern
    statement TEXT NOT NULL,
    normalized_key TEXT NOT NULL,
    scope_key TEXT NOT NULL,
    memory_class TEXT NOT NULL,       -- working | episodic | semantic | procedural
    rationale TEXT,
    status TEXT NOT NULL,             -- pending_review | accepted | rejected
    created_at INTEGER NOT NULL,
    resolved_at INTEGER,
    resolution_actor TEXT,
    resulting_memory_item_id TEXT REFERENCES memory_items(id) ON DELETE SET NULL,
    row_version INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_memory_review_requests_status
ON memory_review_requests(status, created_at);

CREATE INDEX IF NOT EXISTS idx_memory_review_requests_session
ON memory_review_requests(review_session_id, created_at);

CREATE INDEX IF NOT EXISTS idx_memory_review_requests_scope_key
ON memory_review_requests(scope_key, normalized_key);

-- Dedup lookup support for the promotion write path: materialization dedups a
-- memory item by (scope_key, normalized_key) before deciding insert vs update.
CREATE INDEX IF NOT EXISTS idx_memory_items_scope_normalized_key
ON memory_items(scope_key, normalized_key);
