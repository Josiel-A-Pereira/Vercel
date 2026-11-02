//! Owner-controlled guardrails for agent-desktop, evaluated by the Dogwood
//! Local Engine. The binary builds a [`CommandFacts`] for every gated
//! command, asks the gate for a verdict before dispatch, and records the
//! outcome after; policies come from files the machine's owner controls
//! (see [`discover`]). With no policy configured the gate is inert.
//!
//! Dogwood keeps Cedar's semantics — deny-by-default, `forbid` overrides
//! `permit` — and adds `when temporal { … }` clauses over the recorded
//! request/response history, so a policy can require observation before
//! action or bound action rates.

mod account_home;
mod catalog;
mod discover;
mod discovered;
mod gate;
mod install;
mod store;
mod target_facts;

pub use catalog::{COMMAND_GROUPS, GROUPS, group_for, schema_text};
pub use discover::{Origin, POLICY_DIR_ENV, PolicySources, discover, engine_state_dir};
pub use discovered::Discovered;
pub use gate::PolicyGate;
pub use target_facts::TargetFacts;

/// What the gate knows about one command at one of its targets before
/// dispatch. Every event carries the full schema vocabulary.
#[derive(Debug, Clone)]
pub struct CommandFacts {
    /// The CLI command name, e.g. `click`; must appear in [`COMMAND_GROUPS`].
    pub command: &'static str,
    /// The acting agent: `--agent-id` / `AGENT_DESKTOP_AGENT_ID`, else `default`.
    pub agent: String,
    /// Where the command lands.
    pub target: TargetFacts,
    /// Whether physical input was allowed (`--headed`).
    pub headed: bool,
    /// Whether the command mutates the desktop.
    pub mutating: bool,
    /// The active session id, or empty.
    pub session: String,
}

impl CommandFacts {
    /// The resource entity id: the app name, or `system`.
    pub fn app_or_system(&self) -> &str {
        self.target.app_or_system()
    }

    pub(crate) fn string_fields(&self) -> [(&'static str, &str); 6] {
        [
            ("command", self.command),
            ("app", self.app_or_system()),
            ("app_id", &self.target.app_id),
            ("window", &self.target.window),
            ("role", &self.target.role),
            ("element", &self.target.element),
        ]
    }
}

/// The gate's answer for one request event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    /// Denied, with the `@id` annotations of the determining policies (empty
    /// when nothing permitted the request) and any evaluation errors the
    /// authorizer failed closed on.
    Denied {
        policies: Vec<String>,
        errors: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyError {
    /// A policy source could not be read or enumerated.
    Load {
        path: std::path::PathBuf,
        message: String,
    },
    /// A policy source is writable by someone other than the owner.
    Insecure {
        path: std::path::PathBuf,
        message: String,
    },
    /// The policy text was rejected by the parser, validator, or schema.
    Install { message: String },
    /// The engine or its durable store failed.
    Engine { message: String },
    /// The gate's own state directory could not be prepared.
    State { message: String },
}

impl std::fmt::Display for PolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load { path, message } => {
                write!(f, "policy source {}: {message}", path.display())
            }
            Self::Insecure { path, message } => {
                write!(f, "policy source {} is insecure: {message}", path.display())
            }
            Self::Install { message } => write!(f, "policy rejected: {message}"),
            Self::Engine { message } => write!(f, "policy engine: {message}"),
            Self::State { message } => write!(f, "policy state: {message}"),
        }
    }
}

impl std::error::Error for PolicyError {}

#[cfg(test)]
mod tests;
