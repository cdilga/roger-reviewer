use std::fs;

use tempfile::tempdir;

use roger_app_core::ReviewTarget;
use roger_storage::{
    CreateMaterializedFinding, CreateReviewRun, CreateReviewSession, PriorReviewLookupQuery,
    PriorReviewRetrievalMode, Result, RogerStore, SemanticAssetManifest, SemanticLookupCandidate,
    SemanticLookupTargetKind, UpdateIndexState, UpsertMemoryItem,
};

fn target(repository: &str, pull_request_number: u64) -> ReviewTarget {
    ReviewTarget {
        repository: repository.to_owned(),
        pull_request_number,
        base_ref: "main".to_owned(),
        head_ref: "feature".to_owned(),
        base_commit: "1111111".to_owned(),
        head_commit: "2222222".to_owned(),
    }
}

fn seed_session(
    store: &RogerStore,
    session_id: &str,
    run_id: &str,
    repository: &str,
    pull_request_number: u64,
) -> Result<()> {
    store.create_review_session(CreateReviewSession {
        id: session_id,
        review_target: &target(repository, pull_request_number),
        provider: "opencode",
        session_locator: None,
        resume_bundle_artifact_id: None,
        continuity_state: "usable",
        attention_state: "awaiting_user_input",
        launch_profile_id: None,
    })?;

    store.create_review_run(CreateReviewRun {
        id: run_id,
        session_id,
        run_kind: "deep_review",
        repo_snapshot: "git:2222222",
        continuity_quality: "usable",
        session_locator_artifact_id: None,
    })?;

    Ok(())
}

fn install_verified_semantic_assets(store: &RogerStore) -> Result<()> {
    let artifact_rel_path = "fastembed/model.bin";
    let payload = b"semantic-v1";
    let absolute = store.layout().semantic_asset_root().join(artifact_rel_path);
    if let Some(parent) = absolute.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&absolute, payload)?;
    store.install_semantic_asset_manifest(&SemanticAssetManifest {
        schema_version: 1,
        package_id: "fastembed-mini".to_owned(),
        revision: "2026-03-31".to_owned(),
        artifact_rel_path: artifact_rel_path.to_owned(),
        artifact_digest: "sha256:0d05f729f928b76c15e31e5097fb25f1f11909706e64d9c582607e5d227166c3"
            .to_owned(),
        installed_at: 1_743_380_000,
    })?;
    Ok(())
}

