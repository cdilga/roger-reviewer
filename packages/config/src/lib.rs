use serde::{Deserialize, Serialize};
use std::path::Path;

pub mod cli_defaults {
    pub const ENV_STORE_ROOT: &str = "RR_STORE_ROOT";
    pub const ENV_OPENCODE_BIN: &str = "RR_OPENCODE_BIN";
    pub const ENV_BRIDGE_EXTENSION_ID: &str = "RR_BRIDGE_EXTENSION_ID";
    pub const ENV_BRIDGE_HOST_BINARY: &str = "RR_BRIDGE_HOST_BINARY";
    pub const ENV_EXTENSION_PROFILE_ROOT: &str = "RR_EXTENSION_PROFILE_ROOT";

    pub const DEFAULT_OPENCODE_BIN: &str = "opencode";
    pub const DEFAULT_CODEX_BIN: &str = "codex";
    pub const DEFAULT_GEMINI_BIN: &str = "gemini";
    pub const DEFAULT_CLAUDE_BIN: &str = "claude";
    pub const DEFAULT_UI_TARGET: &str = "cli";
    pub const DEFAULT_INSTANCE_PREFERENCE: &str = "reuse_if_possible";
    pub const DEFAULT_LAUNCH_PROFILE_ID: &str = "profile-open-pr";
    pub const DEFAULT_PROVIDER: &str = "opencode";
    pub const DEFAULT_ISOLATION_MODE: &str = "current_checkout";
    pub const DEFAULT_PROMPT_CONTRACT_VERSION: &str = "prompt_preset_contract.v1";
    pub const DEFAULT_HOOK_CONTRACT_VERSION: &str = "none";
}

