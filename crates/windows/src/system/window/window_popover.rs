//! The popover surface: a popup window an application owns beside the window
//! it was opened from.
//!
//! Chromium draws its toolbar bubbles (the Edge and Chrome extensions menu, a
//! site-permission prompt, a profile menu) as a separate `WS_POPUP` window
//! owned by the browser frame and styled as a tool window. The window census
//! drops tool windows and the frame's own UIA tree does not contain the
//! bubble, so neither `list-windows` nor `snapshot --surface window` can reach
//! it. A popover is found instead by ownership: a visible, uncloaked popup of
//! the same process whose owner chain leads to the requested window. Chromium
//! also keeps tooltip popups under the same owner; they are click-through
//! (`WS_EX_TRANSPARENT`) and take no input, so they are never candidates. Of
//! what remains, the largest window is the one rooted.

use agent_desktop_core::{AdapterError, ProcessId, Rect};

use super::window_enum::{EnumeratedWindow, WindowHandle, enumerate_top_level};
use super::window_identity::live_window_owner;

const OWNER_CHAIN_LIMIT: usize = 8;

pub(crate) fn locate_popover(owner: WindowHandle) -> Result<Option<WindowHandle>, AdapterError> {
    let Some(pid) = live_window_owner(owner) else {
        return Ok(None);
    };
    let mut candidates = Vec::new();
    enumerate_top_level(|window| {
        if window.handle != owner && is_popover_of(&window, owner, pid) {
            candidates.push((window.handle, window.rect));
        }
        true
    })?;
    Ok(largest_window(candidates))
}

pub(crate) fn largest_window(
    candidates: impl IntoIterator<Item = (WindowHandle, Rect)>,
) -> Option<WindowHandle> {
    let mut best: Option<(WindowHandle, f64)> = None;
    for (handle, rect) in candidates {
        let area = rect.width * rect.height;
        if best.is_none_or(|(_, largest)| area > largest) {
            best = Some((handle, area));
        }
    }
    best.map(|(handle, _)| handle)
}

fn is_popover_of(window: &EnumeratedWindow, owner: WindowHandle, pid: ProcessId) -> bool {
    window.visible
        && !window.cloaked
        && !window.iconic
        && !window.is_zero_sized()
        && live_window_owner(window.handle) == Some(pid)
        && is_popup(window.handle)
        && !is_click_through(window.handle)
        && owner_chain_reaches(window.handle, owner)
}

#[cfg(target_os = "windows")]
fn is_popup(handle: WindowHandle) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GWL_STYLE, GetWindowLongW, WS_POPUP};

    let style = unsafe { GetWindowLongW(handle, GWL_STYLE) } as u32;
    style & WS_POPUP != 0
}

#[cfg(not(target_os = "windows"))]
fn is_popup(_handle: WindowHandle) -> bool {
    false
}

#[cfg(target_os = "windows")]
fn is_click_through(handle: WindowHandle) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GetWindowLongW, WS_EX_TRANSPARENT,
    };

    let ex_style = unsafe { GetWindowLongW(handle, GWL_EXSTYLE) } as u32;
    ex_style & WS_EX_TRANSPARENT != 0
}

#[cfg(not(target_os = "windows"))]
fn is_click_through(_handle: WindowHandle) -> bool {
    false
}

#[cfg(target_os = "windows")]
fn owner_chain_reaches(handle: WindowHandle, owner: WindowHandle) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{GW_OWNER, GetWindow};

    let mut current = handle;
    for _ in 0..OWNER_CHAIN_LIMIT {
        current = unsafe { GetWindow(current, GW_OWNER) };
        if current.is_null() {
            return false;
        }
        if current == owner {
            return true;
        }
    }
    false
}

#[cfg(not(target_os = "windows"))]
fn owner_chain_reaches(_handle: WindowHandle, _owner: WindowHandle) -> bool {
    let _ = OWNER_CHAIN_LIMIT;
    false
}

#[cfg(test)]
#[path = "window_popover_tests.rs"]
mod tests;
