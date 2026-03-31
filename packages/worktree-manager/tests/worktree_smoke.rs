//! Integration smoke tests for worktree manager using stub ops.

use std::path::{Path, PathBuf};

use roger_worktree_manager::{
    CreateWorktreeRequest, InstanceRegistry, InstanceStatus, LaunchRoutingOutcome,
    LaunchRoutingQuery, NamedInstance, PreflightInput, StubWorktreeOps, WorktreeError,
    WorktreeNeed, WorktreeOps, classify_worktree_need, resolve_launch_routing,
    resolve_resource_plan,
};

#[test]
fn preflight_required_when_dirty_and_different_branch() {
    let input = PreflightInput {
        repo_root: PathBuf::from("/repo"),
        current_branch: "main".to_owned(),
        target_head_ref: "feat/review-target".to_owned(),
        working_tree_dirty: true,
        existing_worktrees: vec![],
    };
    let result = classify_worktree_need(&input);
    assert!(
        matches!(
            &result,
            WorktreeNeed::Required { reason }
                if reason.contains("dirty") && reason.contains("feat/review-target")
        ),
        "expected Required, got {result:?}"
    );
}

#[test]
fn preflight_not_needed_same_branch_clean() {
    let input = PreflightInput {
        repo_root: PathBuf::from("/repo"),
        current_branch: "feat/x".to_owned(),
        target_head_ref: "feat/x".to_owned(),
        working_tree_dirty: false,
        existing_worktrees: vec![],
    };
    assert_eq!(classify_worktree_need(&input), WorktreeNeed::NotNeeded);
}

#[test]
fn resource_plan_isolated_paths() {
    let plan = resolve_resource_plan(
        "pr-42",
        Path::new("/data/roger"),
        Path::new("/tmp/wt"),
        "myrepo",
    );
    assert_eq!(plan.instance_name, "pr-42");
    assert!(plan.db_path.starts_with("/data/roger/instances/pr-42"));
    assert!(plan.worktree_path.starts_with("/tmp/wt/myrepo-pr-42"));
    assert!(plan.artifact_dir.starts_with("/data/roger/instances/pr-42"));
    assert!(plan.log_dir.starts_with("/data/roger/instances/pr-42"));
}

#[test]
fn stub_worktree_create_and_identity() {
    let ops = StubWorktreeOps::new();
    let req = CreateWorktreeRequest {
        repo_root: PathBuf::from("/repo"),
        target_path: PathBuf::from("/tmp/wt-test"),
        branch: "feat/review".to_owned(),
        commit: Some("deadbeef12345678".to_owned()),
        instance_name: Some("review-7".to_owned()),
    };
    let wt = ops.create_worktree(&req).unwrap();
    assert_eq!(wt.id, "wt-deadbeef");
    assert_eq!(wt.branch, "feat/review");
    assert_eq!(wt.instance_name.as_deref(), Some("review-7"));
}

#[test]
fn instance_same_pr_ambiguity_fails_closed() {
    let mut reg = InstanceRegistry::new();
    for name in ["inst-1", "inst-2"] {
        reg.register(NamedInstance {
            name: name.to_owned(),
            review_target_pr: Some(42),
            review_target_repo: Some("acme/repo".to_owned()),
            worktree_id: None,
            resource_plan: None,
            created_at: 1000,
            updated_at: 1000,
            status: InstanceStatus::Active,
        })
        .unwrap();
    }

    let result = reg.resolve_for_pr(42);
    assert!(
        matches!(
            &result,
            Err(WorktreeError::SamePrAmbiguity { pr, instances })
                if *pr == 42 && instances.len() == 2
        ),
        "expected SamePrAmbiguity, got {result:?}"
    );
}

#[test]
fn retarget_invalidates_when_pr_changes() {
    let mut reg = InstanceRegistry::new();
    reg.register(NamedInstance {
        name: "inst-a".to_owned(),
        review_target_pr: Some(10),
        review_target_repo: None,
        worktree_id: None,
        resource_plan: None,
        created_at: 1000,
        updated_at: 1000,
        status: InstanceStatus::Active,
    })
    .unwrap();

    let result = reg
        .retarget("inst-a", 20, Some("acme/new".to_owned()))
        .unwrap();
    assert!(result.approvals_invalidated);
    assert_eq!(result.old_pr, Some(10));
    assert_eq!(result.new_pr, 20);

    let inst = reg.get("inst-a").unwrap();
    assert_eq!(inst.review_target_pr, Some(20));
    assert_eq!(inst.review_target_repo.as_deref(), Some("acme/new"));
}

