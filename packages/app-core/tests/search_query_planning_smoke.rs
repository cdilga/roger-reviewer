use roger_app_core::{
    SearchAnchorSet, SearchCandidateVisibility, SearchPlanError, SearchPlanInput, SearchQueryMode,
    SearchQueryPlanError, SearchQueryPlanningInput, SearchRetrievalClass, SearchRetrievalLane,
    SearchScopeSet, SearchSemanticPosture, SearchSemanticRuntimePosture, SearchSessionBaseline,
    SearchTrustFloor, materialize_search_plan, plan_search_query,
};
use serde_json::json;

fn anchor_hints(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn granted_scopes(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn auto_resolves_related_context_when_anchor_hints_are_present() {
    let plan = plan_search_query(SearchQueryPlanningInput {
        query_text: "approval invalidation",
        query_mode: Some("auto"),
        anchor_hints: &anchor_hints(&["finding-1"]),
        supports_candidate_audit: true,
        supports_promotion_review: false,
    })
    .expect("auto should resolve with anchors");

    assert_eq!(plan.requested_query_mode, SearchQueryMode::Auto);
    assert_eq!(plan.resolved_query_mode, SearchQueryMode::RelatedContext);
}

#[test]
fn auto_resolves_exact_lookup_for_locator_like_queries() {
    let plan = plan_search_query(SearchQueryPlanningInput {
        query_text: "packages/cli/src/lib.rs",
        query_mode: None,
        anchor_hints: &[],
        supports_candidate_audit: true,
        supports_promotion_review: false,
    })
    .expect("auto should resolve exact lookup");

    assert_eq!(plan.requested_query_mode, SearchQueryMode::Auto);
    assert_eq!(plan.resolved_query_mode, SearchQueryMode::ExactLookup);
}

#[test]
fn auto_resolves_recall_for_free_text_queries() {
    let plan = plan_search_query(SearchQueryPlanningInput {
        query_text: "stale draft invalidation",
        query_mode: Some("auto"),
        anchor_hints: &[],
        supports_candidate_audit: true,
        supports_promotion_review: false,
    })
    .expect("auto should resolve recall");

    assert_eq!(plan.requested_query_mode, SearchQueryMode::Auto);
    assert_eq!(plan.resolved_query_mode, SearchQueryMode::Recall);
}

#[test]
fn related_context_fails_closed_without_anchor_hints() {
    let err = plan_search_query(SearchQueryPlanningInput {
        query_text: "stale draft invalidation",
        query_mode: Some("related_context"),
        anchor_hints: &[],
        supports_candidate_audit: true,
        supports_promotion_review: false,
    })
    .expect_err("related_context should require anchors");

    assert_eq!(err, SearchQueryPlanError::RelatedContextRequiresAnchors);
}

#[test]
fn promotion_review_fails_closed_when_surface_does_not_support_it() {
    let err = plan_search_query(SearchQueryPlanningInput {
        query_text: "approval invalidation",
        query_mode: Some("promotion_review"),
        anchor_hints: &anchor_hints(&["finding-1"]),
        supports_candidate_audit: true,
        supports_promotion_review: false,
    })
    .expect_err("promotion review should fail closed when unsupported");

    assert_eq!(err, SearchQueryPlanError::PromotionReviewUnsupported);
}

#[test]
fn recall_plan_is_inspectable_and_deterministic() {
    let first = plan_search_query(SearchQueryPlanningInput {
        query_text: "stale draft invalidation",
        query_mode: Some("recall"),
        anchor_hints: &[],
        supports_candidate_audit: true,
        supports_promotion_review: false,
    })
    .expect("recall should plan");
    let second = plan_search_query(SearchQueryPlanningInput {
        query_text: "stale draft invalidation",
        query_mode: Some("recall"),
        anchor_hints: &[],
        supports_candidate_audit: true,
        supports_promotion_review: false,
    })
    .expect("recall should plan twice");

    assert_eq!(first, second);
    assert_eq!(first.scope_set, SearchScopeSet::CurrentRepository);
    assert_eq!(
        first.session_baseline,
        SearchSessionBaseline::AmbientSessionOptional
    );
    assert_eq!(first.anchor_set, SearchAnchorSet::None);
    assert_eq!(first.trust_floor, SearchTrustFloor::PromotedAndEvidenceOnly);
    assert_eq!(
        first.candidate_visibility,
        SearchCandidateVisibility::Hidden
    );
    assert_eq!(
        first.semantic_posture,
        SearchSemanticPosture::DegradedSemanticVisible
    );
    assert_eq!(
        first.strategy.primary_lane,
        SearchRetrievalLane::LexicalRecall
    );
    assert!(first.strategy.lexical);
    assert!(first.strategy.prior_review);
    assert!(first.strategy.semantic);
    assert!(!first.strategy.candidate_audit);

    let encoded = serde_json::to_value(first).expect("serialize recall plan");
    assert_eq!(
        encoded,
        json!({
            "requested_query_mode": "recall",
            "resolved_query_mode": "recall",
            "scope_set": "current_repository",
            "session_baseline": "ambient_session_optional",
            "anchor_set": "none",
            "trust_floor": "promoted_and_evidence_only",
            "candidate_visibility": "hidden",
            "semantic_posture": "degraded_semantic_visible",
            "strategy": {
                "primary_lane": "lexical_recall",
                "lexical": true,
                "prior_review": true,
                "semantic": true,
                "candidate_audit": false,
                "query_expansion": false
            }
        })
    );
}

#[test]
fn candidate_audit_plan_stays_anchor_scoped_and_candidate_only() {
    let anchors = anchor_hints(&["finding-1"]);
    let plan = plan_search_query(SearchQueryPlanningInput {
        query_text: "stale draft invalidation",
        query_mode: Some("candidate_audit"),
        anchor_hints: &anchors,
        supports_candidate_audit: true,
        supports_promotion_review: false,
    })
    .expect("candidate audit should plan");

    assert_eq!(plan.requested_query_mode, SearchQueryMode::CandidateAudit);
    assert_eq!(plan.resolved_query_mode, SearchQueryMode::CandidateAudit);
    assert_eq!(plan.scope_set, SearchScopeSet::CurrentRepository);
    assert_eq!(
        plan.session_baseline,
        SearchSessionBaseline::AnchorScopedContext
    );
    assert_eq!(plan.anchor_set, SearchAnchorSet::ExplicitHints);
    assert_eq!(
        plan.trust_floor,
        SearchTrustFloor::CandidateInspectionAllowed
    );
    assert_eq!(
        plan.candidate_visibility,
        SearchCandidateVisibility::CandidateAuditOnly
    );
    assert_eq!(plan.semantic_posture, SearchSemanticPosture::LexicalOnly);
    assert_eq!(
        plan.strategy.primary_lane,
        SearchRetrievalLane::CandidateAudit
    );
    assert!(plan.strategy.lexical);
    assert!(plan.strategy.prior_review);
    assert!(plan.strategy.candidate_audit);
    assert!(!plan.strategy.semantic);
    assert!(plan.includes_tentative_candidates());
    assert!(
        plan.strategy_reason()
            .contains("tentative candidates without widening ordinary recall")
    );
}

#[test]
fn search_plan_materializes_repo_scope_runtime_and_strategy_truth() {
    let scopes = granted_scopes(&["repo"]);
    let anchors = anchor_hints(&["finding-1"]);
    let input = SearchPlanInput {
        review_session_id: Some("session-1"),
        review_run_id: Some("run-1"),
        repository: "owner/repo",
        granted_scopes: &scopes,
        query_text: "approval invalidation",
        query_mode: Some("auto"),
        requested_retrieval_classes: &[],
        anchor_hints: &anchors,
        supports_candidate_audit: true,
        supports_promotion_review: false,
        semantic_assets_verified: false,
    };

    let first = materialize_search_plan(input.clone()).expect("search plan should materialize");
    let second =
        materialize_search_plan(input).expect("search plan should materialize deterministically");

    assert_eq!(first, second);
    assert_eq!(
        first.query_plan.resolved_query_mode,
        SearchQueryMode::RelatedContext
    );
    assert_eq!(first.review_session_id.as_deref(), Some("session-1"));
    assert_eq!(first.review_run_id.as_deref(), Some("run-1"));
    assert_eq!(first.granted_scopes, vec!["repo".to_owned()]);
    assert_eq!(first.scope_keys, vec!["repo:owner/repo".to_owned()]);
    assert_eq!(
        first.retrieval_classes,
        vec![
            SearchRetrievalClass::PromotedMemory,
            SearchRetrievalClass::EvidenceHits,
        ]
    );
    assert_eq!(
        first.semantic_runtime_posture,
        SearchSemanticRuntimePosture::DisabledPendingVerification
    );
    assert!(!first.retrieval_strategy.semantic);
    assert!(first.retrieval_strategy.lexical);
    assert!(first.retrieval_strategy.prior_review);
    assert!(
        first
            .strategy_reason
            .contains("Scope stays bound to repo:owner/repo"),
        "{}",
        first.strategy_reason
    );
    assert!(
        first.strategy_reason.contains(
            "semantic retrieval is disabled until verified local semantic assets are available"
        ),
        "{}",
        first.strategy_reason
    );

    let encoded = serde_json::to_value(&first).expect("serialize search plan");
    assert_eq!(
        encoded,
        json!({
            "query_plan": {
                "requested_query_mode": "auto",
                "resolved_query_mode": "related_context",
                "scope_set": "current_repository",
                "session_baseline": "anchor_scoped_context",
                "anchor_set": "explicit_hints",
                "trust_floor": "promoted_and_evidence_only",
                "candidate_visibility": "hidden",
                "semantic_posture": "degraded_semantic_visible",
                "strategy": {
                    "primary_lane": "related_context",
                    "lexical": true,
                    "prior_review": true,
                    "semantic": true,
                    "candidate_audit": false,
                    "query_expansion": false
                }
            },
            "review_session_id": "session-1",
            "review_run_id": "run-1",
            "granted_scopes": ["repo"],
            "scope_keys": ["repo:owner/repo"],
            "retrieval_classes": ["promoted_memory", "evidence_hits"],
            "semantic_runtime_posture": "disabled_pending_verification",
            "retrieval_strategy": {
                "primary_lane": "related_context",
                "lexical": true,
                "prior_review": true,
                "semantic": false,
                "candidate_audit": false,
                "query_expansion": false
            },
            "strategy_reason": first.strategy_reason
        })
    );
}

#[test]
fn candidate_aware_search_plan_requires_tentative_candidates() {
    let scopes = granted_scopes(&["repo"]);
    let err = materialize_search_plan(SearchPlanInput {
        review_session_id: Some("session-1"),
        review_run_id: Some("run-1"),
        repository: "owner/repo",
        granted_scopes: &scopes,
        query_text: "approval invalidation",
        query_mode: Some("candidate_audit"),
        requested_retrieval_classes: &["promoted_memory".to_owned(), "evidence_hits".to_owned()],
        anchor_hints: &anchor_hints(&["finding-1"]),
        supports_candidate_audit: true,
        supports_promotion_review: false,
        semantic_assets_verified: false,
    })
    .expect_err("candidate-aware mode should not silently hide tentative candidates");

    assert_eq!(
        err,
        SearchPlanError::CandidateAwareQueryRequiresTentativeCandidates {
            query_mode: "candidate_audit".to_owned(),
        }
    );
}

#[test]
fn tentative_candidates_require_candidate_aware_query_mode() {
    let scopes = granted_scopes(&["repo"]);
    let err = materialize_search_plan(SearchPlanInput {
        review_session_id: None,
        review_run_id: None,
        repository: "owner/repo",
        granted_scopes: &scopes,
        query_text: "approval invalidation",
        query_mode: Some("recall"),
        requested_retrieval_classes: &["tentative_candidates".to_owned()],
        anchor_hints: &[],
        supports_candidate_audit: true,
        supports_promotion_review: false,
        semantic_assets_verified: false,
    })
    .expect_err("ordinary recall should not surface tentative candidates");

    assert_eq!(
        err,
        SearchPlanError::TentativeCandidatesRequireCandidateAwareQuery {
            query_mode: "recall".to_owned(),
        }
    );
}
