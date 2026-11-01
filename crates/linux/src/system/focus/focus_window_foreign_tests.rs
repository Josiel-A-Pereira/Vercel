use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode};

use super::fake::*;
use super::*;

fn wayland(route: Option<Result<(), AdapterError>>) -> World {
    let mut world = World::new(Some(OTHER));
    world.session = Session::Wayland;
    world.foreign = route;
    world
}

#[test]
fn a_wayland_session_without_a_manager_keeps_its_wayland_refusal() {
    let world = wayland(None);
    let refused = failure(&world);
    assert_eq!(refused.code, ErrorCode::ActionNotSupported);
    assert_eq!(
        refused.details.as_ref().expect("details")["cause"],
        "wayland_session"
    );
    assert_eq!(refused.disposition, DeliverySemantics::not_delivered());
    assert_eq!(world.foreign_calls.get(), 1);
    assert_eq!(world.sends(), 0);
}

#[test]
fn a_wayland_session_with_a_route_returns_the_route_success() {
    let world = wayland(Some(Ok(())));
    run(&world, &window(), budget()).expect("the compositor activated the window");
    assert_eq!(world.foreign_calls.get(), 1);
    assert_eq!(world.sends(), 0);
}

#[test]
fn a_wayland_session_with_a_route_returns_the_route_failure_unchanged() {
    let route = AdapterError::timeout("The compositor did not answer in time")
        .with_disposition(DeliverySemantics::not_delivered());
    let world = wayland(Some(Err(route)));
    let error = failure(&world);
    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert_eq!(world.sends(), 0);
}

#[test]
fn the_route_needs_neither_the_frame_nor_a_display_client() {
    let mut world = wayland(Some(Ok(())));
    world.frame_active.set(Some(false));
    world.state = X11State::Unavailable;
    run(&world, &window(), budget()).expect("only the compositor's answer counts");
}

#[test]
fn a_stopped_application_never_reaches_the_route() {
    let mut world = wayland(Some(Ok(())));
    world.stopped = true;
    assert_eq!(failure(&world).code, ErrorCode::AppUnresponsive);
    assert_eq!(world.foreign_calls.get(), 0);
}

#[test]
fn x11_and_missing_display_sessions_never_reach_the_route() {
    let mut x11 = World::new(Some(OTHER));
    x11.foreign = Some(Ok(()));
    run(&x11, &window(), budget()).expect("the window manager grants it");
    let mut dark = World::new(Some(OTHER));
    dark.session = Session::NoDisplay;
    dark.foreign = Some(Ok(()));
    assert_eq!(failure(&dark).code, ErrorCode::PlatformNotSupported);
    assert_eq!(x11.foreign_calls.get() + dark.foreign_calls.get(), 0);
}
