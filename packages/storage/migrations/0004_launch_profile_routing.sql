ALTER TABLE local_launch_profiles
ADD COLUMN source_surface TEXT NOT NULL DEFAULT 'cli';

ALTER TABLE local_launch_profiles
ADD COLUMN ui_target TEXT NOT NULL DEFAULT 'cli';

ALTER TABLE local_launch_profiles
ADD COLUMN terminal_environment TEXT NOT NULL DEFAULT 'system_default';

ALTER TABLE local_launch_profiles
ADD COLUMN multiplexer_mode TEXT NOT NULL DEFAULT 'none';

ALTER TABLE local_launch_profiles
ADD COLUMN reuse_policy TEXT NOT NULL DEFAULT 'reuse_if_possible';
