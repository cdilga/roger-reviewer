use tempfile::tempdir;

use roger_storage::{
    CreateLaunchProfile, LaunchProfileRouteResolution, LaunchSurface, ResolveLaunchProfileRoute,
    Result, RogerStore,
};

#[test]
fn cli_and_extension_launches_share_daemonless_routing_contract() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    store.put_launch_profile(CreateLaunchProfile {
        id: "profile-cli",
        name: "CLI profile",
        source_surface: LaunchSurface::Cli,
        ui_target: "tui",
        terminal_environment: "vscode_integrated_terminal",
        multiplexer_mode: "ntm",
        reuse_policy: "reuse_if_possible",
        repo_root: "/tmp/repo",
        worktree_strategy: "shared-if-clean",
    })?;
    store.put_launch_profile(CreateLaunchProfile {
        id: "profile-extension",
        name: "Extension profile",
        source_surface: LaunchSurface::Extension,
        ui_target: "tui",
        terminal_environment: "wezterm_split",
        multiplexer_mode: "wezterm_split",
        reuse_policy: "always_new",
        repo_root: "/tmp/repo",
        worktree_strategy: "shared-if-clean",
    })?;

    let cli_resolution = store.resolve_launch_profile_route(ResolveLaunchProfileRoute {
        source_surface: LaunchSurface::Cli,
        requested_profile_id: Some("profile-cli".to_owned()),
        fallback_profile_id: None,
        available_terminal_environments: vec!["vscode_integrated_terminal".to_owned()],
        available_multiplexer_modes: vec!["ntm".to_owned(), "none".to_owned()],
    })?;
    assert!(
        matches!(
            &cli_resolution,
            LaunchProfileRouteResolution::Resolved(decision)
                if !decision.degraded
                    && decision.terminal_environment == "vscode_integrated_terminal"
                    && decision.multiplexer_mode == "ntm"
                    && decision.profile_id == "profile-cli"
        ),
        "expected resolved CLI route, got {cli_resolution:?}"
    );

    let extension_resolution = store.resolve_launch_profile_route(ResolveLaunchProfileRoute {
        source_surface: LaunchSurface::Extension,
        requested_profile_id: Some("profile-extension".to_owned()),
        fallback_profile_id: None,
        available_terminal_environments: vec!["wezterm_window".to_owned()],
        available_multiplexer_modes: vec!["none".to_owned()],
    })?;
    assert!(
        matches!(
            &extension_resolution,
            LaunchProfileRouteResolution::Resolved(decision)
                if decision.degraded
                    && decision.terminal_environment == "wezterm_window"
                    && decision.multiplexer_mode == "none"
                    && decision
                        .reason
                        .as_deref()
                        .expect("degraded reason")
                        .contains("unavailable")
        ),
        "expected resolved Extension route, got {extension_resolution:?}"
    );

    Ok(())
}

#[test]
fn routing_reports_not_found_when_no_profile_matches_request() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    let resolution = store.resolve_launch_profile_route(ResolveLaunchProfileRoute {
        source_surface: LaunchSurface::Cli,
        requested_profile_id: Some("missing-profile".to_owned()),
        fallback_profile_id: None,
        available_terminal_environments: vec!["vscode_integrated_terminal".to_owned()],
        available_multiplexer_modes: vec!["none".to_owned()],
    })?;

    assert!(
        matches!(
            &resolution,
            LaunchProfileRouteResolution::NotFound { reason }
                if reason.contains("no matching launch profile")
        ),
        "expected not-found route result, got {resolution:?}"
    );

    Ok(())
}

#[test]
fn routing_can_default_to_latest_profile_for_source_surface() -> Result<()> {
    let temp = tempdir()?;
    let store = RogerStore::open(temp.path())?;

    store.put_launch_profile(CreateLaunchProfile {
        id: "profile-1",
        name: "CLI profile 1",
        source_surface: LaunchSurface::Cli,
        ui_target: "cli",
        terminal_environment: "system_default",
        multiplexer_mode: "none",
        reuse_policy: "reuse_if_possible",
        repo_root: "/tmp/repo",
        worktree_strategy: "shared-if-clean",
    })?;
    store.put_launch_profile(CreateLaunchProfile {
        id: "profile-2",
        name: "CLI profile 2",
        source_surface: LaunchSurface::Cli,
        ui_target: "tui",
        terminal_environment: "wezterm_window",
        multiplexer_mode: "none",
        reuse_policy: "always_new",
        repo_root: "/tmp/repo",
        worktree_strategy: "shared-if-clean",
    })?;

    let resolution = store.resolve_launch_profile_route(ResolveLaunchProfileRoute {
        source_surface: LaunchSurface::Cli,
        requested_profile_id: None,
        fallback_profile_id: None,
        available_terminal_environments: vec!["wezterm_window".to_owned()],
        available_multiplexer_modes: vec!["none".to_owned()],
    })?;

    assert!(
        matches!(
            &resolution,
            LaunchProfileRouteResolution::Resolved(decision)
                if decision.profile_id == "profile-2"
                    && decision.ui_target == "tui"
                    && !decision.degraded
        ),
        "expected resolved route, got {resolution:?}"
    );

    Ok(())
}
