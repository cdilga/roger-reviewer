use std::collections::HashMap;

use tempfile::tempdir;

use roger_app_core::ReviewTarget;
use roger_storage::{
    CreateLaunchPreflightPlan, CreateReviewSession, CreateSessionLaunchBinding,
    LaunchPreflightMode, LaunchPreflightResourceDecisions, LaunchPreflightResultClass,
    LaunchSurface, ResolveLaunchPreflightPlan, Result, RogerStore,
};

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

#[test]
fn latest_preflight_plan_is_loaded_for_binding_context() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;
    let review_target = sample_target();

    store.create_review_session(CreateReviewSession {
        id: "session-1",
        review_target: &review_target,
        provider: "opencode",
        session_locator: None,
        resume_bundle_artifact_id: None,
        continuity_state: "awaiting_resume",
        attention_state: "awaiting_user_input",
        launch_profile_id: None,
    })?;
    store.put_session_launch_binding(CreateSessionLaunchBinding {
        id: "binding-cli",
        session_id: "session-1",
        repo_locator: "owner/repo",
        review_target: Some(&review_target),
        surface: LaunchSurface::Cli,
        launch_profile_id: None,
        ui_target: Some("cli"),
        instance_preference: Some("reuse_if_possible"),
        cwd: Some("/tmp/repo"),
        worktree_root: None,
    })?;

    let initial_decisions = LaunchPreflightResourceDecisions {
        env_config: HashMap::from([(
            "ROGER_PROFILE".to_owned(),
            "named-instance-review-42".to_owned(),
        )]),
        ports: vec!["review_http=4123".to_owned()],
        repo_local_db_paths: vec!["/tmp/repo/.roger/instances/review-42/roger.db".to_owned()],
        container_naming: vec!["roger-review-42".to_owned()],
        caches: vec!["/tmp/repo/.roger/instances/review-42/cache".to_owned()],
        artifacts: vec!["/tmp/repo/.roger/instances/review-42/artifacts".to_owned()],
        logs: vec!["/tmp/repo/.roger/instances/review-42/logs".to_owned()],
    };
    let initial_actions = vec!["create named instance review-42".to_owned()];
    store.put_launch_preflight_plan(CreateLaunchPreflightPlan {
        id: "plan-1",
        session_id: "session-1",
        launch_binding_id: Some("binding-cli"),
        result_class: LaunchPreflightResultClass::ReadyWithActions,
        selected_mode: LaunchPreflightMode::NamedInstance,
        resource_decisions: &initial_decisions,
        required_operator_actions: &initial_actions,
    })?;

    let settled_decisions = LaunchPreflightResourceDecisions {
        env_config: HashMap::from([(
            "ROGER_PROFILE".to_owned(),
            "named-instance-review-42".to_owned(),
        )]),
        ports: vec!["review_http=4123".to_owned(), "review_ws=4321".to_owned()],
        repo_local_db_paths: vec!["/tmp/repo/.roger/instances/review-42/roger.db".to_owned()],
        container_naming: vec!["roger-review-42".to_owned()],
        caches: vec!["/tmp/repo/.roger/instances/review-42/cache".to_owned()],
        artifacts: vec!["/tmp/repo/.roger/instances/review-42/artifacts".to_owned()],
        logs: vec!["/tmp/repo/.roger/instances/review-42/logs".to_owned()],
    };
    let no_actions: Vec<String> = vec![];
    store.put_launch_preflight_plan(CreateLaunchPreflightPlan {
        id: "plan-2",
        session_id: "session-1",
        launch_binding_id: Some("binding-cli"),
        result_class: LaunchPreflightResultClass::Ready,
        selected_mode: LaunchPreflightMode::NamedInstance,
        resource_decisions: &settled_decisions,
        required_operator_actions: &no_actions,
    })?;

    let loaded = store.latest_launch_preflight_plan(ResolveLaunchPreflightPlan {
        session_id: "session-1",
        launch_binding_id: Some("binding-cli"),
    })?;
    let loaded = loaded.expect("latest preflight plan");

    assert_eq!(loaded.id, "plan-2");
    assert_eq!(loaded.result_class, LaunchPreflightResultClass::Ready);
    assert_eq!(loaded.selected_mode, LaunchPreflightMode::NamedInstance);
    assert_eq!(
        loaded
            .resource_decisions
            .env_config
            .get("ROGER_PROFILE")
            .expect("env key"),
        "named-instance-review-42"
    );
    assert_eq!(
        loaded.resource_decisions.ports,
        vec!["review_http=4123".to_owned(), "review_ws=4321".to_owned()]
    );
    assert!(loaded.required_operator_actions.is_empty());

    Ok(())
}

