CREATE TABLE IF NOT EXISTS migration_journal (
    id TEXT PRIMARY KEY,
    release_version TEXT NOT NULL,
    schema_from INTEGER NOT NULL,
    schema_to INTEGER NOT NULL,
    migration_class TEXT NOT NULL,
    checkpoint_path TEXT,
    terminal_state TEXT NOT NULL,
    failure_reason TEXT,
    started_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_migration_journal_started_at
ON migration_journal(started_at DESC);

CREATE INDEX IF NOT EXISTS idx_migration_journal_terminal_state
ON migration_journal(terminal_state, updated_at DESC);
