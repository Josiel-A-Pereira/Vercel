use super::*;
use crate::input::delivery_channel::DeliveryChannel;

#[test]
fn every_keyboard_path_names_the_virtual_keyboard_when_it_carried_the_keys() {
    let through = || FakeWorld::wayland().through(DeliveryChannel::VirtualKeyboard);

    let world = through();
    let steps = type_text_steps(
        &world,
        &element(),
        "hi",
        InteractionPolicy::focus_fallback(),
        deadline(),
    )
    .unwrap();
    assert_eq!(steps[0].label(), "VirtualKeyboard.type_text");

    let world = through();
    let steps = press_key_element_steps(
        &world,
        &element(),
        &combo(),
        InteractionPolicy::focus_fallback(),
        deadline(),
    )
    .unwrap();
    assert_eq!(steps[0].label(), "VirtualKeyboard.press_key");

    let world = through();
    let steps = press_key_global_steps(&world, &combo(), deadline()).unwrap();
    assert_eq!(steps[0].label(), "VirtualKeyboard.press_key");

    let world = through();
    let result = press_for_process(
        &world,
        &combo(),
        InteractionPolicy::headless(),
        &mut || Ok(()),
        deadline(),
    )
    .unwrap();
    assert_eq!(result.steps[0].label(), "VirtualKeyboard.press_key");
}

#[test]
fn the_wayland_focus_advice_is_true_for_hyprland_and_for_gnome() {
    let inactive = || {
        AdapterError::new(ErrorCode::ActionFailed, "inactive")
            .with_details(serde_json::json!({ "check": "window_active" }))
    };
    for policy in [InteractionPolicy::headless(), InteractionPolicy::headed()] {
        let world = FakeWorld::wayland();
        let error = press_for_process(
            &world,
            &combo(),
            policy,
            &mut || Err(inactive()),
            deadline(),
        )
        .unwrap_err();
        let advice = error.suggestion.expect("advice");
        assert!(advice.contains("bring the target window to the front yourself"));
        assert!(advice.contains("Hyprland") && advice.contains("not on GNOME"));
        assert!(advice.contains("global press without --app, --window-id or --headed"));
        assert!(!advice.contains("automatic activation is unavailable"));
    }
}
