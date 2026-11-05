use agent_desktop_core::{Point, ProcessId, Rect, WindowInfo, WindowState};

use super::receiver_among;

fn window(app: &str, bounds: (f64, f64, f64, f64), minimized: bool) -> WindowInfo {
    WindowInfo {
        id: format!("{app}-{}", bounds.0),
        title: app.to_string(),
        app: app.to_string(),
        pid: ProcessId::from(4242),
        process_instance: None,
        bounds: Some(Rect {
            x: bounds.0,
            y: bounds.1,
            width: bounds.2,
            height: bounds.3,
        }),
        state: WindowState {
            is_focused: false,
            accessible: true,
            minimized: Some(minimized),
            visible: Some(true),
        },
    }
}

const POINT: Point = Point { x: 50.0, y: 50.0 };

#[test]
fn no_window_under_the_point_is_the_desktop() {
    let windows = vec![window("Files", (200.0, 200.0, 100.0, 100.0), false)];
    assert!(receiver_among(windows, &POINT).expect("decides").is_none());
}

#[test]
fn one_application_under_the_point_receives_it() {
    let windows = vec![
        window("Files", (0.0, 0.0, 100.0, 100.0), false),
        window("Files", (10.0, 10.0, 100.0, 100.0), false),
        window("Terminal", (500.0, 500.0, 100.0, 100.0), false),
    ];
    let receiver = receiver_among(windows, &POINT).expect("decides");
    assert_eq!(receiver.map(|window| window.app).as_deref(), Some("Files"));
}

#[test]
fn several_applications_overlapping_the_point_are_not_guessed_between() {
    let windows = vec![
        window("Files", (0.0, 0.0, 100.0, 100.0), false),
        window("Vault", (20.0, 20.0, 100.0, 100.0), false),
    ];
    assert!(receiver_among(windows, &POINT).is_err());
}

#[test]
fn minimized_windows_receive_nothing() {
    let windows = vec![
        window("Vault", (0.0, 0.0, 100.0, 100.0), true),
        window("Files", (0.0, 0.0, 100.0, 100.0), false),
    ];
    let receiver = receiver_among(windows, &POINT).expect("decides");
    assert_eq!(receiver.map(|window| window.app).as_deref(), Some("Files"));
}