#[test]
fn preflight_lookup_supports_unbound_session_scope_and_binding_filtering() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;
    let review_target = sample_target();

    store.create_review_session(CreateReviewSession {
        id: "session-1",
        review_target: &review_target,
        provider: "opencode",
        session_locator: None,
        resume_bundle_artifact_id: None,
        continuity_state: "awaiting_resume",
        attention_state: "awaiting_user_input",
        launch_profile_id: None,
    })?;

    let unbound_decisions = LaunchPreflightResourceDecisions {
        env_config: HashMap::new(),
        ports: vec![],
        repo_local_db_paths: vec![],
        container_naming: vec![],
        caches: vec![],
        artifacts: vec![],
        logs: vec![],
    };
    let unbound_actions = vec![
        "select a launch profile before extension-driven resume".to_owned(),
        "confirm named instance strategy".to_owned(),
    ];
    store.put_launch_preflight_plan(CreateLaunchPreflightPlan {
        id: "plan-unbound",
        session_id: "session-1",
        launch_binding_id: None,
        result_class: LaunchPreflightResultClass::ProfileRequired,
        selected_mode: LaunchPreflightMode::CurrentCheckout,
        resource_decisions: &unbound_decisions,
        required_operator_actions: &unbound_actions,
    })?;

    let session_scoped = store.latest_launch_preflight_plan(ResolveLaunchPreflightPlan {
        session_id: "session-1",
        launch_binding_id: None,
    })?;
    assert_eq!(
        session_scoped.expect("session scoped plan").id,
        "plan-unbound"
    );

    store.put_session_launch_binding(CreateSessionLaunchBinding {
        id: "binding-bridge",
        session_id: "session-1",
        repo_locator: "owner/repo",
        review_target: Some(&review_target),
        surface: LaunchSurface::Bridge,
        launch_profile_id: None,
        ui_target: Some("tui"),
        instance_preference: Some("always_new"),
        cwd: Some("/tmp/repo"),
        worktree_root: None,
    })?;

    let filtered_before_bound = store.latest_launch_preflight_plan(ResolveLaunchPreflightPlan {
        session_id: "session-1",
        launch_binding_id: Some("binding-bridge"),
    })?;
    assert!(filtered_before_bound.is_none());

    let bound_decisions = LaunchPreflightResourceDecisions {
        env_config: HashMap::from([("ROGER_PROFILE".to_owned(), "worktree-review-42".to_owned())]),
        ports: vec!["review_http=5123".to_owned()],
        repo_local_db_paths: vec!["/tmp/repo/.worktrees/pr-42/.roger/roger.db".to_owned()],
        container_naming: vec!["roger-review-pr42".to_owned()],
        caches: vec!["/tmp/repo/.worktrees/pr-42/.roger/cache".to_owned()],
        artifacts: vec!["/tmp/repo/.worktrees/pr-42/.roger/artifacts".to_owned()],
        logs: vec!["/tmp/repo/.worktrees/pr-42/.roger/logs".to_owned()],
    };
    let bound_actions = vec!["verify worktree checkout at feature".to_owned()];
    store.put_launch_preflight_plan(CreateLaunchPreflightPlan {
        id: "plan-bound",
        session_id: "session-1",
        launch_binding_id: Some("binding-bridge"),
        result_class: LaunchPreflightResultClass::ReadyWithActions,
        selected_mode: LaunchPreflightMode::Worktree,
        resource_decisions: &bound_decisions,
        required_operator_actions: &bound_actions,
    })?;

    let binding_scoped = store.latest_launch_preflight_plan(ResolveLaunchPreflightPlan {
        session_id: "session-1",
        launch_binding_id: Some("binding-bridge"),
    })?;
    let binding_scoped = binding_scoped.expect("binding scoped plan");
    assert_eq!(binding_scoped.id, "plan-bound");
    assert_eq!(binding_scoped.selected_mode, LaunchPreflightMode::Worktree);
    assert_eq!(
        binding_scoped.required_operator_actions,
        vec!["verify worktree checkout at feature".to_owned()]
    );

    let latest_session = store.latest_launch_preflight_plan(ResolveLaunchPreflightPlan {
        session_id: "session-1",
        launch_binding_id: None,
    })?;
    assert_eq!(
        latest_session.expect("latest session plan").id,
        "plan-bound"
    );

    Ok(())
}
