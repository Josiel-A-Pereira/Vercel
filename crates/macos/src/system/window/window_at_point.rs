//! Which application window receives pointer input at a screen point.
//!
//! CoreGraphics lists on-screen windows front to back across every layer
//! (only the on-screen listing is in that order),
//! so the first window containing the point is the one on top there: an
//! app's popup menu or floating panel as much as its document window, and
//! the Dock as itself. The menu bar belongs to the window server, but a
//! click there opens the frontmost application's menus, so it is judged as
//! that application's focused window. Fully transparent windows and
//! agent-desktop's own overlay windows, which ignore mouse events, let the
//! pointer through and are skipped.

use std::path::{Path, PathBuf};
use std::time::Instant;

use agent_desktop_core::{AdapterError, Point, Rect, WindowInfo};

use super::cg_window::{
    WindowDictionary, WindowRecord, bool_field, dictionaries_for, ensure_before_deadline,
    int_field, int_or_float_field, rect_field, string_field,
};

/// `kCGMainMenuWindowLevel`: the menu bar strip.
pub(super) const MAIN_MENU_LAYER: i64 = 24;

pub(crate) fn window_at_point_until(
    point: &Point,
    deadline: Instant,
) -> Result<Option<WindowInfo>, AdapterError> {
    ensure_before_deadline(deadline)?;
    let own_executable = std::env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok());
    for dictionary in stacked_window_dictionaries()? {
        let Some(record) = receiving_record(&dictionary, point, own_executable.as_deref())? else {
            continue;
        };
        ensure_before_deadline(deadline)?;
        if int_field(&dictionary, "kCGWindowLayer") == Some(MAIN_MENU_LAYER) {
            return super::window_inventory::focused_window_until(deadline);
        }
        let process_instance = crate::system::process::process_identity::token_for_pid(record.pid)?;
        return WindowRecord {
            process_instance,
            ..record
        }
        .into_window_info(false, None, true)
        .map(Some);
    }
    Ok(None)
}

fn receiving_record(
    dictionary: &WindowDictionary,
    point: &Point,
    own_executable: Option<&Path>,
) -> Result<Option<WindowRecord>, AdapterError> {
    if !bool_field(dictionary, "kCGWindowIsOnscreen")? {
        return Ok(None);
    }
    let Some(bounds) = rect_field(dictionary, "kCGWindowBounds") else {
        return Ok(None);
    };
    if !contains(&bounds, point) || int_or_float_field(dictionary, "kCGWindowAlpha") == Some(0.0) {
        return Ok(None);
    }
    let Some(pid) = int_field(dictionary, "kCGWindowOwnerPID")
        .and_then(|pid| i32::try_from(pid).ok())
        .filter(|pid| *pid > 0)
    else {
        return Ok(None);
    };
    if runs_executable(pid, own_executable) {
        return Ok(None);
    }
    let app_name = string_field(dictionary, "kCGWindowOwnerName").unwrap_or_default();
    if app_name.is_empty() {
        return Err(AdapterError::new(
            agent_desktop_core::ErrorCode::ActionFailed,
            "the window under the point has no owning application name",
        ));
    }
    Ok(Some(WindowRecord {
        app_name,
        pid,
        title: string_field(dictionary, "kCGWindowName").filter(|title| !title.is_empty()),
        window_number: int_field(dictionary, "kCGWindowNumber").unwrap_or_default(),
        bounds,
        visible: true,
        process_instance: None,
    }))
}

/// The windows on screen, front to back. Only the on-screen listing is in
/// stacking order; the full listing that includes off-screen windows is not.
pub(super) fn stacked_window_dictionaries() -> Result<Vec<WindowDictionary>, AdapterError> {
    dictionaries_for(
        core_graphics::window::kCGWindowListOptionOnScreenOnly
            | core_graphics::window::kCGWindowListExcludeDesktopElements,
    )
}

pub(crate) fn contains(bounds: &Rect, point: &Point) -> bool {
    point.x >= bounds.x
        && point.y >= bounds.y
        && point.x < bounds.x + bounds.width
        && point.y < bounds.y + bounds.height
}

fn runs_executable(pid: i32, executable: Option<&Path>) -> bool {
    let Some(executable) = executable else {
        return false;
    };
    crate::system::renderer_kind::executable_path(pid)
        .and_then(|path: PathBuf| path.canonicalize().ok())
        .is_some_and(|path| path == executable)
}

#[cfg(test)]
#[path = "window_at_point_tests.rs"]
mod tests;
