use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics, ErrorCode};

use crate::tree::AXElement;

pub(super) struct FocusSnapshot {
    frontmost_pid: i32,
    frontmost_launch_time: Option<f64>,
    application: AXElement,
    window: Option<AXElement>,
    focused: Option<AXElement>,
}

/// Reads on-screen windows to initialize this process's WindowServer connection without activating a window.
pub(super) fn initialize_window_server_connection() -> Result<(), AdapterError> {
    use core_graphics::display::CGDisplay;
    use core_graphics::window::kCGWindowListOptionOnScreenOnly;

    CGDisplay::window_list_info(kCGWindowListOptionOnScreenOnly, None)
        .map(drop)
        .ok_or_else(|| unknown_focus("Could not initialize the WindowServer connection"))
}

impl FocusSnapshot {
    pub(super) fn matches(&self, other: &Self) -> bool {
        same_frontmost(self, other)
            && crate::tree::same_element(&self.application, &other.application)
            && same_optional_element(&self.window, &other.window)
            && same_optional_element(&self.focused, &other.focused)
    }

    fn focus_changed(&self, other: &Self) -> bool {
        !same_optional_element(&self.focused, &other.focused)
    }

    fn activated(&self, other: &Self) -> bool {
        !same_frontmost(self, other)
            || !crate::tree::same_element(&self.application, &other.application)
            || !same_optional_element(&self.window, &other.window)
    }
}

pub(super) fn read_global_focus(deadline: Deadline) -> Result<FocusSnapshot, AdapterError> {
    use accessibility_sys::AXUIElementCreateSystemWide;

    deadline.ensure_remaining()?;
    initialize_window_server_connection()?;
    deadline.ensure_remaining()?;
    let frontmost_before = read_frontmost(deadline)?;
    let system = AXElement(unsafe { AXUIElementCreateSystemWide() });
    let application = crate::tree::element_for_pid(frontmost_before.pid);
    let window = read_element(&application, "AXFocusedWindow", deadline)?;
    let focused = read_element(&system, "AXFocusedUIElement", deadline)?;
    let frontmost_after = read_frontmost(deadline)?;
    if !same_owner(&frontmost_before, &frontmost_after) {
        return Err(unknown_focus(
            "NSWorkspace frontmost app changed during focus capture",
        ));
    }
    Ok(FocusSnapshot {
        frontmost_pid: frontmost_after.pid,
        frontmost_launch_time: frontmost_after.launch_time,
        application,
        window,
        focused,
    })
}

fn read_frontmost(
    deadline: Deadline,
) -> Result<crate::system::app::workspace_apps::WindowOwner, AdapterError> {
    let until = crate::tree::locator_deadline::from_operation(deadline)?;
    crate::system::app::workspace_apps::window_owner_snapshot_until(until)?
        .frontmost()
        .cloned()
        .ok_or_else(|| unknown_focus("NSWorkspace frontmost application is unknown"))
}

fn same_optional_element(before: &Option<AXElement>, after: &Option<AXElement>) -> bool {
    match (before, after) {
        (Some(before), Some(after)) => crate::tree::same_element(before, after),
        (None, None) => true,
        _ => false,
    }
}

fn same_frontmost(before: &FocusSnapshot, after: &FocusSnapshot) -> bool {
    before.frontmost_pid == after.frontmost_pid
        && same_launch_time(before.frontmost_launch_time, after.frontmost_launch_time)
}

fn same_owner(
    before: &crate::system::app::workspace_apps::WindowOwner,
    after: &crate::system::app::workspace_apps::WindowOwner,
) -> bool {
    before.pid == after.pid && same_launch_time(before.launch_time, after.launch_time)
}

fn same_launch_time(before: Option<f64>, after: Option<f64>) -> bool {
    match (before, after) {
        (Some(before), Some(after)) => before.to_bits() == after.to_bits(),
        (None, None) => true,
        _ => false,
    }
}

fn unknown_focus(message: &str) -> AdapterError {
    AdapterError::new(ErrorCode::ActionFailed, message)
        .with_disposition(DeliverySemantics::not_delivered())
}

fn read_element(
    element: &AXElement,
    attribute: &str,
    deadline: Deadline,
) -> Result<Option<AXElement>, AdapterError> {
    crate::tree::properties::attributes::copy_element_attr_result(element, attribute, deadline)
        .map_err(|error| {
            crate::system::focus::map_ax_read_error(
                error,
                &format!("read {attribute} while capturing global focus"),
            )
        })
}

pub(super) fn preflight_focus_error(before: &FocusSnapshot, after: &FocusSnapshot) -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionFailed,
        "Global focus changed before selection",
    )
    .with_details(focus_details(before, Some(after)))
    .with_disposition(DeliverySemantics::not_delivered())
}

pub(super) fn focus_changed_error(before: &FocusSnapshot, after: &FocusSnapshot) -> AdapterError {
    after_write_error(
        AdapterError::new(
            ErrorCode::ActionFailed,
            "Text selection changed the frontmost app, active window, or focused element",
        ),
        before,
        Some(after),
    )
}

pub(super) fn after_write_error(
    error: AdapterError,
    before: &FocusSnapshot,
    after: Option<&FocusSnapshot>,
) -> AdapterError {
    let mut details = error
        .details
        .clone()
        .unwrap_or_else(|| serde_json::json!({}));
    if !details.is_object() {
        details = serde_json::json!({ "cause": details });
    }
    let focus = focus_details(before, after);
    if let (Some(object), Some(focus)) = (details.as_object_mut(), focus.as_object()) {
        object.extend(focus.clone());
    }
    crate::actions::DeliveryTracker::uncertain(error.with_details(details))
}

pub(super) fn after_uncertain_write(
    error: AdapterError,
    before: &FocusSnapshot,
    read_after: impl FnOnce() -> Result<FocusSnapshot, AdapterError>,
) -> AdapterError {
    if error.disposition != DeliverySemantics::uncertain() {
        return error;
    }
    match read_after() {
        Ok(after) => after_write_error(error, before, Some(&after)),
        Err(_) => after_write_error(error, before, None),
    }
}

fn focus_details(before: &FocusSnapshot, after: Option<&FocusSnapshot>) -> serde_json::Value {
    serde_json::json!({
        "verification_scope": "text_selection",
        "focus_changed": after.map(|state| before.focus_changed(state)),
        "activated": after.map(|state| before.activated(state)),
        "focus_observation": if after.is_some() { "complete" } else { "unknown" },
    })
}

#[cfg(test)]
#[path = "select_text_focus_tests.rs"]
mod tests;
