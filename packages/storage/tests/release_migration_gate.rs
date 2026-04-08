use std::fs;
use std::path::{Path, PathBuf};

use roger_app_core::ReviewTarget;
use roger_storage::{Result, RogerStore, SemanticAssetManifest, StorageLayout};
use rusqlite::{Connection, params};
use serde::Deserialize;
use tempfile::tempdir;

#[derive(Debug, Deserialize)]
struct CheckpointManifest {
    release_version: String,
    schema_from: i64,
    schema_to: i64,
    migration_class: String,
    checkpoint_created_at: i64,
    checkpoint_db_path: String,
    sidecar_root_path: String,
    recovery_guidance: Vec<String>,
}

fn sample_target() -> ReviewTarget {
    ReviewTarget {
        repository: "owner/repo".to_owned(),
        pull_request_number: 42,
        base_ref: "main".to_owned(),
        head_ref: "feature".to_owned(),
        base_commit: "deadbeef".to_owned(),
        head_commit: "feedface".to_owned(),
    }
}

fn migration_file_paths() -> Result<Vec<PathBuf>> {
    let mut paths = fs::read_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations"))?
        .map(|entry| entry.map(|item| item.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|path| path.extension().and_then(|ext| ext.to_str()) == Some("sql"));
    paths.sort();
    assert!(
        paths.len() >= 2,
        "release migration gate needs at least two schema migrations"
    );
    Ok(paths)
}

fn count_rows(conn: &Connection, table: &str) -> rusqlite::Result<i64> {
    let sql = format!("SELECT COUNT(*) FROM {table}");
    conn.query_row(&sql, [], |row| row.get(0))
}

fn seed_prior_schema_store(
    root: &Path,
    withheld_migrations: usize,
) -> Result<(StorageLayout, i64, i64)> {
    let layout = StorageLayout::under(root);
    fs::create_dir_all(&layout.root)?;
    fs::create_dir_all(&layout.artifact_root)?;
    fs::create_dir_all(&layout.sidecar_root)?;

    let migration_paths = migration_file_paths()?;
    assert!(
        withheld_migrations >= 1 && withheld_migrations < migration_paths.len(),
        "withheld_migrations must be within 1..len(migrations)"
    );
    let legacy_paths = &migration_paths[..migration_paths.len() - withheld_migrations];
    let legacy_schema_version = legacy_paths.len() as i64;
    let expected_schema_version = migration_paths.len() as i64;

    let conn = Connection::open(&layout.db_path)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    for (index, path) in legacy_paths.iter().enumerate() {
        let sql = fs::read_to_string(path)?;
        conn.execute_batch(&sql)?;
        let version = index as i64 + 1;
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unknown_migration");
        conn.execute(
            "INSERT INTO schema_migrations(version, name, applied_at)
             VALUES (?1, ?2, ?3)",
            params![version, name, 1_710_000_000_i64 + version],
        )?;
        conn.pragma_update(None, "user_version", version)?;
    }

    let review_target_json = serde_json::json!({
        "repository": "owner/repo",
        "pull_request_number": 42,
        "base_ref": "main",
        "head_ref": "feature",
        "base_commit": "deadbeef",
        "head_commit": "feedface",
    })
    .to_string();
    let continuity_state_json = serde_json::json!({
        "status": "awaiting_resume"
    })
    .to_string();

    conn.execute(
        "INSERT INTO local_launch_profiles(
            id, name, repo_root, worktree_strategy, row_version, created_at, updated_at,
            source_surface, ui_target, terminal_environment, multiplexer_mode, reuse_policy
        ) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            "profile-open-pr",
            "Open PR",
            "/tmp/repo",
            "shared-if-clean",
            1_710_000_100_i64,
            "cli",
            "cli",
            "system_default",
            "ntm",
            "reuse_if_possible",
        ],
    )?;

    conn.execute(
        "INSERT INTO review_sessions(
            id, review_target, session_locator, resume_bundle_artifact_id,
            attention_state, row_version, created_at, updated_at,
            provider, continuity_state, launch_profile_id
        ) VALUES (?1, ?2, NULL, NULL, ?3, 0, ?4, ?4, ?5, ?6, ?7)",
        params![
            "session-legacy",
            review_target_json,
            "awaiting_user_input",
            1_710_000_101_i64,
            "opencode",
            continuity_state_json,
            "profile-open-pr",
        ],
    )?;

    conn.execute(
        "INSERT INTO review_runs(
            id, session_id, run_kind, repo_snapshot, created_at,
            continuity_quality, session_locator_artifact_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL)",
        params![
            "run-legacy",
            "session-legacy",
            "explore",
            "git:deadbeef",
            1_710_000_102_i64,
            "degraded",
        ],
    )?;

    conn.execute(
        "INSERT INTO findings(
            id, session_id, first_run_id, fingerprint, title, triage_state, outbound_state,
            row_version, created_at, updated_at, normalized_summary, severity, confidence,
            first_seen_stage, last_seen_run_id, last_seen_stage
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?8, ?9, ?10, ?11, ?12, ?3, ?12)",
        params![
            "finding-legacy",
            "session-legacy",
            "run-legacy",
            "fp:deadbeef",
            "Legacy finding survives migration",
            "new",
            "drafted",
            1_710_000_103_i64,
            "Legacy finding survives migration",
            "medium",
            "high",
            "exploration",
        ],
    )?;

    conn.execute(
        "INSERT INTO outbound_drafts(
            id, session_id, finding_id, target_locator, payload_digest, body,
            row_version, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?7)",
        params![
            "draft-legacy",
            "session-legacy",
            "finding-legacy",
            "github:owner/repo#42/files#thread-1",
            "sha256:payload-legacy",
            "Please double-check the migration gate.",
            1_710_000_104_i64,
        ],
    )?;

    conn.execute(
        "INSERT INTO outbound_approval_tokens(
            id, draft_id, payload_digest, target_locator, approved_at,
            revoked_at, row_version, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 0, ?5, ?5)",
        params![
            "approval-legacy",
            "draft-legacy",
            "sha256:payload-legacy",
            "github:owner/repo#42/files#thread-1",
            1_710_000_105_i64,
        ],
    )?;

    conn.execute(
        "INSERT INTO posted_actions(
            id, draft_id, remote_locator, payload_digest, status, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            "posted-legacy",
            "draft-legacy",
            "github-review-comment-1001",
            "sha256:payload-legacy",
            "posted",
            1_710_000_106_i64,
        ],
    )?;

    conn.execute(
        "INSERT INTO session_launch_bindings(
            id, session_id, surface, launch_profile_id, cwd, worktree_root,
            row_version, created_at, updated_at, repo_locator, review_target, ui_target,
            instance_preference
        ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 0, ?6, ?6, ?7, ?8, ?9, ?10)",
        params![
            "binding-legacy",
            "session-legacy",
            "cli",
            "profile-open-pr",
            "/tmp/repo",
            1_710_000_107_i64,
            "owner/repo",
            serde_json::json!(sample_target()).to_string(),
            "cli",
            "reuse_if_possible",
        ],
    )?;

    conn.execute(
        "INSERT INTO prompt_invocations(
            id, review_session_id, review_run_id, stage, prompt_preset_id, source_surface,
            resolved_text_digest, resolved_text_artifact_id, resolved_text_inline_preview,
            explicit_objective, provider, model, scope_context_json, config_layer_digest,
            launch_intake_id, used_at, row_version, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, ?11, ?12, ?13, NULL, ?14, 0, ?14, ?14)",
        params![
            "prompt-legacy",
            "session-legacy",
            "run-legacy",
            "exploration",
            "default",
            "cli",
            "sha256:prompt",
            "preview",
            "resume review",
            "opencode",
            "gpt-legacy",
            "{\"repository\":\"owner/repo\"}",
            "cfg:legacy",
            1_710_000_108_i64,
        ],
    )?;

    conn.execute(
        "INSERT INTO outcome_events(
            id, event_type, occurred_at, review_session_id, review_run_id,
            prompt_invocation_id, actor_kind, actor_id, source_surface, payload_json, created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?3)",
        params![
            "event-legacy",
            "review_completed",
            1_710_000_109_i64,
            "session-legacy",
            "run-legacy",
            "prompt-legacy",
            "agent",
            "legacy-worker",
            "cli",
            "{\"outcome\":\"complete\"}",
        ],
    )?;

    conn.execute(
        "INSERT INTO index_states(
            scope_key, generation, status, artifact_digest, row_version, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, 0, ?5, ?5)",
        params![
            "repo:owner/repo",
            3_i64,
            "ready",
            "sha256:index-legacy",
            1_710_000_110_i64,
        ],
    )?;

    conn.execute(
        "INSERT INTO memory_items(
            id, scope_key, memory_class, state, statement, normalized_key, anchor_digest,
            source_kind, row_version, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?9)",
        params![
            "memory-legacy",
            "repo:owner/repo",
            "finding_pattern",
            "active",
            "legacy memory survives migration",
            "legacy-memory-survives-migration",
            "sha256:anchor",
            "review",
            1_710_000_111_i64,
        ],
    )?;

    conn.execute(
        "INSERT INTO launch_preflight_plans(
            id, session_id, launch_binding_id, result_class, selected_mode,
            resource_decisions_json, required_operator_actions_json, row_version,
            created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, ?8, ?8)",
        params![
            "preflight-legacy",
            "session-legacy",
            "binding-legacy",
            "ready",
            "current_checkout",
            "{\"store\":\"reuse\"}",
            "[]",
            1_710_000_112_i64,
        ],
    )?;

    Ok((layout, legacy_schema_version, expected_schema_version))
}

