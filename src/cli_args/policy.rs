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
    /// The caller's `AGENT_DESKTOP_POLICY_DIR`, carried to a command host
    /// that does not share the caller's environment.
    #[arg(skip)]
    pub env_dir: Option<PathBuf>,
}

impl PolicyArgs {
    /// The `AGENT_DESKTOP_POLICY_DIR` of the agent that issued the command.
    /// A command host serves many callers and inherited only the
    /// environment of the one that started it, so it uses the value each
    /// request carries and never its own.
    ///
    /// An empty or relative value is refused like `AGENT_DESKTOP_HOME`: it
    /// would name a different folder from each working directory.
    pub(crate) fn caller_env_dir(&self) -> Result<Option<PathBuf>, AppError> {
        if self.env_dir.is_some() {
            return Ok(self.env_dir.clone());
        }
        #[cfg(target_os = "macos")]
        if crate::command_host::is_host_process() {
            return Ok(None);
        }
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
