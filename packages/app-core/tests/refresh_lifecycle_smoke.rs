use roger_app_core::{
    DraftRefreshSignalKind, ExistingFindingSnapshot, FindingOutboundState, RefreshLifecycleState,
    RefreshedFindingSnapshot, classify_refresh_lifecycle,
};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
struct RefreshIdentityFixture {
    existing: Vec<ExistingFindingSnapshot>,
    refreshed: Vec<RefreshedFindingSnapshot>,
    expected_unmatched_new_fingerprints: Vec<String>,
}

fn load_refresh_identity_fixture() -> RefreshIdentityFixture {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../tests/fixtures/fixture_refresh_rebase_target_drift/refresh_identity_case.json",
    );
    let raw = fs::read_to_string(&path).expect("failed to read refresh identity fixture");
    serde_json::from_str(&raw).expect("failed to decode refresh identity fixture")
}

#[test]
fn refresh_classification_covers_all_lifecycle_transitions_and_signals() {
    let fixture = load_refresh_identity_fixture();

    let result = classify_refresh_lifecycle(&fixture.existing, &fixture.refreshed);
    assert_eq!(result.transitions.len(), 5);
    assert_eq!(
        result.unmatched_new_fingerprints,
        fixture.expected_unmatched_new_fingerprints
    );

    let carried = result
        .transitions
        .iter()
        .find(|transition| transition.finding_id == "finding-carried")
        .expect("carried transition exists");
    assert_eq!(carried.next_state, RefreshLifecycleState::CarriedForward);
    assert!(carried.replacement_fingerprint.is_none());
    assert_eq!(
        carried.draft_signal.as_ref().map(|signal| signal.kind),
        Some(DraftRefreshSignalKind::Reconfirm)
    );

    let invalid_anchor = result
        .transitions
        .iter()
        .find(|transition| transition.finding_id == "finding-invalid-anchor")
        .expect("invalid-anchor transition exists");
    assert_eq!(
        invalid_anchor.next_state,
        RefreshLifecycleState::InvalidAnchor
    );
    assert_eq!(
        invalid_anchor.replacement_fingerprint.as_deref(),
        Some("fp-invalid-anchor")
    );
    assert_eq!(
        invalid_anchor
            .draft_signal
            .as_ref()
            .map(|signal| signal.kind),
        Some(DraftRefreshSignalKind::Invalidate)
    );

    let superseded = result
        .transitions
        .iter()
        .find(|transition| transition.finding_id == "finding-superseded")
        .expect("superseded transition exists");
    assert_eq!(superseded.next_state, RefreshLifecycleState::Superseded);
    assert_eq!(
        superseded.replacement_fingerprint.as_deref(),
        Some("fp-new")
    );
    assert_eq!(
        superseded.draft_signal.as_ref().map(|signal| signal.kind),
        Some(DraftRefreshSignalKind::Invalidate)
    );
    assert!(
        superseded
            .draft_signal
            .as_ref()
            .expect("superseded draft signal")
            .reason_code
            .contains("fp-new")
    );

    let resolved = result
        .transitions
        .iter()
        .find(|transition| transition.finding_id == "finding-resolved")
        .expect("resolved transition exists");
    assert_eq!(resolved.next_state, RefreshLifecycleState::Resolved);
    assert!(resolved.replacement_fingerprint.is_none());
    assert_eq!(
        resolved.draft_signal.as_ref().map(|signal| signal.kind),
        Some(DraftRefreshSignalKind::Invalidate)
    );

    let stale = result
        .transitions
        .iter()
        .find(|transition| transition.finding_id == "finding-stale")
        .expect("stale transition exists");
    assert_eq!(stale.next_state, RefreshLifecycleState::Stale);
    assert!(stale.replacement_fingerprint.is_none());
    assert_eq!(
        stale.draft_signal.as_ref().map(|signal| signal.kind),
        Some(DraftRefreshSignalKind::Invalidate)
    );
}

#[test]
fn carried_forward_not_drafted_finding_emits_no_signal() {
    let existing = vec![ExistingFindingSnapshot {
        finding_id: "finding-new".to_owned(),
        fingerprint: "fp-1".to_owned(),
        primary_anchor_digest: Some("anchor-a".to_owned()),
        outbound_state: FindingOutboundState::NotDrafted,
    }];
    let refreshed = vec![RefreshedFindingSnapshot {
        fingerprint: "fp-1".to_owned(),
        primary_anchor_digest: Some("anchor-a".to_owned()),
    }];

    let result = classify_refresh_lifecycle(&existing, &refreshed);
    let transition = result.transitions.first().expect("one transition");
    assert_eq!(transition.next_state, RefreshLifecycleState::CarriedForward);
    assert!(transition.draft_signal.is_none());
    assert!(result.unmatched_new_fingerprints.is_empty());
}