fn seed_minimal_schema_store(
    root: &Path,
    applied_migrations: usize,
) -> Result<(StorageLayout, i64, i64)> {
    let layout = StorageLayout::under(root);
    fs::create_dir_all(&layout.root)?;
    fs::create_dir_all(&layout.artifact_root)?;
    fs::create_dir_all(&layout.sidecar_root)?;

    let migration_paths = migration_file_paths()?;
    assert!(
        applied_migrations >= 1 && applied_migrations < migration_paths.len(),
        "applied_migrations must be within 1..len(migrations)-1"
    );
    let expected_schema_version = migration_paths.len() as i64;

    let conn = Connection::open(&layout.db_path)?;
    conn.pragma_update(None, "foreign_keys", "ON")?;

    for (index, path) in migration_paths.iter().take(applied_migrations).enumerate() {
        let sql = fs::read_to_string(path)?;
        conn.execute_batch(&sql)?;
        let version = index as i64 + 1;
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("unknown_migration");
        conn.execute(
            "INSERT INTO schema_migrations(version, name, applied_at)
             VALUES (?1, ?2, ?3)",
            params![version, name, 1_710_100_000_i64 + version],
        )?;
        conn.pragma_update(None, "user_version", version)?;
    }

    Ok((layout, applied_migrations as i64, expected_schema_version))
}

