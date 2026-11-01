use std::time::Duration;

use agent_desktop_core::{AppInfo, DeliverySemantics, ErrorCode, ProcessId};

use super::super::Request;
use super::super::fake::{Fake, dialog, window};
use super::close;
use crate::system::process::liveness::Liveness;
use crate::system::x11::control::ControlRead;
use crate::system::x11::window::toplevel::Toplevel;

fn request() -> Request<'static> {
    Request {
        name: "Files",
        pid: 42,
        instance: Some("t"),
        force: false,
    }
}

fn foreign_world() -> Fake {
    let mut world = Fake::new(Liveness::Alive);
    world.wayland = true;
    world.foreign = true;
    world
}

fn titled(id: u32, title: &str) -> Toplevel {
    Toplevel {
        title: title.to_owned(),
        ..window(id)
    }
}

#[test]
fn a_contested_title_refuses_the_whole_close_before_anything_is_sent() {
    let mut world = foreign_world();
    world.windows = ControlRead::Ready(vec![titled(1, "Docs")]);
    world.contested = 1;
    world.exits_on_close = true;
    let error = close(&world, &request()).expect_err("refused");
    assert_eq!(error.code, ErrorCode::AmbiguousTarget);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    let details = error.details.expect("details");
    assert_eq!(details["cause"], "ambiguous_window");
    assert_eq!(details["physical_delivery_started"], false);
    assert_eq!(details["contested"], 1);
    assert_eq!(world.count("close"), 0);
    assert_eq!(world.count("term") + world.count("kill"), 0);
}

#[test]
fn a_process_whose_every_title_is_contested_is_ambiguous_not_windowless() {
    let mut world = foreign_world();
    world.windows = ControlRead::Ready(Vec::new());
    world.contested = 2;
    let error = close(&world, &request()).expect_err("refused");
    assert_eq!(error.code, ErrorCode::AmbiguousTarget);
    assert_eq!(world.count("close"), 0);
}

#[test]
fn a_contested_process_that_exited_meanwhile_is_ok() {
    let mut world = foreign_world();
    world.contested = 1;
    world.state_after_toplevels = Some(Liveness::Gone);
    assert!(!close(&world, &request()).expect("it left meanwhile"));
}

#[test]
fn no_proven_window_on_the_foreign_route_is_windowless_not_a_wayland_session() {
    let mut world = foreign_world();
    world.windows = ControlRead::Ready(Vec::new());
    let error = close(&world, &request()).expect_err("refused");
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert!(error.message.contains("could receive a close request"));
    let cause = error
        .details
        .as_ref()
        .and_then(|details| details.get("cause"));
    assert!(cause.is_none(), "{cause:?}");
    assert!(error.suggestion.expect("suggestion").contains("--force"));
    assert_eq!(world.count("close"), 0);
}

#[test]
fn no_window_without_the_foreign_route_keeps_the_wayland_session_refusal() {
    let mut world = foreign_world();
    world.foreign = false;
    world.windows = ControlRead::Ready(Vec::new());
    let error = close(&world, &request()).expect_err("refused");
    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert_eq!(error.details.expect("details")["cause"], "wayland_session");
}

#[test]
fn a_window_that_closes_and_ends_the_process_is_a_verified_exit() {
    let mut world = foreign_world();
    world.windows = ControlRead::Ready(vec![titled(1, "Docs")]);
    world.exits_on_close = true;
    let app = AppInfo {
        name: "Files".to_owned(),
        pid: ProcessId::new(42),
        bundle_id: None,
        process_instance: Some("t".to_owned()),
        presentation: None,
    };
    let report = super::super::run_report(&world, &app, false).expect("closed");
    assert_eq!(
        report.disposition(),
        DeliverySemantics::delivered_verified()
    );
    assert_eq!(*world.closed.borrow(), vec![1]);
    assert_eq!(world.count("term") + world.count("kill"), 0);
}

#[test]
fn two_windows_with_different_titles_are_both_closed() {
    let mut world = foreign_world();
    world.windows = ControlRead::Ready(vec![titled(1, "Docs"), titled(2, "Music")]);
    world.exits_on_close = true;
    close(&world, &request()).expect("closes");
    assert_eq!(*world.closed.borrow(), vec![1, 2]);
}

#[test]
fn a_save_prompt_raised_by_the_close_is_reported_and_never_closed() {
    let mut world = foreign_world();
    world.windows = ControlRead::Ready(vec![titled(1, "Docs")]);
    world.dialog_after = Some(Duration::from_millis(100));
    let error = close(&world, &request()).expect_err("the prompt holds the close");
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
    let details = error.details.expect("details");
    assert_eq!(details["cause"], "dialog_pending");
    assert_eq!(details["dialog"], "dialog 99");
    assert_eq!(*world.closed.borrow(), vec![1]);
}

#[test]
fn a_second_attempt_while_the_prompt_stands_sends_nothing() {
    let mut world = foreign_world();
    world.windows = ControlRead::Ready(vec![titled(1, "Docs")]);
    world.dialog_after = Some(Duration::from_millis(100));
    close(&world, &request()).expect_err("the prompt holds the close");
    for _ in 0..2 {
        let error = close(&world, &request()).expect_err("still held");
        assert_eq!(error.details.expect("details")["cause"], "dialog_pending");
        assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    }
    assert_eq!(*world.closed.borrow(), vec![1]);
}

#[test]
fn a_prompt_already_open_when_the_close_starts_gets_no_request_and_neither_does_the_window() {
    let mut world = foreign_world();
    world.windows = ControlRead::Ready(vec![titled(1, "Docs"), dialog(5)]);
    world.exits_on_close = true;
    let error = close(&world, &request()).expect_err("a prompt is open");
    assert_eq!(error.details.expect("details")["cause"], "dialog_pending");
    assert!(world.closed.borrow().is_empty());
}
