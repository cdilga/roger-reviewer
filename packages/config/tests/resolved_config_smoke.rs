use roger_config::{
    ResolvedConfigError, cli_defaults, resolve_cli_config_from_lookup,
};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

fn provider_entry<'a>(json: &'a Value, provider: &str) -> &'a Value {
    json["providers"]
        .as_array()
        .expect("providers array")
        .iter()
        .find(|entry| entry["provider"] == provider)
        .expect("provider entry")
}

#[test]
fn resolved_config_serializes_stable_provider_capability_shape() {
    let resolved = resolve_cli_config_from_lookup(Path::new("/tmp/roger"), |_| None);
    let json = serde_json::to_value(&resolved).expect("serialize resolved config");

    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["surface"], "cli");
    assert_eq!(json["store_root"]["value"], "/tmp/roger/.roger");
    assert_eq!(json["store_root"]["provenance"]["layer"], "built_in");
    assert_eq!(json["launch"]["default_provider"]["value"], "opencode");

    let opencode = provider_entry(&json, "opencode");
    assert_eq!(opencode["display_name"], "OpenCode");
    assert_eq!(opencode["status"], "first_class_live");
    assert_eq!(opencode["support_tier"], "tier_b");
    assert_eq!(opencode["surface_class"], "review_primary");
    assert_eq!(
        opencode["capability_provenance"]["source"],
        "providers.opencode.support_matrix"
    );
    assert_eq!(
        opencode["policy_profile"]["continuity_mode"],
        "locator_reopen_and_return"
    );
    assert_eq!(opencode["supports"]["resume_reopen"], true);
    assert_eq!(opencode["supports"]["rr_return"], true);
    assert_eq!(opencode["hook_contract_version"]["value"], "none");
    assert_eq!(
        opencode["instruction_contract_version"]["value"],
        "prompt_preset_contract.v1"
    );

    let copilot = provider_entry(&json, "copilot");
    assert_eq!(copilot["status"], "planned_not_live");
    assert_eq!(copilot["support_tier"], "tier_a_planned");
    assert_eq!(copilot["surface_class"], "admission_pending");
    assert_eq!(
        copilot["fail_closed_reason"],
        "provider_not_live"
    );
    assert_eq!(
        copilot["policy_profile"]["id"],
        "provider_admission_pending"
    );
}

#[test]
fn routine_surface_baseline_serializes_demoted_repair_overrides_and_degraded_reason() {
    let env = HashMap::from([
        (
            cli_defaults::ENV_STORE_ROOT.to_owned(),
            "/tmp/custom-store".to_owned(),
        ),
        (
            cli_defaults::ENV_BRIDGE_HOST_BINARY.to_owned(),
            "/tmp/rr-host".to_owned(),
        ),
    ]);
    let resolved =
        resolve_cli_config_from_lookup(Path::new("/tmp/roger"), |key| env.get(key).cloned());

    let baseline = resolved
        .routine_surface_baseline(Some("codex"))
        .expect("codex routine surface baseline");
    let json = serde_json::to_value(&baseline).expect("serialize routine surface baseline");

    assert_eq!(json["surface"], "cli");
    assert_eq!(json["provider"]["provider"], "codex");
    assert_eq!(json["provider"]["support_tier"], "tier_a");
    assert_eq!(json["provider"]["surface_class"], "review_bounded");
    assert_eq!(
        json["status_reason"],
        "tier_a_reseed_only_no_locator_reopen_or_return"
    );
    assert_eq!(json["repair_overrides_active"], true);
    assert_eq!(
        json["active_repair_override_keys"],
        serde_json::json!([
            cli_defaults::ENV_STORE_ROOT,
            cli_defaults::ENV_BRIDGE_HOST_BINARY
        ])
    );
    assert!(json.get("bridge").is_none(), "baseline should not expose raw bridge config");
    assert!(json.get("providers").is_none(), "baseline should not dump the full provider matrix");
}

#[test]
fn routine_surface_baseline_fails_closed_for_unknown_override_with_stable_error_shape() {
    let resolved = resolve_cli_config_from_lookup(Path::new("/tmp/roger"), |_| None);

    let err = resolved
        .routine_surface_baseline(Some("totally-unknown"))
        .expect_err("unknown provider should fail closed");
    assert_eq!(
        err,
        ResolvedConfigError {
            reason_code: "provider_override_unknown".to_owned(),
            message: "resolved config has no provider capability entry for 'totally-unknown'"
                .to_owned(),
        }
    );

    let json = serde_json::to_value(&err).expect("serialize config error");
    assert_eq!(json["reason_code"], "provider_override_unknown");
}
