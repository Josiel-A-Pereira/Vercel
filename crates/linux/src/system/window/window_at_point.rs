//! Which application window receives pointer input at a screen point.
//!
//! AT-SPI reports window frames but no stacking order, and the X11
//! stacking order is unavailable on a pure Wayland session, so overlap
//! cannot be settled in general. When every listed window under the point
//! belongs to one application, that application receives the input
//! whichever of its windows is on top. When windows of several
//! applications overlap there, the receiver is not provable and the answer
//! is an error rather than a guess.

use agent_desktop_core::{AdapterError, Deadline, ErrorCode, Point, WindowFilter, WindowInfo};

use crate::system::bus::SharedBus;

pub(crate) fn window_at_point(
    bus: &SharedBus,
    point: &Point,
    deadline: Deadline,
) -> Result<Option<WindowInfo>, AdapterError> {
    let windows = super::window_ops::list_windows(bus, &WindowFilter::default(), deadline)?;
    receiver_among(windows, point)
}

pub(crate) fn receiver_among(
    windows: Vec<WindowInfo>,
    point: &Point,
) -> Result<Option<WindowInfo>, AdapterError> {
    let mut under_point = windows.into_iter().filter(|window| {
        window.state.minimized != Some(true)
            && window.state.visible != Some(false)
            && window.bounds.as_ref().is_some_and(|bounds| {
                point.x >= bounds.x
                    && point.y >= bounds.y
                    && point.x < bounds.x + bounds.width
                    && point.y < bounds.y + bounds.height
            })
    });
    let Some(first) = under_point.next() else {
        return Ok(None);
    };
    if let Some(other) = under_point.find(|window| window.app != first.app) {
        return Err(AdapterError::new(
            ErrorCode::ActionFailed,
            format!(
                "windows of {} and {} overlap ({}, {}) and the stacking order is unknown",
                first.app, other.app, point.x, point.y
            ),
        ));
    }
    Ok(Some(first))
}

#[cfg(test)]
#[path = "window_at_point_tests.rs"]
mod tests;
