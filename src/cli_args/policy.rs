use std::path::PathBuf;

use agent_desktop_core::AppError;
use clap::Args;

/// Extra Dogwood policy layers for one invocation. The owner's policies
/// folder always applies; these add layers that can only refuse more.
#[derive(Args, Debug, Default)]
pub(crate) struct PolicyArgs {
    #[arg(
        long,
        global = true,
        value_name = "FILE",
        help = "Also enforce this Dogwood policy file (repeatable; adds a layer, never lifts the owner's policies folder)"
    )]
    pub policy: Vec<PathBuf>,
    #[arg(
        long,
        global = true,
        value_name = "DIR",
        help = "Also enforce the .dogwood policies in this directory (repeatable; adds a layer, never lifts the owner's policies folder)"
    )]
    pub policy_dir: Vec<PathBuf>,
}

impl PolicyArgs {
    /// The `AGENT_DESKTOP_POLICY_DIR` of the agent that issued the command.
    /// An empty or relative value is refused like `AGENT_DESKTOP_HOME`: it
    /// would name a different folder from each working directory.
    pub(crate) fn env_dir(&self) -> Result<Option<PathBuf>, AppError> {
        let Some(value) = std::env::var_os(agent_desktop_policy::POLICY_DIR_ENV) else {
            return Ok(None);
        };
        let dir = PathBuf::from(value);
        if !dir.is_absolute() {
            return Err(AppError::invalid_input_with_suggestion(
                format!(
                    "{} must be an absolute directory path, got '{}'",
                    agent_desktop_policy::POLICY_DIR_ENV,
                    dir.display()
                ),
                "Unset the variable or set it to an absolute directory of .dogwood files.",
            ));
        }
        Ok(Some(dir))
    }
}
