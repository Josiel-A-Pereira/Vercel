use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode};

use super::*;
use crate::system::window::foreign::foreign_fake::{
    FakePort, activated, cause, deadline, fullscreen, hyprland, ignores, maximized, minimized,
    record, target, vanishes,
};
use crate::system::window::foreign::foreign_record::ToplevelRecord;
use crate::system::x11::control::control_error::ControlError;

fn home() -> ToplevelRecord {
    record(1, "Home", "thunar")
}

fn run(port: &FakePort, op: WindowOp) -> Result<(), AdapterError> {
    apply(port, &target("Home"), &op, deadline())
}

#[test]
fn move_is_refused_before_the_compositor_is_asked_anything() {
    let port = FakePort::showing(vec![home()]);
    let error = run(&port, WindowOp::Move { x: 1.0, y: 2.0 }).unwrap_err();
    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert_eq!(cause(&error), "wayland_session");
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert_eq!(port.refreshes(), 0);
    assert!(port.sent().is_empty());
}

#[test]
fn resize_is_refused_before_the_compositor_is_asked_anything() {
    let port = FakePort::showing(vec![home()]);
    let error = run(
        &port,
        WindowOp::Resize {
            width: 3.0,
            height: 4.0,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert_eq!(port.refreshes(), 0);
    assert!(port.sent().is_empty());
}

#[test]
fn minimize_that_never_sets_the_flag_is_minimize_not_applied_and_unverified() {
    let port = FakePort::showing(vec![home()]).reacting(hyprland);
    let error = run(&port, WindowOp::Minimize).unwrap_err();
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(cause(&error), "minimize_not_applied");
    assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
    assert_eq!(port.sent(), vec![(1, ToplevelRequest::SetMinimized(true))]);
}

#[test]
fn minimize_that_sets_the_flag_succeeds() {
    let port = FakePort::showing(vec![home()]);
    assert!(run(&port, WindowOp::Minimize).is_ok());
    assert!(port.records()[0].flags.minimized);
}

#[test]
fn minimizing_a_window_that_is_already_minimized_sends_nothing() {
    let port = FakePort::showing(vec![minimized(home())]);
    assert!(run(&port, WindowOp::Minimize).is_ok());
    assert!(port.sent().is_empty());
}

#[test]
fn maximize_is_verified_by_the_maximized_flag() {
    let port = FakePort::showing(vec![home()]);
    assert!(run(&port, WindowOp::Maximize).is_ok());
    assert_eq!(port.sent(), vec![(1, ToplevelRequest::SetMaximized(true))]);
    assert!(port.records()[0].flags.maximized);
}

#[test]
fn maximize_that_changes_nothing_is_a_readback_mismatch_with_the_observed_state() {
    let port = FakePort::showing(vec![home()]).reacting(ignores);
    let error = run(&port, WindowOp::Maximize).unwrap_err();
    assert_eq!(cause(&error), "readback_mismatch");
    assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
    let details = error.details.expect("details");
    assert_eq!(details["requested"]["maximized"], true);
    assert_eq!(details["observed"]["maximized"], false);
}

#[test]
fn fullscreen_does_not_count_as_maximized() {
    let port = FakePort::showing(vec![fullscreen(home())]).reacting(ignores);
    let error = run(&port, WindowOp::Maximize).unwrap_err();
    assert_eq!(cause(&error), "readback_mismatch");
    assert_eq!(port.sent(), vec![(1, ToplevelRequest::SetMaximized(true))]);
}

#[test]
fn maximizing_a_maximized_window_sends_nothing() {
    let port = FakePort::showing(vec![maximized(home())]);
    assert!(run(&port, WindowOp::Maximize).is_ok());
    assert!(port.sent().is_empty());
}

#[test]
fn restore_from_minimized_unminimizes_only() {
    let port = FakePort::showing(vec![minimized(home())]);
    assert!(run(&port, WindowOp::Restore).is_ok());
    assert_eq!(port.sent(), vec![(1, ToplevelRequest::SetMinimized(false))]);
    assert!(!port.records()[0].flags.minimized);
}

#[test]
fn restore_from_maximized_unmaximizes_only() {
    let port = FakePort::showing(vec![maximized(home())]);
    assert!(run(&port, WindowOp::Restore).is_ok());
    assert_eq!(port.sent(), vec![(1, ToplevelRequest::SetMaximized(false))]);
}

#[test]
fn restore_from_both_sends_unminimize_then_unmaximize_to_the_same_window() {
    let port = FakePort::showing(vec![maximized(minimized(home()))]);
    assert!(run(&port, WindowOp::Restore).is_ok());
    assert_eq!(
        port.sent(),
        vec![
            (1, ToplevelRequest::SetMinimized(false)),
            (1, ToplevelRequest::SetMaximized(false)),
        ]
    );
    let after = port.records()[0].flags;
    assert!(!after.minimized && !after.maximized);
}

#[test]
fn restore_from_neither_sends_nothing() {
    let port = FakePort::showing(vec![activated(home())]);
    assert!(run(&port, WindowOp::Restore).is_ok());
    assert!(port.sent().is_empty());
}

#[test]
fn restore_does_not_prove_the_window_again_between_its_steps() {
    let twin = record(2, "Home", "thunar");
    let port = FakePort::showing(Vec::new())
        .then_showing(vec![maximized(minimized(home()))])
        .then_showing(vec![maximized(home()), twin.clone()])
        .then_showing(vec![home(), twin]);
    assert!(run(&port, WindowOp::Restore).is_ok());
    assert_eq!(
        port.sent(),
        vec![
            (1, ToplevelRequest::SetMinimized(false)),
            (1, ToplevelRequest::SetMaximized(false)),
        ]
    );
}

#[test]
fn restore_that_loses_the_window_after_the_first_step_is_window_destroyed() {
    let port = FakePort::showing(Vec::new())
        .then_showing(vec![maximized(minimized(home()))])
        .then_showing(Vec::new());
    let error = run(&port, WindowOp::Restore).unwrap_err();
    assert_eq!(error.code, ErrorCode::WindowNotFound);
    assert_eq!(cause(&error), "window_destroyed");
    assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
    assert_eq!(port.sent(), vec![(1, ToplevelRequest::SetMinimized(false))]);
}

#[test]
fn restore_whose_window_vanishes_while_unminimizing_is_window_destroyed() {
    let port = FakePort::showing(vec![minimized(home())]).reacting(vanishes);
    let error = run(&port, WindowOp::Restore).unwrap_err();
    assert_eq!(cause(&error), "window_destroyed");
}

#[test]
fn an_op_on_two_same_title_windows_is_ambiguous_and_sends_nothing() {
    let port = FakePort::showing(vec![home(), record(2, "Home", "thunar")]);
    let error = run(&port, WindowOp::Maximize).unwrap_err();
    assert_eq!(cause(&error), "ambiguous_window");
    assert!(port.sent().is_empty());
}

#[test]
fn a_failed_flush_is_uncertain_and_not_verified() {
    let port = FakePort::showing(vec![home()]).failing_send(ControlError::SendFailed);
    let error = run(&port, WindowOp::Maximize).unwrap_err();
    assert_eq!(error.disposition, DeliverySemantics::uncertain());
}
