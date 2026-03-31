ALTER TABLE findings ADD COLUMN normalized_summary TEXT NOT NULL DEFAULT '';
ALTER TABLE findings ADD COLUMN severity TEXT NOT NULL DEFAULT 'medium';
ALTER TABLE findings ADD COLUMN confidence TEXT NOT NULL DEFAULT 'medium';
ALTER TABLE findings ADD COLUMN first_seen_stage TEXT NOT NULL DEFAULT 'unknown';
ALTER TABLE findings ADD COLUMN last_seen_run_id TEXT;
ALTER TABLE findings ADD COLUMN last_seen_stage TEXT;

UPDATE findings
SET normalized_summary = title
WHERE normalized_summary = '';

UPDATE findings
SET last_seen_run_id = first_run_id
WHERE last_seen_run_id IS NULL;

UPDATE findings
SET last_seen_stage = first_seen_stage
WHERE last_seen_stage IS NULL;

CREATE TABLE IF NOT EXISTS code_evidence_locations (
    id TEXT PRIMARY KEY,
    finding_id TEXT NOT NULL REFERENCES findings(id) ON DELETE CASCADE,
    review_session_id TEXT NOT NULL REFERENCES review_sessions(id) ON DELETE CASCADE,
    review_run_id TEXT NOT NULL REFERENCES review_runs(id) ON DELETE CASCADE,
    evidence_role TEXT NOT NULL,
    repo_rel_path TEXT NOT NULL,
    start_line INTEGER NOT NULL,
    end_line INTEGER,
    anchor_state TEXT NOT NULL,
    anchor_digest TEXT,
    excerpt_artifact_id TEXT REFERENCES artifacts(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_code_evidence_locations_finding
ON code_evidence_locations(finding_id);

CREATE INDEX IF NOT EXISTS idx_code_evidence_locations_run
ON code_evidence_locations(review_session_id, review_run_id);
