use super::*;

#[test]
fn on_wayland_an_activated_compositor_window_lets_the_press_through() {
    let physical = FakeWorld::wayland();
    let mut keys = Keys::owning(&physical);
    keys.compositor = Some(Ok(true));
    press(&physical, &keys, InteractionPolicy::headless()).expect("both proofs agree");
    assert!(keys.read_count("compositor") >= 1);
    assert_eq!(physical.count("sent"), 1);
}

#[test]
fn a_focused_frame_the_compositor_does_not_call_activated_gets_no_keys() {
    let physical = FakeWorld::wayland();
    let mut keys = Keys::owning(&physical);
    keys.compositor = Some(Ok(false));
    let error = press(&physical, &keys, InteractionPolicy::headed()).unwrap_err();
    assert_eq!(check_of(&error).as_deref(), Some("window_active"));
    assert_eq!(
        error.details.as_ref().unwrap()["cause"],
        "compositor_not_activated"
    );
    assert!(not_delivered(&error));
    assert!(physical.sent_nothing());
}

#[test]
fn a_named_window_the_compositor_does_not_call_activated_is_not_exactly_focused() {
    let physical = FakeWorld::wayland();
    let mut keys = Keys::owning(&physical);
    keys.compositor = Some(Ok(false));
    let error = press_in(Press {
        physical: &physical,
        keys: &keys,
        process: &process(),
        window: Some(&window(PID, "target")),
        combo: &combo(),
        policy: InteractionPolicy::headless(),
        deadline: deadline(),
    })
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::StaleRef);
    assert_eq!(check_of(&error).as_deref(), Some("exact_window_focus"));
    assert!(not_delivered(&error));
    assert!(physical.sent_nothing());
}

#[test]
fn a_compositor_list_that_cannot_be_read_refuses_instead_of_passing() {
    let physical = FakeWorld::wayland();
    let mut keys = Keys::owning(&physical);
    keys.compositor = Some(Err(AdapterError::new(ErrorCode::Timeout, "link failed")
        .with_disposition(DeliverySemantics::not_delivered())));
    let error = press(&physical, &keys, InteractionPolicy::headed()).unwrap_err();
    assert_eq!(error.code, ErrorCode::Timeout);
    assert!(not_delivered(&error));
    assert!(physical.sent_nothing());
}

#[test]
fn the_compositor_is_not_read_on_x11_or_when_the_frame_proof_already_refused() {
    let physical = FakeWorld::x11();
    let mut keys = Keys::owning(&physical);
    keys.compositor = Some(Ok(false));
    press(&physical, &keys, InteractionPolicy::headed()).expect("x11 keeps its own proof");
    assert_eq!(keys.read_count("compositor"), 0);

    let wayland = FakeWorld::wayland();
    let mut keys = Keys::owning(&wayland);
    keys.compositor = Some(Ok(true));
    keys.set_focused(Some(OTHER));
    press(&wayland, &keys, InteractionPolicy::headed()).unwrap_err();
    assert_eq!(keys.read_count("compositor"), 0);
}

#[test]
fn a_press_through_the_virtual_keyboard_is_labelled_with_it() {
    let physical = FakeWorld::wayland()
        .through(crate::input::delivery_channel::DeliveryChannel::VirtualKeyboard);
    let mut keys = Keys::owning(&physical);
    keys.compositor = Some(Ok(true));
    let result = press(&physical, &keys, InteractionPolicy::headless()).expect("sent");
    assert_eq!(result.steps[0].label(), "VirtualKeyboard.press_key");
}