#[test]
fn prior_review_lookup_is_repo_first_and_truthfully_degrades_without_semantic_assets() -> Result<()>
{
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;

    seed_session(&store, "session-owner", "run-owner", "owner/repo", 42)?;
    seed_session(&store, "session-other", "run-other", "other/repo", 9)?;

    store.upsert_materialized_finding(CreateMaterializedFinding {
        id: "finding-owner",
        session_id: "session-owner",
        review_run_id: "run-owner",
        stage: "deep_review",
        fingerprint: "fp:approval-invalidation",
        title: "Approval invalidation can silently drift",
        normalized_summary: "approval invalidation path can drift after refresh",
        severity: "high",
        confidence: "high",
        triage_state: "new",
        outbound_state: "not-drafted",
    })?;
    store.upsert_materialized_finding(CreateMaterializedFinding {
        id: "finding-other",
        session_id: "session-other",
        review_run_id: "run-other",
        stage: "deep_review",
        fingerprint: "fp:approval-invalidation-other",
        title: "Approval invalidation in unrelated repo",
        normalized_summary: "unrelated repository finding",
        severity: "medium",
        confidence: "medium",
        triage_state: "new",
        outbound_state: "not-drafted",
    })?;

    store.upsert_memory_item(UpsertMemoryItem {
        id: "mem-proven",
        scope_key: "repo:owner/repo",
        memory_class: "procedural",
        state: "proven",
        statement: "Always reconfirm approved drafts after refresh invalidation signals",
        normalized_key: "reconfirm approved drafts after refresh",
        anchor_digest: Some("anchor:refresh"),
        source_kind: "manual",
    })?;
    store.upsert_memory_item(UpsertMemoryItem {
        id: "mem-candidate",
        scope_key: "repo:owner/repo",
        memory_class: "semantic",
        state: "candidate",
        statement: "Cache stale anchors when commit drift is detected",
        normalized_key: "cache stale anchors for drift",
        anchor_digest: Some("anchor:drift"),
        source_kind: "derived",
    })?;

    store.upsert_index_state(UpdateIndexState {
        scope_key: "lexical:repo:owner/repo",
        generation: 1,
        status: "ready",
        artifact_digest: Some("sha256:lexical"),
    })?;

    let result = store.prior_review_lookup(PriorReviewLookupQuery {
        scope_key: "repo:owner/repo",
        repository: "owner/repo",
        query_text: "refresh",
        limit: 10,
        include_tentative_candidates: false,
        allow_project_scope: false,
        allow_org_scope: false,
        semantic_assets_verified: false,
        semantic_candidates: Vec::new(),
    })?;

    assert_eq!(result.mode, PriorReviewRetrievalMode::LexicalOnly);
    assert_eq!(result.scope_bucket, "repo_memory");
    assert!(
        result
            .degraded_reasons
            .iter()
            .any(|reason| reason.contains("semantic assets"))
    );

    let evidence_ids = result
        .evidence_hits
        .iter()
        .map(|hit| hit.finding_id.as_str())
        .collect::<Vec<_>>();
    assert!(evidence_ids.contains(&"finding-owner"));
    assert!(!evidence_ids.contains(&"finding-other"));

    assert_eq!(result.promoted_memory.len(), 1);
    assert_eq!(result.promoted_memory[0].memory_id, "mem-proven");
    assert!(result.tentative_candidates.is_empty());

    Ok(())
}

#[test]
fn prior_review_lookup_uses_recovery_scan_when_lexical_sidecar_is_unavailable() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;

    seed_session(&store, "session-owner", "run-owner", "owner/repo", 42)?;

    store.upsert_materialized_finding(CreateMaterializedFinding {
        id: "finding-owner",
        session_id: "session-owner",
        review_run_id: "run-owner",
        stage: "deep_review",
        fingerprint: "fp:refresh-lifecycle",
        title: "Refresh lifecycle can invalidate prior approvals",
        normalized_summary: "refresh lifecycle reconfirmation should gate posting",
        severity: "high",
        confidence: "high",
        triage_state: "accepted",
        outbound_state: "drafted",
    })?;

    let result = store.prior_review_lookup(PriorReviewLookupQuery {
        scope_key: "repo:owner/repo",
        repository: "owner/repo",
        query_text: "refresh",
        limit: 10,
        include_tentative_candidates: false,
        allow_project_scope: false,
        allow_org_scope: false,
        semantic_assets_verified: false,
        semantic_candidates: Vec::new(),
    })?;

    assert_eq!(result.mode, PriorReviewRetrievalMode::RecoveryScan);
    assert!(
        result
            .degraded_reasons
            .iter()
            .any(|reason| reason.contains("lexical sidecar unavailable or stale"))
    );
    assert!(
        result
            .degraded_reasons
            .iter()
            .any(|reason| reason.contains("semantic assets"))
    );
    assert_eq!(result.evidence_hits.len(), 1);
    assert_eq!(result.evidence_hits[0].finding_id, "finding-owner");

    Ok(())
}

