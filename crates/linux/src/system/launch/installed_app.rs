//! The installed application an identifier would launch, found the way
//! `launch` finds a desktop entry: by desktop id first, then by its exact
//! `Name` in any letter case. The desktop id is the identifier a running
//! process of that entry is listed under, so a policy on it holds before and
//! after launch.

use agent_desktop_core::InstalledApp;

use super::desktop_entry::parse;
use super::desktop_lookup::find;
use super::launch_search::Search;
use crate::system::app::app_alias::current_index;

pub(crate) fn find_installed_app(identifier: &str) -> Option<InstalledApp> {
    let search = Search::from_env();
    find_installed_app_in(identifier, &search, &current_index())
}

pub(crate) fn find_installed_app_in(
    identifier: &str,
    search: &Search,
    index: &crate::system::app::app_alias::AliasIndex,
) -> Option<InstalledApp> {
    if identifier.is_empty() || identifier.starts_with('/') {
        return None;
    }
    let id = match find(identifier, &search.data_dirs) {
        Some(file) => file.file_stem()?.to_str()?.to_string(),
        None => {
            let mut ids = index.ids_named(identifier);
            if ids.len() != 1 {
                return None;
            }
            ids.remove(0)
        }
    };
    let text = std::fs::read_to_string(find(&id, &search.data_dirs)?).ok()?;
    let entry = parse(&id, &text)?;
    Some(InstalledApp {
        name: entry.name,
        bundle_id: Some(id),
    })
}

#[cfg(test)]
#[path = "installed_app_tests.rs"]
mod tests;