#[test]
fn tear_down_removes_from_active_list() {
    let mut reg = InstanceRegistry::new();
    reg.register(NamedInstance {
        name: "inst-a".to_owned(),
        review_target_pr: None,
        review_target_repo: None,
        worktree_id: None,
        resource_plan: None,
        created_at: 1000,
        updated_at: 1000,
        status: InstanceStatus::Active,
    })
    .unwrap();

    assert_eq!(reg.list_active().len(), 1);
    reg.tear_down("inst-a").unwrap();
    assert!(reg.list_active().is_empty());
    assert_eq!(reg.get("inst-a").unwrap().status, InstanceStatus::TornDown);
}

// Launch routing integration tests

fn make_query(pr: u64) -> LaunchRoutingQuery {
    LaunchRoutingQuery {
        pr_number: pr,
        repo: "acme/repo".to_owned(),
        explicit_instance: None,
        source_surface: "cli".to_owned(),
        allow_stale_recovery: false,
    }
}

fn make_inst(name: &str, pr: u64, status: InstanceStatus) -> NamedInstance {
    NamedInstance {
        name: name.to_owned(),
        review_target_pr: Some(pr),
        review_target_repo: Some("acme/repo".to_owned()),
        worktree_id: None,
        resource_plan: None,
        created_at: 1000,
        updated_at: 1000,
        status,
    }
}

#[test]
fn launch_routing_creates_new_when_empty() {
    let reg = InstanceRegistry::new();
    let result = resolve_launch_routing(&reg, &make_query(42)).unwrap();
    assert_eq!(result, LaunchRoutingOutcome::CreateNew);
}

#[test]
fn launch_routing_resolves_single_match() {
    let mut reg = InstanceRegistry::new();
    reg.register(make_inst("r1", 42, InstanceStatus::Active))
        .unwrap();
    let result = resolve_launch_routing(&reg, &make_query(42)).unwrap();
    assert_eq!(
        result,
        LaunchRoutingOutcome::Resolved {
            instance_name: "r1".to_owned()
        }
    );
}

#[test]
fn launch_routing_ambiguous_fails_closed() {
    let mut reg = InstanceRegistry::new();
    reg.register(make_inst("r1", 42, InstanceStatus::Active))
        .unwrap();
    reg.register(make_inst("r2", 42, InstanceStatus::Active))
        .unwrap();

    let result = resolve_launch_routing(&reg, &make_query(42)).unwrap();
    assert!(
        matches!(
            &result,
            LaunchRoutingOutcome::Ambiguous { candidates, guidance }
                if candidates.len() == 2 && guidance.contains("--instance")
        ),
        "expected Ambiguous, got {result:?}"
    );
}

#[test]
fn launch_routing_explicit_instance_overrides() {
    let mut reg = InstanceRegistry::new();
    reg.register(make_inst("r1", 42, InstanceStatus::Active))
        .unwrap();
    reg.register(make_inst("r2", 42, InstanceStatus::Active))
        .unwrap();

    let mut query = make_query(42);
    query.explicit_instance = Some("r2".to_owned());
    let result = resolve_launch_routing(&reg, &query).unwrap();
    assert_eq!(
        result,
        LaunchRoutingOutcome::Resolved {
            instance_name: "r2".to_owned()
        }
    );
}

#[test]
fn launch_routing_recovers_idle_instance() {
    let mut reg = InstanceRegistry::new();
    reg.register(make_inst("idle-1", 42, InstanceStatus::Idle))
        .unwrap();

    let mut query = make_query(42);
    query.allow_stale_recovery = true;
    let result = resolve_launch_routing(&reg, &query).unwrap();
    assert_eq!(
        result,
        LaunchRoutingOutcome::RecoveredStale {
            instance_name: "idle-1".to_owned()
        }
    );
}
