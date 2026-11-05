use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode};

use super::*;
use crate::system::window::foreign::foreign_fake::{
    FakePort, activated, cause, deadline, ignores, record, target, vanishes,
};
use crate::system::window::foreign::foreign_record::ToplevelRecord;
use crate::system::x11::control::control_error::ControlError;

fn run(port: &FakePort) -> Result<(), AdapterError> {
    activate(port, &target("Home"), deadline())
}

fn home(id: u32) -> ToplevelRecord {
    record(id, "Home", "thunar")
}

#[test]
fn an_already_active_window_is_left_alone_and_nothing_is_sent() {
    let port = FakePort::showing(vec![activated(home(1))]);
    assert!(run(&port).is_ok());
    assert!(port.sent().is_empty());
    assert_eq!(port.refreshes(), 1);
}

#[test]
fn a_proven_window_gets_activate_and_is_verified_by_the_activated_state() {
    let port = FakePort::showing(vec![home(1), activated(record(2, "Other", "thunar"))]);
    assert!(run(&port).is_ok());
    assert_eq!(port.sent(), vec![(1, ToplevelRequest::Activate)]);
    let after = port.records();
    assert!(after[0].flags.activated && !after[1].flags.activated);
}

#[test]
fn a_window_that_never_becomes_active_is_activation_refused_and_unverified() {
    let port = FakePort::showing(vec![home(1)]).reacting(ignores);
    let error = run(&port).unwrap_err();
    assert_eq!(cause(&error), "activation_refused");
    assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
    assert_eq!(port.sent(), vec![(1, ToplevelRequest::Activate)]);
}

#[test]
fn a_window_that_closes_while_waiting_is_window_destroyed() {
    let port = FakePort::showing(vec![home(1)]).reacting(vanishes);
    let error = run(&port).unwrap_err();
    assert_eq!(error.code, ErrorCode::WindowNotFound);
    assert_eq!(cause(&error), "window_destroyed");
    assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
}

#[test]
fn two_related_windows_with_the_title_are_ambiguous_and_nothing_is_sent() {
    let port = FakePort::showing(vec![home(1), home(2)]);
    let error = run(&port).unwrap_err();
    assert_eq!(error.code, ErrorCode::AmbiguousTarget);
    assert_eq!(cause(&error), "ambiguous_window");
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert!(port.sent().is_empty());
}

#[test]
fn a_title_nobody_has_is_not_mapped_and_nothing_is_sent() {
    let port = FakePort::showing(vec![record(1, "Away", "thunar")]);
    let error = run(&port).unwrap_err();
    assert_eq!(error.code, ErrorCode::WindowNotFound);
    assert_eq!(cause(&error), "window_not_mapped");
    assert!(port.sent().is_empty());
}

#[test]
fn a_title_held_only_by_another_program_is_unproven_and_nothing_is_sent() {
    let port = FakePort::showing(vec![record(1, "Home", "kitty")]);
    let error = run(&port).unwrap_err();
    assert_eq!(cause(&error), "toplevel_identity_unproven");
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert!(port.sent().is_empty());
}

#[test]
fn an_unrelated_window_with_the_same_title_is_unproven() {
    let port = FakePort::showing(vec![record(1, "Home", "kitty"), home(2)]);
    let error = run(&port).unwrap_err();
    assert_eq!(cause(&error), "toplevel_identity_unproven");
    assert!(port.sent().is_empty());
}

#[test]
fn a_refresh_that_times_out_is_a_timeout_and_nothing_is_sent() {
    let port = FakePort::showing(vec![home(1)]).failing_refresh(ControlError::TimedOut);
    let error = run(&port).unwrap_err();
    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert!(port.sent().is_empty());
}

#[test]
fn a_broken_connection_means_no_display_and_nothing_is_sent() {
    let port = FakePort::showing(vec![home(1)]).failing_refresh(ControlError::NoDisplay);
    let error = run(&port).unwrap_err();
    assert_eq!(cause(&error), "no_display");
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
}

#[test]
fn a_seatless_session_cannot_activate_and_says_nothing_was_sent() {
    let port = FakePort::showing(vec![home(1)]).failing_send(ControlError::NoDisplay);
    let error = run(&port).unwrap_err();
    assert_eq!(cause(&error), "no_display");
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
}

#[test]
fn a_failed_flush_leaves_the_outcome_uncertain() {
    let port = FakePort::showing(vec![home(1)]).failing_send(ControlError::SendFailed);
    let error = run(&port).unwrap_err();
    assert_eq!(cause(&error), "send_failed");
    assert_eq!(error.disposition, DeliverySemantics::uncertain());
}

#[test]
fn a_same_titled_window_of_another_process_is_never_activated() {
    use crate::system::window::foreign::foreign_names::ProcessNames;
    use crate::system::window::foreign::foreign_target::ForeignTarget;
    let port = FakePort::showing(vec![
        record(1, "Home", "zzqqxx"),
        record(2, "Home", "targettool-viewer"),
    ]);
    let target = ForeignTarget {
        title: "Home",
        names: ProcessNames::from_names(vec!["targettool".to_owned()]),
    };
    let _ = activate(&port, &target, deadline());
    assert!(
        !port.sent().contains(&(2, ToplevelRequest::Activate)),
        "the decoy window was activated: {:?}",
        port.sent()
    );
}
