use std::path::PathBuf;

use super::CommandContext;

/// Where this invocation's desktop policies come from, carried on the
/// context so batch items and host-forwarded commands keep the caller's
/// policy surface. The core only transports the paths; the binary's gate
/// resolves and enforces them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolicyPaths {
    pub files: Vec<PathBuf>,
    pub dirs: Vec<PathBuf>,
    /// The caller's `AGENT_DESKTOP_POLICY_DIR`, resolved where the caller's
    /// environment is visible.
    pub env_dir: Option<PathBuf>,
}

impl CommandContext {
    pub fn with_policy_paths(mut self, policy_paths: PolicyPaths) -> Self {
        self.options.policy_paths = policy_paths;
        self
    }

    pub fn policy_paths(&self) -> &PolicyPaths {
        &self.options.policy_paths
    }
}
