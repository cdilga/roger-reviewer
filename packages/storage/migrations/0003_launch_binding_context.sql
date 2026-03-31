ALTER TABLE session_launch_bindings ADD COLUMN repo_locator TEXT NOT NULL DEFAULT '';
ALTER TABLE session_launch_bindings ADD COLUMN review_target TEXT;
ALTER TABLE session_launch_bindings ADD COLUMN ui_target TEXT;
ALTER TABLE session_launch_bindings ADD COLUMN instance_preference TEXT;

CREATE INDEX IF NOT EXISTS idx_session_launch_bindings_surface_repo
    ON session_launch_bindings(surface, repo_locator);