const RESOLVED_CONFIG_SCHEMA_VERSION: u32 = 1;
const LIVE_REVIEW_PROVIDERS: [&str; 4] = ["opencode", "codex", "gemini", "claude"];
const KNOWN_PROVIDERS: [&str; 6] = [
    "opencode", "codex", "gemini", "claude", "copilot", "pi-agent",
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueProvenanceLayer {
    BuiltIn,
    Env,
    EnvRepair,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValueProvenance {
    pub layer: ValueProvenanceLayer,
    pub source: String,
    pub repair_only: bool,
}

impl ValueProvenance {
    fn built_in(source: impl Into<String>) -> Self {
        Self {
            layer: ValueProvenanceLayer::BuiltIn,
            source: source.into(),
            repair_only: false,
        }
    }

    fn env(source: &'static str, repair_only: bool) -> Self {
        Self {
            layer: if repair_only {
                ValueProvenanceLayer::EnvRepair
            } else {
                ValueProvenanceLayer::Env
            },
            source: source.to_owned(),
            repair_only,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedValue<T> {
    pub value: T,
    pub provenance: ValueProvenance,
}

impl<T> ResolvedValue<T> {
    fn built_in(value: T, source: impl Into<String>) -> Self {
        Self {
            value,
            provenance: ValueProvenance::built_in(source),
        }
    }

    fn env(value: T, source: &'static str, repair_only: bool) -> Self {
        Self {
            value,
            provenance: ValueProvenance::env(source, repair_only),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedPolicyProfile {
    pub id: String,
    pub summary: String,
    pub mutation_posture: String,
    pub continuity_mode: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedProviderSupportMatrix {
    pub review_start: bool,
    pub resume_reseed: bool,
    pub resume_reopen: bool,
    pub rr_return: bool,
    pub status: bool,
    pub findings: bool,
    pub sessions: bool,
    pub doctor: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedProviderCapability {
    pub provider: String,
    pub display_name: String,
    pub status: String,
    pub support_tier: String,
    pub surface_class: String,
    pub capability_provenance: ValueProvenance,
    pub policy_profile: ResolvedPolicyProfile,
    pub hook_contract_version: ResolvedValue<String>,
    pub instruction_contract_version: ResolvedValue<String>,
    pub binary_path: ResolvedValue<String>,
    pub asset_root: Option<ResolvedValue<String>>,
    pub fail_closed_reason: Option<String>,
    pub degraded_reason: Option<String>,
    pub supports: ResolvedProviderSupportMatrix,
    pub notes: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedBridgeConfig {
    pub extension_id: Option<ResolvedValue<String>>,
    pub host_binary: Option<ResolvedValue<String>>,
    pub profile_root: Option<ResolvedValue<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedLaunchBaseline {
    pub launch_profile_id: ResolvedValue<String>,
    pub default_provider: ResolvedValue<String>,
    pub ui_target: ResolvedValue<String>,
    pub instance_preference: ResolvedValue<String>,
    pub isolation_mode: ResolvedValue<String>,
    pub named_instance_on_collision: ResolvedValue<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRogerConfig {
    pub schema_version: u32,
    pub surface: String,
    pub store_root: ResolvedValue<String>,
    pub launch: ResolvedLaunchBaseline,
    pub bridge: ResolvedBridgeConfig,
    pub providers: Vec<ResolvedProviderCapability>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRoutineSurfaceBaseline {
    pub surface: String,
    pub launch_profile_id: ResolvedValue<String>,
    pub provider: ResolvedProviderCapability,
    pub ui_target: ResolvedValue<String>,
    pub instance_preference: ResolvedValue<String>,
    pub isolation_mode: ResolvedValue<String>,
    pub named_instance_on_collision: ResolvedValue<bool>,
    pub repair_overrides_active: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub active_repair_override_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedConfigError {
    pub reason_code: String,
    pub message: String,
}

impl ResolvedProviderCapability {
    pub fn status_reason(&self) -> Option<String> {
        self.fail_closed_reason
            .clone()
            .or_else(|| self.degraded_reason.clone())
    }
}

impl ResolvedRogerConfig {
    pub fn provider(&self, provider: &str) -> Option<&ResolvedProviderCapability> {
        self.providers.iter().find(|item| item.provider == provider)
    }

    pub fn routine_surface_baseline(
        &self,
        provider_override: Option<&str>,
    ) -> Result<ResolvedRoutineSurfaceBaseline, ResolvedConfigError> {
        let provider_key = provider_override.unwrap_or(self.launch.default_provider.value.as_str());
        let provider = self.provider(provider_key).cloned().ok_or_else(|| {
            let reason_code = if provider_override.is_some() {
                "provider_override_unknown"
            } else {
                "default_provider_unknown"
            };
            ResolvedConfigError {
                reason_code: reason_code.to_owned(),
                message: format!(
                    "resolved config has no provider capability entry for '{provider_key}'"
                ),
            }
        })?;

        let active_repair_override_keys = if self.repair_overrides_active() {
            self.active_repair_override_keys()
        } else {
            Vec::new()
        };

        Ok(ResolvedRoutineSurfaceBaseline {
            surface: self.surface.clone(),
            launch_profile_id: self.launch.launch_profile_id.clone(),
            provider,
            ui_target: self.launch.ui_target.clone(),
            instance_preference: self.launch.instance_preference.clone(),
            isolation_mode: self.launch.isolation_mode.clone(),
            named_instance_on_collision: self.launch.named_instance_on_collision.clone(),
            repair_overrides_active: self.repair_overrides_active(),
            active_repair_override_keys,
            status_reason: self
                .provider(provider_key)
                .and_then(ResolvedProviderCapability::status_reason),
        })
    }

    pub fn repair_overrides_active(&self) -> bool {
        self.store_root.provenance.repair_only
            || self
                .bridge
                .extension_id
                .as_ref()
                .is_some_and(|value| value.provenance.repair_only)
            || self
                .bridge
                .host_binary
                .as_ref()
                .is_some_and(|value| value.provenance.repair_only)
            || self
                .bridge
                .profile_root
                .as_ref()
                .is_some_and(|value| value.provenance.repair_only)
            || self
                .providers
                .iter()
                .any(|provider| provider.binary_path.provenance.repair_only)
    }

    pub fn active_repair_override_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        if self.store_root.provenance.repair_only {
            keys.push(self.store_root.provenance.source.clone());
        }
        if let Some(value) = self.bridge.extension_id.as_ref() {
            if value.provenance.repair_only {
                keys.push(value.provenance.source.clone());
            }
        }
        if let Some(value) = self.bridge.host_binary.as_ref() {
            if value.provenance.repair_only {
                keys.push(value.provenance.source.clone());
            }
        }
        if let Some(value) = self.bridge.profile_root.as_ref() {
            if value.provenance.repair_only {
                keys.push(value.provenance.source.clone());
            }
        }
        for provider in &self.providers {
            if provider.binary_path.provenance.repair_only {
                keys.push(provider.binary_path.provenance.source.clone());
            }
        }
        keys
    }
}

pub fn resolve_cli_config(cwd: &Path) -> ResolvedRogerConfig {
    resolve_cli_config_from_lookup(cwd, |key| std::env::var(key).ok())
}

pub fn resolve_cli_config_from_lookup<F>(cwd: &Path, mut lookup: F) -> ResolvedRogerConfig
where
    F: FnMut(&str) -> Option<String>,
{
    let store_root = lookup(cli_defaults::ENV_STORE_ROOT)
        .map(|value| ResolvedValue::env(value, cli_defaults::ENV_STORE_ROOT, true))
        .unwrap_or_else(|| {
            ResolvedValue::built_in(
                cwd.join(".roger").to_string_lossy().into_owned(),
                "store.root",
            )
        });

    let launch = ResolvedLaunchBaseline {
        launch_profile_id: ResolvedValue::built_in(
            cli_defaults::DEFAULT_LAUNCH_PROFILE_ID.to_owned(),
            "launch.defaults.by_surface.cli",
        ),
        default_provider: ResolvedValue::built_in(
            cli_defaults::DEFAULT_PROVIDER.to_owned(),
            "launch.defaults.by_surface.cli.provider",
        ),
        ui_target: ResolvedValue::built_in(
            cli_defaults::DEFAULT_UI_TARGET.to_owned(),
            "launch.defaults.by_surface.cli.ui_target",
        ),
        instance_preference: ResolvedValue::built_in(
            cli_defaults::DEFAULT_INSTANCE_PREFERENCE.to_owned(),
            "launch_profiles.profile-open-pr.reuse_policy",
        ),
        isolation_mode: ResolvedValue::built_in(
            cli_defaults::DEFAULT_ISOLATION_MODE.to_owned(),
            "isolation.default_mode",
        ),
        named_instance_on_collision: ResolvedValue::built_in(
            true,
            "isolation.named_instance.default",
        ),
    };

    let bridge = ResolvedBridgeConfig {
        extension_id: lookup(cli_defaults::ENV_BRIDGE_EXTENSION_ID)
            .map(|value| ResolvedValue::env(value, cli_defaults::ENV_BRIDGE_EXTENSION_ID, true)),
        host_binary: lookup(cli_defaults::ENV_BRIDGE_HOST_BINARY)
            .map(|value| ResolvedValue::env(value, cli_defaults::ENV_BRIDGE_HOST_BINARY, true)),
        profile_root: lookup(cli_defaults::ENV_EXTENSION_PROFILE_ROOT)
            .map(|value| ResolvedValue::env(value, cli_defaults::ENV_EXTENSION_PROFILE_ROOT, true)),
    };

    let providers = KNOWN_PROVIDERS
        .iter()
        .map(|provider| resolve_provider_capability(provider, &mut lookup))
        .collect();

    ResolvedRogerConfig {
        schema_version: RESOLVED_CONFIG_SCHEMA_VERSION,
        surface: "cli".to_owned(),
        store_root,
        launch,
        bridge,
        providers,
    }
}

fn resolve_provider_capability<F>(provider: &str, lookup: &mut F) -> ResolvedProviderCapability
where
    F: FnMut(&str) -> Option<String>,
{
    let binary_path = match provider {
        "opencode" => lookup(cli_defaults::ENV_OPENCODE_BIN)
            .map(|value| ResolvedValue::env(value, cli_defaults::ENV_OPENCODE_BIN, false))
            .unwrap_or_else(|| {
                ResolvedValue::built_in(
                    cli_defaults::DEFAULT_OPENCODE_BIN.to_owned(),
                    "providers.opencode.binary_path",
                )
            }),
        "codex" => ResolvedValue::built_in(
            cli_defaults::DEFAULT_CODEX_BIN.to_owned(),
            "providers.codex.binary_path",
        ),
        "gemini" => ResolvedValue::built_in(
            cli_defaults::DEFAULT_GEMINI_BIN.to_owned(),
            "providers.gemini.binary_path",
        ),
        "claude" => ResolvedValue::built_in(
            cli_defaults::DEFAULT_CLAUDE_BIN.to_owned(),
            "providers.claude.binary_path",
        ),
        "copilot" => {
            ResolvedValue::built_in("github-copilot".to_owned(), "providers.copilot.binary_path")
        }
        "pi-agent" => {
            ResolvedValue::built_in("pi-agent".to_owned(), "providers.pi-agent.binary_path")
        }
        _ => ResolvedValue::built_in(provider.to_owned(), "providers.unknown.binary_path"),
    };

    ResolvedProviderCapability {
        provider: provider.to_owned(),
        display_name: provider_display_name(provider).to_owned(),
        status: provider_support_status(provider).to_owned(),
        support_tier: provider_support_tier(provider).to_owned(),
        surface_class: provider_surface_class(provider).to_owned(),
        capability_provenance: ValueProvenance::built_in(format!(
            "providers.{provider}.support_matrix"
        )),
        policy_profile: provider_policy_profile(provider),
        hook_contract_version: ResolvedValue::built_in(
            provider_hook_contract_version(provider).to_owned(),
            format!("providers.{provider}.hook_contract_version"),
        ),
        instruction_contract_version: ResolvedValue::built_in(
            provider_instruction_contract_version(provider).to_owned(),
            format!("providers.{provider}.instruction_contract_version"),
        ),
        binary_path,
        asset_root: None,
        fail_closed_reason: provider_fail_closed_reason(provider).map(ToOwned::to_owned),
        degraded_reason: provider_degraded_reason(provider).map(ToOwned::to_owned),
        supports: ResolvedProviderSupportMatrix {
            review_start: LIVE_REVIEW_PROVIDERS.contains(&provider),
            resume_reseed: LIVE_REVIEW_PROVIDERS.contains(&provider),
            resume_reopen: provider == "opencode",
            rr_return: provider == "opencode",
            status: true,
            findings: true,
            sessions: true,
            doctor: provider != "pi-agent",
        },
        notes: provider_support_notes(provider).to_owned(),
    }
}

fn provider_display_name(provider: &str) -> &'static str {
    match provider {
        "opencode" => "OpenCode",
        "codex" => "Codex",
        "gemini" => "Gemini",
        "claude" => "Claude Code",
        "copilot" => "GitHub Copilot CLI",
        "pi-agent" => "Pi-Agent",
        _ => "Unknown provider",
    }
}

fn provider_support_status(provider: &str) -> &'static str {
    match provider {
        "opencode" => "first_class_live",
        "codex" | "gemini" | "claude" => "bounded_live",
        "copilot" => "planned_not_live",
        _ => "not_supported",
    }
}

fn provider_support_tier(provider: &str) -> &'static str {
    match provider {
        "opencode" => "tier_b",
        "codex" | "gemini" | "claude" => "tier_a",
        "copilot" => "tier_a_planned",
        _ => "unsupported",
    }
}

fn provider_surface_class(provider: &str) -> &'static str {
    match provider {
        "opencode" => "review_primary",
        "codex" | "gemini" | "claude" => "review_bounded",
        "copilot" => "admission_pending",
        _ => "not_live",
    }
}

fn provider_policy_profile(provider: &str) -> ResolvedPolicyProfile {
    match provider {
        "opencode" => ResolvedPolicyProfile {
            id: "review_safe_tier_b_continuity".to_owned(),
            summary: "first-class review-safe policy with locator reopen and rr return".to_owned(),
            mutation_posture: "review_only".to_owned(),
            continuity_mode: "locator_reopen_and_return".to_owned(),
        },
        "codex" | "gemini" | "claude" => ResolvedPolicyProfile {
            id: "review_safe_tier_a_reseed_only".to_owned(),
            summary: "bounded review-safe policy with reseed/raw-capture continuity only"
                .to_owned(),
            mutation_posture: "review_only".to_owned(),
            continuity_mode: "reseed_only".to_owned(),
        },
        "copilot" => ResolvedPolicyProfile {
            id: "provider_admission_pending".to_owned(),
            summary: "provider admission is planned, but live policy hooks and proofs are pending"
                .to_owned(),
            mutation_posture: "review_only".to_owned(),
            continuity_mode: "not_live".to_owned(),
        },
        _ => ResolvedPolicyProfile {
            id: "unsupported".to_owned(),
            summary: "provider is outside the live Roger 0.1.0 surface".to_owned(),
            mutation_posture: "blocked".to_owned(),
            continuity_mode: "not_live".to_owned(),
        },
    }
}

fn provider_hook_contract_version(provider: &str) -> &'static str {
    match provider {
        "copilot" => "pending_provider_hooks",
        _ => cli_defaults::DEFAULT_HOOK_CONTRACT_VERSION,
    }
}

fn provider_instruction_contract_version(provider: &str) -> &'static str {
    match provider {
        "copilot" => "pending_provider_instructions",
        "pi-agent" => "none",
        _ => cli_defaults::DEFAULT_PROMPT_CONTRACT_VERSION,
    }
}

fn provider_support_notes(provider: &str) -> &'static str {
    match provider {
        "opencode" => "first-class tier-b continuity path with locator reopen and rr return",
        "codex" | "gemini" | "claude" => {
            "bounded tier-a start/reseed/raw-capture path only; no locator reopen or rr return"
        }
        "copilot" => "planned target, not yet a live rr review --provider value",
        "pi-agent" => "not part of the 0.1.0 live CLI surface",
        _ => "provider is not part of the current live rr review surface",
    }
}

fn provider_fail_closed_reason(provider: &str) -> Option<&'static str> {
    match provider {
        "copilot" => Some("provider_not_live"),
        "pi-agent" => Some("provider_not_supported"),
        _ => None,
    }
}

fn provider_degraded_reason(provider: &str) -> Option<&'static str> {
    match provider {
        "codex" | "gemini" | "claude" => Some("tier_a_reseed_only_no_locator_reopen_or_return"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ResolvedConfigError, ValueProvenanceLayer, cli_defaults, resolve_cli_config_from_lookup,
    };
    use std::collections::HashMap;
    use std::path::Path;

    #[test]
    fn resolves_built_in_cli_baseline_and_provider_matrix() {
        let resolved = resolve_cli_config_from_lookup(Path::new("/tmp/roger"), |_| None);

        assert_eq!(resolved.schema_version, 1);
        assert_eq!(resolved.surface, "cli");
        assert_eq!(resolved.store_root.value, "/tmp/roger/.roger");
        assert_eq!(
            resolved.store_root.provenance.layer,
            ValueProvenanceLayer::BuiltIn
        );
        assert_eq!(resolved.launch.launch_profile_id.value, "profile-open-pr");
        assert_eq!(resolved.launch.default_provider.value, "opencode");
        assert_eq!(resolved.launch.ui_target.value, "cli");
        assert_eq!(
            resolved.launch.instance_preference.value,
            "reuse_if_possible"
        );
        assert_eq!(resolved.launch.isolation_mode.value, "current_checkout");
        assert_eq!(resolved.launch.named_instance_on_collision.value, true);

        let opencode = resolved.provider("opencode").expect("opencode capability");
        assert_eq!(opencode.display_name, "OpenCode");
        assert_eq!(opencode.status, "first_class_live");
        assert_eq!(opencode.support_tier, "tier_b");
        assert_eq!(opencode.surface_class, "review_primary");
        assert_eq!(
            opencode.capability_provenance.layer,
            ValueProvenanceLayer::BuiltIn
        );
        assert_eq!(
            opencode.capability_provenance.source,
            "providers.opencode.support_matrix"
        );
        assert_eq!(opencode.policy_profile.id, "review_safe_tier_b_continuity");
        assert_eq!(opencode.binary_path.value, "opencode");
        assert_eq!(opencode.hook_contract_version.value, "none");
        assert_eq!(
            opencode.instruction_contract_version.value,
            "prompt_preset_contract.v1"
        );
        assert!(opencode.fail_closed_reason.is_none());
        assert!(opencode.degraded_reason.is_none());
        assert!(opencode.supports.resume_reopen);
        assert!(opencode.supports.rr_return);

        let codex = resolved.provider("codex").expect("codex capability");
        assert_eq!(codex.status, "bounded_live");
        assert_eq!(codex.support_tier, "tier_a");
        assert_eq!(codex.surface_class, "review_bounded");
        assert_eq!(
            codex.degraded_reason.as_deref(),
            Some("tier_a_reseed_only_no_locator_reopen_or_return")
        );
        assert!(!codex.supports.resume_reopen);
        assert!(!codex.supports.rr_return);

        let copilot = resolved.provider("copilot").expect("copilot capability");
        assert_eq!(copilot.status, "planned_not_live");
        assert_eq!(copilot.support_tier, "tier_a_planned");
        assert_eq!(copilot.surface_class, "admission_pending");
        assert_eq!(copilot.policy_profile.id, "provider_admission_pending");
        assert_eq!(
            copilot.fail_closed_reason.as_deref(),
            Some("provider_not_live")
        );
        assert_eq!(
            copilot.hook_contract_version.value,
            "pending_provider_hooks"
        );
        assert_eq!(
            copilot.instruction_contract_version.value,
            "pending_provider_instructions"
        );
        assert!(!resolved.repair_overrides_active());
        assert!(resolved.active_repair_override_keys().is_empty());
    }

    #[test]
    fn tracks_env_provenance_for_operator_and_repair_overrides() {
        let env = HashMap::from([
            (
                cli_defaults::ENV_STORE_ROOT.to_owned(),
                "/tmp/custom-store".to_owned(),
            ),
            (
                cli_defaults::ENV_OPENCODE_BIN.to_owned(),
                "/opt/opencode/bin/opencode".to_owned(),
            ),
            (
                cli_defaults::ENV_BRIDGE_EXTENSION_ID.to_owned(),
                "abcd1234".to_owned(),
            ),
            (
                cli_defaults::ENV_BRIDGE_HOST_BINARY.to_owned(),
                "/tmp/rr-host".to_owned(),
            ),
            (
                cli_defaults::ENV_EXTENSION_PROFILE_ROOT.to_owned(),
                "/tmp/browser-profile".to_owned(),
            ),
        ]);
        let resolved =
            resolve_cli_config_from_lookup(Path::new("/tmp/roger"), |key| env.get(key).cloned());

        assert_eq!(resolved.store_root.value, "/tmp/custom-store");
        assert_eq!(
            resolved.store_root.provenance.layer,
            ValueProvenanceLayer::EnvRepair
        );
        assert!(resolved.store_root.provenance.repair_only);

        let opencode = resolved.provider("opencode").expect("opencode capability");
        assert_eq!(opencode.binary_path.value, "/opt/opencode/bin/opencode");
        assert_eq!(
            opencode.binary_path.provenance.layer,
            ValueProvenanceLayer::Env
        );
        assert!(!opencode.binary_path.provenance.repair_only);

        assert_eq!(
            resolved
                .bridge
                .extension_id
                .as_ref()
                .expect("extension id")
                .provenance
                .layer,
            ValueProvenanceLayer::EnvRepair
        );
        assert_eq!(
            resolved
                .bridge
                .host_binary
                .as_ref()
                .expect("host binary")
                .value,
            "/tmp/rr-host"
        );
        assert_eq!(
            resolved
                .bridge
                .profile_root
                .as_ref()
                .expect("profile root")
                .value,
            "/tmp/browser-profile"
        );

        assert!(resolved.repair_overrides_active());
        assert_eq!(
            resolved.active_repair_override_keys(),
            vec![
                cli_defaults::ENV_STORE_ROOT.to_owned(),
                cli_defaults::ENV_BRIDGE_EXTENSION_ID.to_owned(),
                cli_defaults::ENV_BRIDGE_HOST_BINARY.to_owned(),
                cli_defaults::ENV_EXTENSION_PROFILE_ROOT.to_owned(),
            ]
        );
    }

    #[test]
    fn builds_routine_surface_baseline_for_default_provider() {
        let resolved = resolve_cli_config_from_lookup(Path::new("/tmp/roger"), |_| None);

        let baseline = resolved
            .routine_surface_baseline(None)
            .expect("routine surface baseline");
        assert_eq!(baseline.surface, "cli");
        assert_eq!(baseline.launch_profile_id.value, "profile-open-pr");
        assert_eq!(baseline.provider.provider, "opencode");
        assert_eq!(baseline.provider.support_tier, "tier_b");
        assert_eq!(baseline.provider.surface_class, "review_primary");
        assert_eq!(
            baseline.provider.policy_profile.id,
            "review_safe_tier_b_continuity"
        );
        assert_eq!(baseline.ui_target.value, "cli");
        assert_eq!(baseline.instance_preference.value, "reuse_if_possible");
        assert_eq!(baseline.isolation_mode.value, "current_checkout");
        assert_eq!(baseline.named_instance_on_collision.value, true);
        assert!(!baseline.repair_overrides_active);
        assert!(baseline.active_repair_override_keys.is_empty());
        assert!(baseline.status_reason.is_none());
    }

    #[test]
    fn routine_surface_baseline_surfaces_fail_closed_reason_for_provider_override() {
        let resolved = resolve_cli_config_from_lookup(Path::new("/tmp/roger"), |_| None);

        let baseline = resolved
            .routine_surface_baseline(Some("copilot"))
            .expect("copilot routine surface baseline");
        assert_eq!(baseline.provider.provider, "copilot");
        assert_eq!(baseline.provider.support_tier, "tier_a_planned");
        assert_eq!(baseline.provider.surface_class, "admission_pending");
        assert_eq!(
            baseline.status_reason.as_deref(),
            Some("provider_not_live")
        );
        assert_eq!(
            baseline.provider.capability_provenance.source,
            "providers.copilot.support_matrix"
        );
    }

    #[test]
    fn routine_surface_baseline_keeps_repair_provenance_demoted_but_visible() {
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
            .routine_surface_baseline(None)
            .expect("routine surface baseline");
        assert!(baseline.repair_overrides_active);
        assert_eq!(
            baseline.active_repair_override_keys,
            vec![
                cli_defaults::ENV_STORE_ROOT.to_owned(),
                cli_defaults::ENV_BRIDGE_HOST_BINARY.to_owned(),
            ]
        );
        assert_eq!(baseline.provider.provider, "opencode");
    }

    #[test]
    fn routine_surface_baseline_rejects_unknown_provider_override() {
        let resolved = resolve_cli_config_from_lookup(Path::new("/tmp/roger"), |_| None);

        let err = resolved
            .routine_surface_baseline(Some("totally-unknown"))
            .expect_err("unknown provider should fail closed");
        assert_eq!(
            err,
            ResolvedConfigError {
                reason_code: "provider_override_unknown".to_owned(),
                message:
                    "resolved config has no provider capability entry for 'totally-unknown'"
                        .to_owned(),
            }
        );
    }
}
