ALTER TABLE prompt_invocations
ADD COLUMN review_task_id TEXT;

ALTER TABLE prompt_invocations
ADD COLUMN worker_invocation_id TEXT;

ALTER TABLE prompt_invocations
ADD COLUMN turn_index INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_prompt_invocations_task
ON prompt_invocations(review_task_id, used_at DESC);

CREATE INDEX IF NOT EXISTS idx_prompt_invocations_worker
ON prompt_invocations(worker_invocation_id, used_at DESC);