#[test]
fn release_migration_gate_rehearses_prior_schema_store_upgrade() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let (layout, legacy_schema_version, expected_schema_version) =
        seed_prior_schema_store(&root, 1)?;

    let store = RogerStore::open(&root)?;
    assert_eq!(store.schema_version()?, expected_schema_version);
    assert!(
        expected_schema_version > legacy_schema_version,
        "expected migration to advance schema version"
    );

    let session = store
        .review_session("session-legacy")?
        .expect("legacy session preserved");
    assert_eq!(session.review_target, sample_target());
    assert_eq!(session.provider, "opencode");
    assert_eq!(
        session.launch_profile_id.as_deref(),
        Some("profile-open-pr")
    );

    let overview = store.session_overview("session-legacy")?;
    assert_eq!(overview.run_count, 1);
    assert_eq!(overview.finding_count, 1);
    assert_eq!(overview.draft_count, 1);
    assert_eq!(overview.approval_count, 1);
    assert_eq!(overview.posted_action_count, 1);

    let latest_run = store
        .latest_review_run("session-legacy")?
        .expect("latest review run present");
    assert_eq!(latest_run.id, "run-legacy");

    let bindings = store.launch_bindings_for_session("session-legacy")?;
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].id, "binding-legacy");
    assert_eq!(bindings[0].repo_locator, "owner/repo");

    let approval = store
        .approval_for_draft("draft-legacy")?
        .expect("approval record preserved");
    assert_eq!(approval.payload_digest, "sha256:payload-legacy");

    let index_state = store
        .index_state("repo:owner/repo")?
        .expect("index state preserved");
    assert_eq!(index_state.generation, 3);

    drop(store);

    let conn = Connection::open(&layout.db_path)?;
    let applied_versions = count_rows(&conn, "schema_migrations")?;
    assert_eq!(applied_versions, expected_schema_version);
    assert_eq!(count_rows(&conn, "prompt_invocations")?, 1);
    assert_eq!(count_rows(&conn, "outcome_events")?, 1);
    assert_eq!(count_rows(&conn, "memory_items")?, 1);
    assert_eq!(count_rows(&conn, "launch_preflight_plans")?, 1);
    assert_eq!(count_rows(&conn, "migration_journal")?, 1);

    let (
        schema_from,
        schema_to,
        migration_class,
        checkpoint_path,
        terminal_state,
        failure_reason,
        release_version,
    ): (i64, i64, String, String, String, Option<String>, String) = conn.query_row(
        "SELECT schema_from, schema_to, migration_class, checkpoint_path, terminal_state,
                failure_reason, release_version
         FROM migration_journal
         ORDER BY started_at DESC
         LIMIT 1",
        [],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        },
    )?;
    assert_eq!(schema_from, legacy_schema_version);
    assert_eq!(schema_to, expected_schema_version);
    assert_eq!(migration_class, "class_a");
    assert_eq!(terminal_state, "committed");
    assert!(failure_reason.is_none());
    assert!(!release_version.is_empty());

    let checkpoint_dir = layout.root.join(&checkpoint_path);
    assert!(
        checkpoint_dir.is_dir(),
        "checkpoint directory should exist: {}",
        checkpoint_dir.display()
    );
    let checkpoint_db = checkpoint_dir.join("roger.db");
    assert!(
        checkpoint_db.is_file(),
        "checkpoint db should exist: {}",
        checkpoint_db.display()
    );
    let checkpoint_manifest_path = checkpoint_dir.join("checkpoint_manifest.json");
    let manifest: CheckpointManifest =
        serde_json::from_slice(&fs::read(&checkpoint_manifest_path)?)?;
    assert_eq!(manifest.schema_from, legacy_schema_version);
    assert_eq!(manifest.schema_to, expected_schema_version);
    assert_eq!(manifest.migration_class, "class_a");
    assert!(!manifest.release_version.is_empty());
    assert!(manifest.checkpoint_created_at > 0);
    assert!(manifest.checkpoint_db_path.ends_with("roger.db"));
    assert!(manifest.sidecar_root_path.starts_with("sidecars"));
    assert!(
        !manifest.recovery_guidance.is_empty(),
        "checkpoint manifest should include recovery guidance"
    );

    Ok(())
}

