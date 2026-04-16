use roger_app_core::{
    SearchQueryMode, SearchQueryPlanError, SearchQueryPlanningInput, plan_search_query,
};

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
