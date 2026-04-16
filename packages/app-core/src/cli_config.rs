pub use roger_config::cli_defaults::{
    DEFAULT_INSTANCE_PREFERENCE as INSTANCE_PREFERENCE, DEFAULT_LAUNCH_PROFILE_ID as PROFILE_ID,
    DEFAULT_UI_TARGET as UI_TARGET,
};

use roger_config::ResolvedRogerConfig;
use std::path::Path;

pub fn resolved_cli_config(cwd: &Path) -> ResolvedRogerConfig {
    roger_config::resolve_cli_config(cwd)
}