#[test]
fn release_migration_gate_records_failed_pre_commit_journal_state() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let (layout, _legacy_schema_version, _expected_schema_version) =
        seed_prior_schema_store(&root, 1)?;

    let conn = Connection::open(&layout.db_path)?;
    conn.execute_batch(
        "CREATE TRIGGER fail_schema_migration_insert
         BEFORE INSERT ON schema_migrations
         BEGIN
            SELECT RAISE(ABORT, 'forced schema migration failure');
         END;",
    )?;
    drop(conn);

    let err = match RogerStore::open(&root) {
        Ok(_) => panic!("migration should fail before commit"),
        Err(err) => err,
    };
    assert!(
        err.to_string().contains("forced schema migration failure"),
        "unexpected migration failure: {err}"
    );

    let conn = Connection::open(&layout.db_path)?;
    let (terminal_state, failure_reason, checkpoint_path): (String, Option<String>, String) = conn
        .query_row(
            "SELECT terminal_state, failure_reason, checkpoint_path
             FROM migration_journal
             ORDER BY started_at DESC
             LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
    assert_eq!(terminal_state, "failed_pre_commit");
    let failure_reason = failure_reason.unwrap_or_default();
    assert!(
        failure_reason.contains("forced schema migration failure"),
        "unexpected failure_reason: {failure_reason}"
    );
    assert!(
        layout.root.join(checkpoint_path).is_dir(),
        "failed migrations should preserve checkpoint directory"
    );

    Ok(())
}

