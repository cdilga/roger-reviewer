-- Durable clarification requests (bead rr-surface-parity-epic-rfa2.1).
-- Additive Class A migration: a new clarification_requests table that makes the
-- previously echo-only worker `worker.request_clarification` (and a future
-- operator `rr clarify` path plus the TUI/extension composers) durable and
-- auditable.
--
-- A clarification is a question/follow-up raised against a review session (and
-- optionally a specific run and finding). It is non-mutating: it records that
-- clarification was requested and links it to the finding lineage. An operator
-- (or Roger-owned logic) resolves it, which just flips status to `resolved` and
-- stamps resolved_at/resolution_actor. This is the shared durable entity the
-- worker transport, the operator `rr clarify` path, and the TUI/extension
-- clarification composers all write through one shared review op.

CREATE TABLE IF NOT EXISTS clarification_requests (
    id TEXT PRIMARY KEY,
    review_session_id TEXT NOT NULL,
    review_run_id TEXT,
    finding_id TEXT,
    source TEXT NOT NULL,            -- worker | operator
    body TEXT NOT NULL,
    status TEXT NOT NULL,            -- open | resolved
    created_at INTEGER NOT NULL,
    resolved_at INTEGER,
    resolution_actor TEXT,
    row_version INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_clarification_requests_status
ON clarification_requests(status, created_at);

CREATE INDEX IF NOT EXISTS idx_clarification_requests_session
ON clarification_requests(review_session_id, created_at);

CREATE INDEX IF NOT EXISTS idx_clarification_requests_finding
ON clarification_requests(finding_id, created_at);
