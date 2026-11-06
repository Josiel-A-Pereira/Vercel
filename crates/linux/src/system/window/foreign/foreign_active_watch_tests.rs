use agent_desktop_core::{DeliverySemantics, ErrorCode};

use super::PinnedToplevel;
use crate::system::window::active_watch::ActiveWatch;
use crate::system::window::foreign::foreign_fake::{FakePort, activated, deadline, record, target};
use crate::system::window::foreign::foreign_record::ToplevelRecord;
use crate::system::x11::control::control_error::ControlError;

fn home(id: u32) -> ToplevelRecord {
    record(id, "Home", "thunar")
}

fn pinned(port: FakePort) -> PinnedToplevel<FakePort> {
    PinnedToplevel::pin(port, &target("Home"), deadline())
        .expect("the link syncs")
        .expect("the window is proven activated")
}

#[test]
fn a_title_change_after_the_pin_keeps_the_same_toplevel_active() {
    let port = FakePort::showing(vec![activated(home(1))])
        .then_showing(vec![activated(home(1))])
        .then_showing(vec![activated(record(1, "Home *", "thunar"))])
        .then_showing(vec![activated(record(1, "", "thunar"))]);
    let watch = pinned(port);
    assert!(watch.active(deadline()).unwrap());
    assert!(watch.active(deadline()).unwrap());
}

#[test]
fn a_toplevel_that_is_no_longer_activated_is_a_no() {
    let port = FakePort::showing(vec![activated(home(1))])
        .then_showing(vec![activated(home(1))])
        .then_showing(vec![home(1), activated(record(2, "Other", "kitty"))]);
    assert!(!pinned(port).active(deadline()).unwrap());
}

#[test]
fn a_toplevel_that_is_gone_is_a_no() {
    let port = FakePort::showing(vec![activated(home(1))])
        .then_showing(vec![activated(home(1))])
        .then_showing(vec![activated(record(2, "Home", "thunar"))]);
    assert!(!pinned(port).active(deadline()).unwrap());
}

#[test]
fn each_question_syncs_the_list_once() {
    let port = FakePort::showing(vec![activated(home(1))]);
    let watch = pinned(port);
    assert!(watch.active(deadline()).unwrap());
    assert!(watch.active(deadline()).unwrap());
    assert_eq!(watch.port.refreshes(), 3);
}

#[test]
fn a_window_the_list_does_not_prove_activated_is_not_pinned() {
    for shown in [
        vec![home(1)],
        vec![activated(home(1)), activated(home(2))],
        vec![activated(record(1, "Away", "thunar"))],
        Vec::new(),
    ] {
        let pinned = PinnedToplevel::pin(FakePort::showing(shown), &target("Home"), deadline());
        assert!(pinned.expect("the link syncs").is_none());
    }
}

#[test]
fn a_link_that_cannot_sync_is_an_error_never_a_yes() {
    for (broken, code) in [
        (ControlError::TimedOut, ErrorCode::Timeout),
        (ControlError::NoDisplay, ErrorCode::ActionFailed),
    ] {
        let mut watch = pinned(FakePort::showing(vec![activated(home(1))]));
        watch.port = FakePort::showing(vec![activated(home(1))]).failing_refresh(broken);
        let error = watch.active(deadline()).unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    }
}
