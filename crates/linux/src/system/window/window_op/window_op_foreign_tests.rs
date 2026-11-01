use super::*;

fn cause(error: &AdapterError) -> String {
    error.details.as_ref().expect("details")["cause"]
        .as_str()
        .expect("a cause")
        .to_owned()
}

fn native_wayland(route: Option<Result<(), AdapterError>>) -> World {
    let mut world = World::showing(state(0, 0, 1, 1));
    world.wayland = true;
    world
        .clients
        .replace([ControlRead::Ready(vec![client(CLIENT, "Another title")])].into());
    world.foreign = route;
    world
}

#[test]
fn a_native_wayland_window_without_a_manager_keeps_the_wayland_refusal() {
    let world = native_wayland(None);
    let error = run(&world, &window(), &WindowOp::Maximize, budget()).expect_err("refused");
    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert_eq!(cause(&error), "wayland_session");
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert_eq!(world.foreign_ops.borrow().len(), 1);
    assert_eq!(world.writes(), 0);
}

#[test]
fn a_route_success_is_a_receipt_with_no_geometry_read_and_no_x11_write() {
    for (op, disposition) in [
        (WindowOp::Maximize, DeliverySemantics::delivered_verified()),
        (WindowOp::Restore, DeliverySemantics::delivered_unverified()),
    ] {
        let world = native_wayland(Some(Ok(())));
        let report = run(&world, &window(), &op, budget()).expect("the compositor did it");
        assert_eq!(report.disposition(), disposition, "{op:?}");
        let details = report.result.details.unwrap_or_default();
        assert!(details.get("observed").is_none(), "{op:?}");
        assert_eq!(world.foreign_ops.borrow().len(), 1, "{op:?}");
        assert_eq!(world.writes(), 0);
    }
}

#[test]
fn a_route_refusal_comes_back_as_it_is() {
    let refusal = AdapterError::new(ErrorCode::ActionFailed, "no state change")
        .with_disposition(DeliverySemantics::delivered_unverified());
    let world = native_wayland(Some(Err(refusal)));
    let error = run(&world, &window(), &WindowOp::Minimize, budget()).expect_err("refused");
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.disposition, DeliverySemantics::delivered_unverified());
    assert_eq!(world.writes(), 0);
}

#[test]
fn an_error_other_than_the_wayland_session_never_reaches_the_route() {
    let mut stopped = native_wayland(Some(Ok(())));
    stopped.stopped = true;
    let mut no_manager = native_wayland(Some(Ok(())));
    no_manager.wayland = false;
    no_manager.clients.replace([ControlRead::NoManager].into());
    let mut ambiguous = native_wayland(Some(Ok(())));
    ambiguous.wayland = false;
    ambiguous.clients.replace(
        [ControlRead::Ready(vec![
            client(CLIENT, "Doc"),
            client(CLIENT + 1, "Doc"),
        ])]
        .into(),
    );
    for world in [&stopped, &no_manager, &ambiguous] {
        run(world, &window(), &WindowOp::Maximize, budget()).expect_err("refused");
        assert!(world.foreign_ops.borrow().is_empty());
    }
}

#[test]
fn an_x11_window_is_driven_by_x11_and_never_asks_the_route() {
    let mut world = World::showing(state(0, 0, 1, 1));
    world.foreign = Some(Ok(()));
    let _ = run(&world, &window(), &WindowOp::Maximize, budget());
    assert!(world.foreign_ops.borrow().is_empty());
    assert_eq!(world.writes(), 1);
}

#[test]
fn an_already_satisfied_foreign_state_reports_no_delivery() {
    for op in [WindowOp::Maximize, WindowOp::Restore] {
        let mut world = native_wayland(Some(Ok(())));
        world.foreign_sent = false;
        let report = run(&world, &window(), &op, budget()).expect("already satisfied");
        assert_eq!(report.disposition(), DeliverySemantics::not_delivered());
        assert_eq!(world.writes(), 0);
    }
}
