//! The installed program an identifier would launch, resolved the way
//! `launch` resolves it, and named the way a running process of it is
//! listed: by its image file name in its on-disk letter case. Windows has
//! no bundle identifier, so the record carries none.

use agent_desktop_core::InstalledApp;

use super::launch_path::{launch_identifier, resolve_executable};

pub(crate) fn find_installed_app(identifier: &str) -> Option<InstalledApp> {
    let id = launch_identifier(identifier).ok()?;
    let path = resolve_executable(&id).ok()?;
    let on_disk = std::fs::canonicalize(&path).unwrap_or(path);
    Some(InstalledApp {
        name: on_disk.file_name()?.to_str()?.to_string(),
        bundle_id: None,
    })
}

#[cfg(test)]
#[path = "installed_app_tests.rs"]
mod tests;
