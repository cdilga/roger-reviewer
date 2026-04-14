use std::env;
use std::fs;
use std::time::Instant;

use tempfile::tempdir;

use roger_app_core::ReviewTarget;
use roger_storage::{
    CreateMaterializedFinding, CreateReviewRun, CreateReviewSession, PriorReviewLookupQuery,
    PriorReviewRetrievalMode, Result, RogerStore, SemanticAssetManifest, SemanticLookupCandidate,
    SemanticLookupTargetKind, UpdateIndexState, UpsertMemoryItem,
};

const REPOSITORY: &str = "owner/repo";
const SCOPE_KEY: &str = "repo:owner/repo";
const QUERY_TEXT: &str = "refresh signal";

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

fn seed_perf_corpus(store: &RogerStore) -> Result<Vec<SemanticLookupCandidate>> {
    install_verified_semantic_assets(store)?;

    let mut semantic_candidates = Vec::new();
    for pr in 1..=16_u64 {
        let session_id = format!("session-{pr}");
        let run_id = format!("run-{pr}");
        seed_session(store, &session_id, &run_id, REPOSITORY, pr)?;

        for finding_idx in 0..40_u64 {
            let finding_id = format!("finding-{pr}-{finding_idx}");
            let lexical_match = finding_idx < 12;
            let semantic_only = (12..20).contains(&finding_idx);
            let title = if lexical_match {
                format!("Refresh signal drift {pr}-{finding_idx}")
            } else {
                format!("Anchor mismatch {pr}-{finding_idx}")
            };
            let normalized_summary = if lexical_match {
                format!("refresh signal reconfirmation path {pr}-{finding_idx}")
            } else if semantic_only {
                format!("semantic-only reconfirmation target {pr}-{finding_idx}")
            } else {
                format!("non matching summary {pr}-{finding_idx}")
            };

            store.upsert_materialized_finding(CreateMaterializedFinding {
                id: &finding_id,
                session_id: &session_id,
                review_run_id: &run_id,
                stage: "deep_review",
                fingerprint: &format!("fp:{pr}:{finding_idx}"),
                title: &title,
                normalized_summary: &normalized_summary,
                severity: "medium",
                confidence: "medium",
                triage_state: "new",
                outbound_state: "not-drafted",
            })?;

            if finding_idx < 10 || semantic_only {
                semantic_candidates.push(SemanticLookupCandidate {
                    target_kind: SemanticLookupTargetKind::EvidenceFinding,
                    target_id: finding_id,
                    score: if lexical_match { 0.91 } else { 0.84 },
                });
            }
        }
    }

    for idx in 0..240_u64 {
        let state = match idx % 3 {
            0 => "proven",
            1 => "established",
            _ => "candidate",
        };
        let lexical_match = idx < 96;
        let semantic_only = (96..144).contains(&idx);
        let memory_id = format!("mem-{idx}");
        let statement = if lexical_match {
            format!("Refresh signal handling note {idx}")
        } else if semantic_only {
            format!("Semantic-only stale anchor reminder {idx}")
        } else {
            format!("Background note {idx}")
        };
        let normalized_key = if lexical_match {
            format!("refresh signal note {idx}")
        } else if semantic_only {
            format!("stale anchor note {idx}")
        } else {
            format!("background note {idx}")
        };

        store.upsert_memory_item(UpsertMemoryItem {
            id: &memory_id,
            scope_key: SCOPE_KEY,
            memory_class: if state == "candidate" {
                "semantic"
            } else {
                "procedural"
            },
            state,
            statement: &statement,
            normalized_key: &normalized_key,
            anchor_digest: Some("anchor:refresh"),
            source_kind: "derived",
        })?;

        if idx < 80 || semantic_only {
            semantic_candidates.push(SemanticLookupCandidate {
                target_kind: SemanticLookupTargetKind::MemoryItem,
                target_id: memory_id,
                score: if lexical_match { 0.89 } else { 0.82 },
            });
        }
    }

    store.upsert_index_state(UpdateIndexState {
        scope_key: &format!("lexical:{SCOPE_KEY}"),
        generation: 11,
        status: "ready",
        artifact_digest: Some("sha256:lexical-perf-v11"),
    })?;
    store.upsert_index_state(UpdateIndexState {
        scope_key: &format!("semantic:{SCOPE_KEY}"),
        generation: 12,
        status: "ready",
        artifact_digest: Some("sha256:semantic-perf-v12"),
    })?;

    Ok(semantic_candidates)
}

fn percentile(sorted: &[u128], percentile: f64) -> u128 {
    let clamped = percentile.clamp(0.0, 1.0);
    let index = ((sorted.len() - 1) as f64 * clamped).round() as usize;
    sorted[index]
}

fn env_or_default(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[test]
#[ignore = "manual performance harness for extreme optimisation passes"]
fn prior_review_lookup_perf_hybrid_hot_path_reports_percentiles() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path().join("profile"))?;
    let semantic_candidates = seed_perf_corpus(&store)?;

    let warmup_iterations = env_or_default("ROGER_STORAGE_PERF_WARMUP", 25);
    let measured_iterations = env_or_default("ROGER_STORAGE_PERF_ITERATIONS", 250);

    for _ in 0..warmup_iterations {
        let result = store.prior_review_lookup(PriorReviewLookupQuery {
            scope_key: SCOPE_KEY,
            repository: REPOSITORY,
            query_text: QUERY_TEXT,
            limit: 100,
            include_tentative_candidates: true,
            allow_project_scope: false,
            allow_org_scope: false,
            semantic_assets_verified: true,
            semantic_candidates: semantic_candidates.clone(),
        })?;
        assert_eq!(result.mode, PriorReviewRetrievalMode::Hybrid);
    }

    let mut timings_us = Vec::with_capacity(measured_iterations);
    let mut last_result_sizes = (0_usize, 0_usize, 0_usize);
    for _ in 0..measured_iterations {
        let started = Instant::now();
        let result = store.prior_review_lookup(PriorReviewLookupQuery {
            scope_key: SCOPE_KEY,
            repository: REPOSITORY,
            query_text: QUERY_TEXT,
            limit: 100,
            include_tentative_candidates: true,
            allow_project_scope: false,
            allow_org_scope: false,
            semantic_assets_verified: true,
            semantic_candidates: semantic_candidates.clone(),
        })?;
        let elapsed = started.elapsed();
        assert_eq!(result.mode, PriorReviewRetrievalMode::Hybrid);
        last_result_sizes = (
            result.evidence_hits.len(),
            result.promoted_memory.len(),
            result.tentative_candidates.len(),
        );
        timings_us.push(elapsed.as_micros());
    }

    timings_us.sort_unstable();
    let total_us: u128 = timings_us.iter().sum();
    let mean_us = total_us as f64 / timings_us.len() as f64;

    println!(
        "prior_review_lookup_perf iterations={} warmup={} p50_us={} p95_us={} p99_us={} mean_us={:.1} evidence_hits={} promoted_memory={} tentative_candidates={} semantic_candidates={}",
        measured_iterations,
        warmup_iterations,
        percentile(&timings_us, 0.50),
        percentile(&timings_us, 0.95),
        percentile(&timings_us, 0.99),
        mean_us,
        last_result_sizes.0,
        last_result_sizes.1,
        last_result_sizes.2,
        semantic_candidates.len(),
    );

    Ok(())
}