#[test]
fn prior_review_lookup_fuses_semantic_candidates_when_assets_and_sidecars_are_ready() -> Result<()>
{
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    install_verified_semantic_assets(&store)?;

    seed_session(&store, "session-owner", "run-owner", "owner/repo", 42)?;

    store.upsert_materialized_finding(CreateMaterializedFinding {
        id: "finding-owner",
        session_id: "session-owner",
        review_run_id: "run-owner",
        stage: "deep_review",
        fingerprint: "fp:refresh-lifecycle",
        title: "Refresh lifecycle can invalidate prior approvals",
        normalized_summary: "refresh lifecycle reconfirmation should gate posting",
        severity: "high",
        confidence: "high",
        triage_state: "accepted",
        outbound_state: "drafted",
    })?;

    store.upsert_memory_item(UpsertMemoryItem {
        id: "mem-proven",
        scope_key: "repo:owner/repo",
        memory_class: "procedural",
        state: "proven",
        statement: "Reconfirm approved drafts whenever refresh lifecycle emits reconfirm signal",
        normalized_key: "reconfirm approved drafts on refresh signal",
        anchor_digest: Some("anchor:refresh"),
        source_kind: "manual",
    })?;
    store.upsert_memory_item(UpsertMemoryItem {
        id: "mem-candidate",
        scope_key: "repo:owner/repo",
        memory_class: "semantic",
        state: "candidate",
        statement: "Queue stale findings for triage when anchors shift",
        normalized_key: "queue stale findings for triage",
        anchor_digest: Some("anchor:stale"),
        source_kind: "derived",
    })?;

    store.upsert_index_state(UpdateIndexState {
        scope_key: "lexical:repo:owner/repo",
        generation: 3,
        status: "ready",
        artifact_digest: Some("sha256:lexical-v3"),
    })?;
    store.upsert_index_state(UpdateIndexState {
        scope_key: "semantic:repo:owner/repo",
        generation: 5,
        status: "ready",
        artifact_digest: Some("sha256:semantic-v5"),
    })?;

    let result = store.prior_review_lookup(PriorReviewLookupQuery {
        scope_key: "repo:owner/repo",
        repository: "owner/repo",
        query_text: "refresh signal stale findings",
        limit: 10,
        include_tentative_candidates: true,
        allow_project_scope: false,
        allow_org_scope: false,
        semantic_assets_verified: true,
        semantic_candidates: vec![
            SemanticLookupCandidate {
                target_kind: SemanticLookupTargetKind::EvidenceFinding,
                target_id: "finding-owner".to_owned(),
                score: 0.91,
            },
            SemanticLookupCandidate {
                target_kind: SemanticLookupTargetKind::MemoryItem,
                target_id: "mem-candidate".to_owned(),
                score: 0.88,
            },
        ],
    })?;

    assert_eq!(result.mode, PriorReviewRetrievalMode::Hybrid);

    let evidence = result
        .evidence_hits
        .iter()
        .find(|hit| hit.finding_id == "finding-owner")
        .expect("finding-owner evidence hit");
    assert_eq!(evidence.semantic_score_milli, 910);
    assert!(evidence.fused_score > evidence.lexical_score);

    let tentative = result
        .tentative_candidates
        .iter()
        .find(|item| item.memory_id == "mem-candidate")
        .expect("candidate memory hit");
    assert_eq!(tentative.semantic_score_milli, 880);
    assert!(tentative.fused_score > tentative.lexical_score);

    assert_eq!(result.promoted_memory.len(), 1);
    assert_eq!(result.promoted_memory[0].memory_id, "mem-proven");

    Ok(())
}

#[test]
fn prior_review_lookup_blocks_project_overlay_when_not_enabled() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;

    let result = store.prior_review_lookup(PriorReviewLookupQuery {
        scope_key: "project:shared",
        repository: "owner/repo",
        query_text: "refresh",
        limit: 5,
        include_tentative_candidates: false,
        allow_project_scope: false,
        allow_org_scope: false,
        semantic_assets_verified: true,
        semantic_candidates: vec![SemanticLookupCandidate {
            target_kind: SemanticLookupTargetKind::EvidenceFinding,
            target_id: "finding-owner".to_owned(),
            score: 0.42,
        }],
    })?;

    assert!(result.evidence_hits.is_empty());
    assert!(result.promoted_memory.is_empty());
    assert!(
        result
            .degraded_reasons
            .iter()
            .any(|reason| reason.contains("project scope requested"))
    );

    Ok(())
}
