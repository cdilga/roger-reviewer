use roger_app_core::{
    LaunchRoutingRequest, LocalLaunchProfile, MultiplexerMode, ReusePolicy, Surface,
    TerminalEnvironment, UiTarget, resolve_launch_routing,
};

fn sample_profile(source_surface: Surface) -> LocalLaunchProfile {
    LocalLaunchProfile {
        id: "profile-local".to_owned(),
        name: "Local launch".to_owned(),
        source_surface,
        ui_target: UiTarget::Tui,
        terminal_environment: TerminalEnvironment::VscodeIntegratedTerminal,
        multiplexer_mode: MultiplexerMode::Ntm,
        reuse_policy: ReusePolicy::ReuseIfPossible,
        repo_root: "/tmp/repo".to_owned(),
        worktree_root: None,
        created_at: 100,
        updated_at: 100,
        row_version: 0,
    }
}

#[test]
fn cli_and_extension_use_same_launch_routing_contract() {
    let cli_decision = resolve_launch_routing(LaunchRoutingRequest {
        source_surface: Surface::Cli,
        profile: sample_profile(Surface::Cli),
        available_terminal_environments: vec![TerminalEnvironment::VscodeIntegratedTerminal],
        available_multiplexer_modes: vec![MultiplexerMode::Ntm, MultiplexerMode::None],
    });
    assert!(!cli_decision.degraded);
    assert_eq!(
        cli_decision.terminal_environment,
        TerminalEnvironment::VscodeIntegratedTerminal
    );
    assert_eq!(cli_decision.multiplexer_mode, MultiplexerMode::Ntm);

    let extension_decision = resolve_launch_routing(LaunchRoutingRequest {
        source_surface: Surface::Extension,
        profile: sample_profile(Surface::Extension),
        available_terminal_environments: vec![TerminalEnvironment::WeztermWindow],
        available_multiplexer_modes: vec![MultiplexerMode::None],
    });
    assert!(extension_decision.degraded);
    assert_eq!(
        extension_decision.terminal_environment,
        TerminalEnvironment::WeztermWindow
    );
    assert_eq!(extension_decision.multiplexer_mode, MultiplexerMode::None);
    assert!(
        extension_decision
            .reason
            .as_deref()
            .expect("fallback reason")
            .contains("unavailable")
    );
}

#[test]
fn routing_can_keep_profile_settings_when_no_availability_probe_exists() {
    let decision = resolve_launch_routing(LaunchRoutingRequest {
        source_surface: Surface::Cli,
        profile: sample_profile(Surface::Cli),
        available_terminal_environments: Vec::new(),
        available_multiplexer_modes: Vec::new(),
    });

    assert!(!decision.degraded);
    assert_eq!(
        decision.terminal_environment,
        TerminalEnvironment::VscodeIntegratedTerminal
    );
    assert_eq!(decision.multiplexer_mode, MultiplexerMode::Ntm);
    assert!(decision.reason.is_none());
}
