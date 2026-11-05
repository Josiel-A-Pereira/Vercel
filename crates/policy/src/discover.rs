//! Policy source discovery. Every configured source is its own layer, and
//! a command runs only if every layer allows it: the owner's folder under
//! the state root (`~/.agent-desktop/policies`), then the
//! `AGENT_DESKTOP_POLICY_DIR` directory, then each `--policy` file and each
//! `--policy-dir` directory on its own, so one flag's permits never lift
//! another flag's deny-by-default. A caller can add layers that refuse more; none can
//! lift the owner's folder. A source the caller named explicitly must
//! exist; the owner's folder is optional.

use std::path::{Path, PathBuf};

use crate::PolicyError;
use crate::discovered::Discovered;

pub const POLICY_DIR_ENV: &str = "AGENT_DESKTOP_POLICY_DIR";
pub const POLICY_FILE_EXTENSION: &str = "dogwood";
const STATE_DIR_NAME: &str = ".agent-desktop";
const POLICIES_DIR_NAME: &str = "policies";
const ENGINE_STATE_DIR_NAME: &str = "policy-state";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Cli,
    Env,
    Home,
    Account,
}

impl Origin {
    pub fn describe(self) -> &'static str {
        match self {
            Self::Cli => "--policy/--policy-dir",
            Self::Env => POLICY_DIR_ENV,
            Self::Home => "the state root's policies folder",
            Self::Account => "the account's policies folder (~/.agent-desktop/policies)",
        }
    }
}

/// One layer's policy files, in deterministic load order, and where the
/// layer lives: its folder, or its single file. The layer keeps one history
/// store per location, so adding a file to a folder keeps the history.
#[derive(Debug, Clone)]
pub struct PolicySources {
    pub files: Vec<PathBuf>,
    pub origin: Origin,
    pub root: PathBuf,
}

impl PolicySources {
    /// A layer of exactly one policy file.
    pub fn for_file(file: PathBuf, origin: Origin) -> Self {
        Self {
            root: file.clone(),
            files: vec![file],
            origin,
        }
    }
}

/// Resolve the policy layers for one invocation, owner's folder first. A
/// layer that cannot be read is reported apart from the healthy ones, so
/// one broken layer never hides another. No layers and no failures means
/// no policies are configured anywhere and the gate stays out of the way.
pub fn discover(cli_files: &[PathBuf], cli_dirs: &[PathBuf], env_dir: Option<&Path>) -> Discovered {
    let account_folder = crate::account_home::account_home()
        .map(|home| home.join(STATE_DIR_NAME).join(POLICIES_DIR_NAME));
    discover_from(account_folder.as_deref(), cli_files, cli_dirs, env_dir)
}

/// [`discover`] with the account's own policies folder given, the seam
/// tests point at a scratch folder instead of the real account's. The
/// account folder applies even when `HOME` or `AGENT_DESKTOP_HOME` moved
/// the state root elsewhere, so relocating the state root never leaves the
/// owner's policies behind; when both name the same folder it is one layer.
pub(crate) fn discover_from(
    account_folder: Option<&Path>,
    cli_files: &[PathBuf],
    cli_dirs: &[PathBuf],
    env_dir: Option<&Path>,
) -> Discovered {
    let mut found = Discovered::default();
    let state_root_folder = default_policy_dir();
    if let Some(folder) = account_folder
        && !state_root_folder
            .as_ref()
            .is_ok_and(|other| same_folder(other, folder))
    {
        found.add(owner_layer(folder, Origin::Account));
    }
    found.add(state_root_folder.and_then(|folder| owner_layer(&folder, Origin::Home)));
    if let Some(dir) = env_dir {
        found.add(env_layer(dir).map(Some));
    }
    for file in cli_files {
        found.add(cli_file_layer(file).map(Some));
    }
    for dir in cli_dirs {
        found.add(cli_dir_layer(dir).map(Some));
    }
    found
}

fn owner_layer(folder: &Path, origin: Origin) -> Result<Option<PolicySources>, PolicyError> {
    let files = sorted(policy_files_in(folder, false)?);
    Ok((!files.is_empty()).then_some(PolicySources {
        files,
        origin,
        root: folder.to_path_buf(),
    }))
}

