use agent_desktop_core::AdapterError;
use std::time::Instant;

use super::AXElement;

pub(crate) fn read(app: &AXElement, deadline: Instant) -> Result<Vec<AXElement>, AdapterError> {
    read_with_array_reader(app, deadline, |attribute| {
        super::surface_read::elements(app, attribute, deadline)
    })
}

pub(in crate::tree) fn read_with_array_reader(
    app: &AXElement,
    deadline: Instant,
    read_array: impl FnMut(&str) -> Result<Vec<AXElement>, AdapterError>,
) -> Result<Vec<AXElement>, AdapterError> {
    collect(
        read_array,
        |attribute| super::surface_read::element(app, attribute, deadline),
        |element| {
            Ok(
                super::surface_read::string(element, "AXRole", deadline)?.as_deref()
                    == Some("AXWindow"),
            )
        },
        super::same_element,
    )
}

fn collect<T>(
    mut read_array: impl FnMut(&str) -> Result<Vec<T>, AdapterError>,
    mut read_element: impl FnMut(&str) -> Result<Option<T>, AdapterError>,
    mut is_window: impl FnMut(&T) -> Result<bool, AdapterError>,
    same: impl Fn(&T, &T) -> bool,
) -> Result<Vec<T>, AdapterError> {
    let mut windows = Vec::new();
    let mut add = |candidate| -> Result<(), AdapterError> {
        if is_window(&candidate)? && !windows.iter().any(|window| same(window, &candidate)) {
            windows.push(candidate);
        }
        Ok(())
    };
    for candidate in read_array("AXWindows")? {
        add(candidate)?;
    }
    for attribute in ["AXMainWindow", "AXFocusedWindow"] {
        if let Some(candidate) = read_element(attribute)? {
            add(candidate)?;
        }
    }
    for candidate in read_array("AXChildren")? {
        add(candidate)?;
    }
    Ok(windows)
}

#[cfg(test)]
#[path = "application_windows_tests.rs"]
mod tests;
