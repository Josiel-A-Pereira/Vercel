//! The installed application an identifier would launch, resolved the way
//! `launch` resolves it (a bundle identifier through Launch Services, any
//! other name by its application file) without starting anything.

use agent_desktop_core::{AdapterError, InstalledApp};

use crate::system::appkit_bridge;

pub(crate) fn find_installed_app(identifier: &str) -> Result<Option<InstalledApp>, AdapterError> {
    if identifier.is_empty() || identifier.contains('/') || identifier.chars().any(char::is_control)
    {
        return Ok(None);
    }
    crate::system::cocoa_runtime::ensure_cocoa_multithreaded()?;
    let by_bundle_id = crate::system::launch::looks_like_bundle_id(identifier);
    let Some(bytes) = appkit_bridge::installed_app_json(identifier, by_bundle_id)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes).map(Some).map_err(|error| {
        AdapterError::internal(format!(
            "Launch Services returned an unreadable application record: {error}"
        ))
    })
}

#[cfg(test)]
#[path = "installed_app_tests.rs"]
mod tests;
