#[cfg(target_os = "windows")]
mod windows_only {
    use agent_desktop_core::{ObservationOps, Point, ProcessId, SystemOps, WindowFilter};

    use crate::adapter::WindowsAdapter;
    use crate::system::fixture::test_time::deadline;

    use super::super::window_at_point;

    /// A freshly shown fixture window is on top over its whole on-screen
    /// area, so a hit test at the centre of that area must answer with the
    /// fixture's window and process. A fixture placed entirely off every
    /// display (a small virtual screen) proves nothing either way: what
    /// `WindowFromPoint` returns off-screen is unspecified.
    #[test]
    fn a_fixture_window_receives_the_centre_of_its_visible_area() {
        crate::tree::fixture::ensure_test_apartment();
        let fixture = crate::tree::fixture::HostedFixture::spawn().expect("a fixture host starts");
        bring_on_screen_and_on_top(fixture.handle());
        let adapter = WindowsAdapter::new();
        let listed = adapter
            .list_windows(&WindowFilter::default(), deadline(10_000))
            .expect("windows list")
            .into_iter()
            .find(|window| window.pid == ProcessId::from(fixture.process_id()))
            .expect("the fixture window is listed");
        let window = listed.bounds.expect("a listed window has bounds");
        let Some(visible) = adapter
            .list_displays(deadline(10_000))
            .expect("displays list")
            .into_iter()
            .find_map(|display| crate::tree::hit_test::intersect_rects(window, display.bounds))
        else {
            return;
        };
        let centre = Point {
            x: visible.x + visible.width / 2.0,
            y: visible.y + visible.height / 2.0,
        };

        let receiver = window_at_point(&adapter, &centre, deadline(10_000))
            .expect("hit test")
            .expect("a window is under the fixture's visible centre");
        assert_eq!(receiver.id, listed.id);
        assert_eq!(receiver.app, listed.app);
    }

    /// A fixture can open beyond a small virtual screen; move it to the
    /// top-left corner and above every other window, so the hit test below
    /// has a definite answer.
    fn bring_on_screen_and_on_top(handle: isize) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            HWND_TOPMOST, SWP_NOSIZE, SWP_SHOWWINDOW, SetWindowPos,
        };
        let moved = unsafe {
            SetWindowPos(
                handle as *mut std::ffi::c_void,
                HWND_TOPMOST,
                40,
                40,
                0,
                0,
                SWP_NOSIZE | SWP_SHOWWINDOW,
            )
        };
        assert_ne!(moved, 0, "the fixture window moves on screen");
    }

    #[test]
    fn a_point_beyond_every_display_has_no_receiving_window() {
        let adapter = WindowsAdapter::new();
        let receiver = window_at_point(
            &adapter,
            &Point {
                x: -9_000_000.0,
                y: -9_000_000.0,
            },
            deadline(10_000),
        )
        .expect("hit test");
        assert!(receiver.is_none(), "{receiver:?}");
    }
}
