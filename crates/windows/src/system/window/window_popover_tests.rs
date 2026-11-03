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