#[test]
fn release_migration_gate_marks_interrupted_attempts_for_recovery() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");

    let store = RogerStore::open(&root)?;
    let layout = store.layout().clone();
    drop(store);

    let conn = Connection::open(&layout.db_path)?;
    conn.execute(
        "INSERT INTO migration_journal(
            id, release_version, schema_from, schema_to, migration_class,
            checkpoint_path, terminal_state, failure_reason, started_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?8)",
        params![
            "migration-interrupted-legacy",
            "0.1.0",
            9_i64,
            10_i64,
            "class_a",
            "backups/interrupted/pre-migration-schema-v9-to-v10",
            "started",
            1_711_000_000_i64,
        ],
    )?;
    drop(conn);

    let _reopened = RogerStore::open(&root)?;

    let conn = Connection::open(&layout.db_path)?;
    let terminal_state: String = conn.query_row(
        "SELECT terminal_state FROM migration_journal WHERE id = ?1",
        params!["migration-interrupted-legacy"],
        |row| row.get(0),
    )?;
    assert_eq!(terminal_state, "needs_operator_recovery");

    Ok(())
}

#[test]
fn release_migration_gate_class_b_invalidates_sidecar_generations_and_manifest() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let (layout, legacy_schema_version, expected_schema_version) =
        seed_prior_schema_store(&root, 2)?;
    assert_eq!(
        expected_schema_version - legacy_schema_version,
        2,
        "test precondition requires Class B migration gap"
    );

    let artifact_rel_path = "legacy/model.bin";
    let artifact_path = layout.semantic_asset_root().join(artifact_rel_path);
    if let Some(parent) = artifact_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&artifact_path, b"legacy-semantic")?;

    let manifest = SemanticAssetManifest {
        schema_version: 1,
        package_id: "legacy-pack".to_owned(),
        revision: "legacy-rev".to_owned(),
        artifact_rel_path: artifact_rel_path.to_owned(),
        artifact_digest: "sha256:legacy-semantic".to_owned(),
        installed_at: 1_712_000_000_i64,
    };
    fs::write(
        layout.semantic_asset_manifest_path(),
        serde_json::to_vec_pretty(&manifest)?,
    )?;

    let store = RogerStore::open(&root)?;
    assert_eq!(store.schema_version()?, expected_schema_version);
    assert_eq!(
        store.semantic_asset_manifest()?,
        None,
        "Class B migration should invalidate semantic sidecar manifest"
    );

    let index_state = store
        .index_state("repo:owner/repo")?
        .expect("index_state should remain present");
    assert_eq!(index_state.generation, 4);
    assert_eq!(index_state.status, "migration_rebuild_required");
    assert_eq!(index_state.artifact_digest, None);
    drop(store);

    let conn = Connection::open(&layout.db_path)?;
    let migration_class: String = conn.query_row(
        "SELECT migration_class FROM migration_journal ORDER BY started_at DESC LIMIT 1",
        [],
        |row| row.get(0),
    )?;
    assert_eq!(migration_class, "class_b");

    Ok(())
}

#[test]
fn release_migration_gate_fails_closed_for_unsupported_class_d_schema_gap() -> Result<()> {
    let temp = tempdir()?;
    let root = temp.path().join("profile");
    let migration_count = migration_file_paths()?.len();
    let applied_migrations = migration_count.saturating_sub(3);
    let (layout, schema_from, expected_schema_version) =
        seed_minimal_schema_store(&root, applied_migrations)?;
    assert!(
        expected_schema_version - schema_from >= 3,
        "test precondition requires Class D migration gap"
    );

    let err = match RogerStore::open(&root) {
        Ok(_) => panic!("unsupported migration class should fail closed"),
        Err(err) => err,
    };
    assert!(
        err.to_string()
            .contains("unsupported automatic migration class class_d"),
        "unexpected unsupported migration error: {err}"
    );

    let conn = Connection::open(&layout.db_path)?;
    let user_version: i64 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    assert_eq!(user_version, schema_from);
    assert_eq!(count_rows(&conn, "migration_journal")?, 0);

    Ok(())
}
