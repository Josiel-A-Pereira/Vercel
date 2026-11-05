use super::*;

fn handle(value: usize) -> WindowHandle {
    value as WindowHandle
}

fn rect(width: f64, height: f64) -> Rect {
    Rect {
        x: 0.0,
        y: 0.0,
        width,
        height,
    }
}

#[test]
fn no_candidate_roots_nothing() {
    assert!(largest_window(Vec::new()).is_none());
}

#[test]
fn the_bubble_wins_over_the_tooltip_it_owns() {
    let tooltip = (handle(1), rect(136.0, 25.0));
    let bubble = (handle(2), rect(384.0, 678.0));
    assert_eq!(largest_window(vec![tooltip, bubble]), Some(handle(2)));
    assert_eq!(largest_window(vec![bubble, tooltip]), Some(handle(2)));
}

#[test]
fn an_equal_area_keeps_the_first_window_enumerated() {
    let first = (handle(1), rect(100.0, 100.0));
    let second = (handle(2), rect(100.0, 100.0));
    assert_eq!(largest_window(vec![first, second]), Some(handle(1)));
}

#[cfg(target_os = "windows")]
#[test]
fn a_window_that_does_not_exist_owns_no_popover() {
    assert!(matches!(locate_popover(std::ptr::null_mut()), Ok(None)));
}

#[cfg(target_os = "windows")]
#[test]
fn an_owned_tool_popup_roots_and_a_hidden_or_click_through_popup_does_not() {
    use crate::tree::fixture::{bootstrap, fixture_popover::PopoverFixture};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, SW_HIDE, SW_SHOWNOACTIVATE, SetWindowLongW, ShowWindow, WS_EX_TOOLWINDOW,
        WS_EX_TRANSPARENT,
    };

    bootstrap();
    let fixture = PopoverFixture::create().expect("the popover fixture opens");
    let owner = fixture.owner_handle() as WindowHandle;
    let popup = fixture.popup_handle() as WindowHandle;
    assert_eq!(locate_popover(owner).unwrap(), Some(popup));
    assert_eq!(locate_popover(popup).unwrap(), None);

    unsafe { ShowWindow(popup, SW_HIDE) };
    assert_eq!(locate_popover(owner).unwrap(), None);
    unsafe { ShowWindow(popup, SW_SHOWNOACTIVATE) };
    assert_eq!(locate_popover(owner).unwrap(), Some(popup));
    unsafe {
        SetWindowLongW(
            popup,
            GWL_EXSTYLE,
            (WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT) as i32,
        );
    }
    assert_eq!(locate_popover(owner).unwrap(), None);
}
