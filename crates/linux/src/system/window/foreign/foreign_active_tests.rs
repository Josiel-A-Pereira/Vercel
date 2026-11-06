use agent_desktop_core::{DeliverySemantics, ErrorCode};

use super::proves_active;
use crate::system::window::foreign::foreign_fake::{FakePort, activated, deadline, record, target};
use crate::system::x11::control::control_error::ControlError;

fn home(id: u32) -> crate::system::window::foreign::foreign_record::ToplevelRecord {
    record(id, "Home", "thunar")
}

#[test]
fn the_proven_window_is_active_only_when_the_compositor_says_activated() {
    let port = FakePort::showing(vec![activated(home(1)), record(2, "Other", "thunar")]);
    assert!(proves_active(&port, &target("Home"), deadline()).unwrap());
    assert!(!proves_active(&port, &target("Other"), deadline()).unwrap());
}

#[test]
fn a_proven_window_that_is_not_activated_is_a_no_and_nothing_is_sent() {
    let port = FakePort::showing(vec![home(1), activated(record(2, "Other", "kitty"))]);
    assert!(!proves_active(&port, &target("Home"), deadline()).unwrap());
    assert!(port.sent().is_empty());
}

#[test]
fn a_toplevel_the_list_does_not_prove_is_a_no_even_when_it_is_activated() {
    for shown in [
        vec![activated(home(1)), activated(home(2))],
        vec![activated(record(1, "Away", "thunar"))],
        vec![activated(record(1, "Home", "kitty"))],
        Vec::new(),
    ] {
        let port = FakePort::showing(shown);
        assert!(!proves_active(&port, &target("Home"), deadline()).unwrap());
        assert!(port.sent().is_empty());
    }
}

#[test]
fn the_list_is_synced_once_so_a_stale_activation_is_not_read() {
    let port = FakePort::showing(vec![activated(home(1))]).then_showing(vec![home(1)]);
    assert!(!proves_active(&port, &target("Home"), deadline()).unwrap());
    assert_eq!(port.refreshes(), 1);
}

#[test]
fn a_link_that_cannot_sync_is_an_error_with_nothing_delivered_never_a_pass() {
    for (broken, code) in [
        (ControlError::TimedOut, ErrorCode::Timeout),
        (ControlError::NoDisplay, ErrorCode::ActionFailed),
    ] {
        let port = FakePort::showing(vec![activated(home(1))]).failing_refresh(broken);
        let error = proves_active(&port, &target("Home"), deadline()).unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    }
}