pub(crate) fn same_folder(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn env_layer(dir: &Path) -> Result<PolicySources, PolicyError> {
    let files = sorted(policy_files_in(dir, true)?);
    if files.is_empty() {
        return Err(PolicyError::Load {
            path: dir.to_path_buf(),
            message: format!("{POLICY_DIR_ENV} is set but holds no .{POLICY_FILE_EXTENSION} files"),
        });
    }
    Ok(PolicySources {
        files,
        origin: Origin::Env,
        root: dir.to_path_buf(),
    })
}

fn cli_file_layer(file: &Path) -> Result<PolicySources, PolicyError> {
    require_policy_file(file)?;
    Ok(PolicySources::for_file(file.to_path_buf(), Origin::Cli))
}

fn cli_dir_layer(dir: &Path) -> Result<PolicySources, PolicyError> {
    let files = sorted(policy_files_in(dir, true)?);
    if files.is_empty() {
        return Err(PolicyError::Load {
            path: dir.to_path_buf(),
            message: format!("no .{POLICY_FILE_EXTENSION} files found in --policy-dir"),
        });
    }
    Ok(PolicySources {
        files,
        origin: Origin::Cli,
        root: dir.to_path_buf(),
    })
}

fn sorted(mut files: Vec<PathBuf>) -> Vec<PathBuf> {
    files.sort();
    files.dedup();
    files
}

/// `<state root>/policy-state` — the engine's durable log, written only by
/// the gate. It lives apart from the user-edited `policies/` folder so an
/// owner can lock that folder without freezing the log.
pub fn engine_state_dir() -> Result<PathBuf, PolicyError> {
    Ok(state_root()?.join(ENGINE_STATE_DIR_NAME))
}

/// Where a layer of `origin` keeps its history. The account's folder keeps
/// it under the account's own home, found the way the folder itself is, so
/// relocating the state root never hands the owner's rules an empty
/// history.
pub fn engine_state_dir_for(origin: Origin) -> Result<PathBuf, PolicyError> {
    state_dir_for(origin, crate::account_home::account_home().as_deref())
}

pub(crate) fn state_dir_for(
    origin: Origin,
    account_home: Option<&Path>,
) -> Result<PathBuf, PolicyError> {
    match (origin, account_home) {
        (Origin::Account, Some(home)) => Ok(home.join(STATE_DIR_NAME).join(ENGINE_STATE_DIR_NAME)),
        _ => engine_state_dir(),
    }
}

fn default_policy_dir() -> Result<PathBuf, PolicyError> {
    Ok(state_root()?.join(POLICIES_DIR_NAME))
}

fn state_root() -> Result<PathBuf, PolicyError> {
    agent_desktop_core::configured_state_root().map_err(|error| PolicyError::State {
        message: error.to_string(),
    })
}

fn policy_files_in(dir: &Path, required: bool) -> Result<Vec<PathBuf>, PolicyError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => {
            return Ok(Vec::new());
        }
        Err(error) => {
            return Err(PolicyError::Load {
                path: dir.to_path_buf(),
                message: format!("cannot read policy directory: {error}"),
            });
        }
    };
    check_ownership(dir)?;
    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| PolicyError::Load {
            path: dir.to_path_buf(),
            message: format!("cannot read policy directory: {error}"),
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some(POLICY_FILE_EXTENSION)
            && entry.file_type().map(|t| t.is_file()).unwrap_or(false)
        {
            check_ownership(&path)?;
            files.push(path);
        }
    }
    Ok(files)
}

fn require_policy_file(path: &Path) -> Result<(), PolicyError> {
    let metadata = std::fs::metadata(path).map_err(|error| PolicyError::Load {
        path: path.to_path_buf(),
        message: format!("cannot read policy file: {error}"),
    })?;
    if !metadata.is_file() {
        return Err(PolicyError::Load {
            path: path.to_path_buf(),
            message: "policy path is not a regular file".to_string(),
        });
    }
    check_ownership(path)
}

/// Policies decide what the tool may do, so a file or folder another user
/// can rewrite is refused — the same stance the CLI takes on `TMPDIR` and
/// `AGENT_DESKTOP_HOME`.
#[cfg(unix)]
fn check_ownership(path: &Path) -> Result<(), PolicyError> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path).map_err(|error| PolicyError::Load {
        path: path.to_path_buf(),
        message: format!("cannot stat policy path: {error}"),
    })?;
    let euid = unsafe { libc::geteuid() };
    if metadata.uid() != euid {
        return Err(PolicyError::Insecure {
            path: path.to_path_buf(),
            message: "not owned by the current user".to_string(),
        });
    }
    if metadata.mode() & 0o022 != 0 {
        return Err(PolicyError::Insecure {
            path: path.to_path_buf(),
            message: "writable by group or others; chmod go-w it".to_string(),
        });
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_ownership(_path: &Path) -> Result<(), PolicyError> {
    Ok(())
}

#[cfg(test)]
#[path = "discover_tests.rs"]
mod tests;
