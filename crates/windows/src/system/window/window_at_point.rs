//! Which application window receives pointer input at a screen point.
//!
//! `WindowFromPoint` names the topmost window under the point, which is the
//! one a click there reaches. Its root window is matched against the window
//! inventory, so the application name is the one every other command
//! reports. A root the inventory does not list (the desktop, the taskbar) is
//! named by the process that owns it.

use agent_desktop_core::{
    AdapterError, Deadline, ErrorCode, ObservationOps, Point, WindowFilter, WindowInfo, WindowState,
};

use crate::adapter::WindowsAdapter;
use crate::tree::hit_test::corroborate::{pid_of_hwnd, window_from_point_root};

pub(crate) fn window_at_point(
    adapter: &WindowsAdapter,
    point: &Point,
    deadline: Deadline,
) -> Result<Option<WindowInfo>, AdapterError> {
    let Some(root) = window_from_point_root(point) else {
        return Ok(None);
    };
    let id = format!("w-{}", root as usize);
    if let Some(window) = adapter
        .list_windows(&WindowFilter::default(), deadline)?
        .into_iter()
        .find(|window| window.id == id)
    {
        return Ok(Some(window));
    }
    let pid = pid_of_hwnd(root).ok_or_else(|| unowned(point))?;
    let app = adapter
        .list_apps(deadline)?
        .into_iter()
        .find(|app| app.pid == pid)
        .ok_or_else(|| unowned(point))?;
    Ok(Some(WindowInfo {
        id,
        title: String::new(),
        app: app.name,
        pid: app.pid,
        process_instance: app.process_instance,
        bounds: None,
        state: WindowState {
            is_focused: false,
            accessible: false,
            minimized: None,
            visible: Some(true),
        },
    }))
}

fn unowned(point: &Point) -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionFailed,
        format!(
            "the window under ({}, {}) belongs to no listed application",
            point.x, point.y
        ),
    )
}

#[cfg(test)]
#[path = "window_at_point_tests.rs"]
mod tests;
