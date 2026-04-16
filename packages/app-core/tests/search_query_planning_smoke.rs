use roger_app_core::{
    SearchAnchorSet, SearchCandidateVisibility, SearchQueryMode, SearchQueryPlanError,
    SearchQueryPlanningInput, SearchRetrievalLane, SearchScopeSet, SearchSemanticPosture,
    SearchSessionBaseline, SearchTrustFloor, plan_search_query,
};
use serde_json::json;

fn anchor_hints(values: &[&str]) -> Vec<String> {
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
    assert_eq!(first.candidate_visibility, SearchCandidateVisibility::Hidden);
    assert_eq!(
        first.semantic_posture,
        SearchSemanticPosture::DegradedSemanticVisible
    );
    assert_eq!(first.strategy.primary_lane, SearchRetrievalLane::LexicalRecall);
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
    assert_eq!(plan.session_baseline, SearchSessionBaseline::AnchorScopedContext);
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
    assert_eq!(plan.strategy.primary_lane, SearchRetrievalLane::CandidateAudit);
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
