use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use crate::{AppError, Deadline, ProcessIdentity, session, storage};

use super::CommandContext;

const MAX_TARGET_BYTES: u64 = 1024;

impl CommandContext {
    pub fn invalidate_target(&self) -> Result<(), AppError> {
        self.remember_process(None)
    }

    pub fn remember_app(&self, app: &crate::AppInfo) -> Result<(), AppError> {
        let process = app
            .process_instance
            .as_deref()
            .filter(|instance| !instance.trim().is_empty() && app.pid.get() != 0)
            .map(|instance| ProcessIdentity::new(app.pid, instance));
        self.remember_process(process)
    }

    pub fn remember_window(&self, window: &crate::WindowInfo) -> Result<(), AppError> {
        let process = window
            .process_instance
            .as_deref()
            .filter(|instance| !instance.trim().is_empty() && window.pid.get() != 0)
            .map(|instance| ProcessIdentity::new(window.pid, instance));
        self.remember_process(process)
    }

    pub fn remember_process(&self, process: Option<ProcessIdentity>) -> Result<(), AppError> {
        let Some(path) = target_path(self)? else {
            return Ok(());
        };
        if let Some(process) = &process {
            validate_process(process)?;
        }
        let bytes = serde_json::to_vec(&process)?;
        if bytes.len() as u64 > MAX_TARGET_BYTES {
            return Err(AppError::invalid_input(
                "Remembered process target exceeds its size limit",
            ));
        }
        let _lock = target_lock(&path)?;
        storage::private_file::write_atomic(&path, &bytes)?;
        Ok(())
    }

    pub fn remembered_process(&self) -> Result<Option<ProcessIdentity>, AppError> {
        let Some(path) = target_path(self)? else {
            return Ok(None);
        };
        let _lock = target_lock(&path)?;
        let process = match storage::private_file::read_private_bounded(&path, MAX_TARGET_BYTES) {
            Ok(bytes) => serde_json::from_slice(&bytes)?,
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if let Some(process) = &process {
            validate_process(process)?;
        }
        Ok(process)
    }
}

fn target_path(context: &CommandContext) -> Result<Option<PathBuf>, AppError> {
    let session_id = context.session_id();
    let agent_id = context.agent_id();
    if session_id.is_none() && agent_id.is_none() {
        return Ok(None);
    }
    if let Some(id) = session_id {
        super::validate_session_id(id)?;
    }
    if let Some(id) = agent_id {
        session::validate_agent_id(id)?;
    }
    let root = storage::state_root::resolve_configured_state_root()?;
    let directory = match (session_id, agent_id) {
        (Some(session), Some(agent)) => root
            .join("sessions")
            .join(session)
            .join("remembered-target")
            .join("agents")
            .join(agent),
        (Some(session), None) => root
            .join("sessions")
            .join(session)
            .join("remembered-target"),
        (None, Some(agent)) => root.join("agents").join(agent).join("remembered-target"),
        (None, None) => return Ok(None),
    };
    storage::private_file_parent::ensure_private(&directory)?;
    Ok(Some(directory.join("target.json")))
}

fn target_lock(path: &Path) -> Result<storage::file_lock::FileLock, AppError> {
    Ok(storage::file_lock::FileLock::acquire(
        &path.with_extension("lock"),
        Deadline::after(2_000)?,
        "remembered process target",
    )?)
}

fn validate_process(process: &ProcessIdentity) -> Result<(), AppError> {
    if process.pid.get() == 0 || process.instance.trim().is_empty() {
        return Err(AppError::invalid_input(
            "Remembered process target requires a positive pid and process instance",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "remembered_target_tests.rs"]
mod tests;
