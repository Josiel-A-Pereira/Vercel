use std::time::Duration;

use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode, OperationReport, WindowOp};

use super::foreign_live_rig::{Client, Scene, Shown, active_title, clients, clients_of, lease};
use crate::system::close::close_app;
use crate::system::focus::focus_window::focus_window;
use crate::system::window::window_op::window_op;

fn cause(error: &AdapterError) -> &str {
    error
        .details
        .as_ref()
        .and_then(|details| details["cause"].as_str())
        .unwrap_or_default()
}

fn operate(scene: &Scene, shown: &Shown, op: WindowOp) -> Result<OperationReport, AdapterError> {
    window_op(&scene.bus, &shown.window, op, &lease())
}

fn state_of(shown: &Shown) -> Client {
    clients_of(shown.pid)
        .into_iter()
        .next()
        .expect("the compositor lists the window")
}

fn focus(scene: &Scene, shown: &Shown) {
    focus_window(&scene.bus, &shown.window, &lease()).expect("the window takes focus");
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn foreign_live_focus_moves_between_two_windows() {
    let mut scene = Scene::start("focus");
    scene.with_keyboard();
    let (first_title, second_title) = (scene.title("first"), scene.title("second"));
    let first = scene.window(&first_title, &[]);
    let second = scene.window(&second_title, &[]);

    focus(&scene, &first);
    assert_eq!(active_title(), first_title);
    assert_eq!(state_of(&first).recency, 0);

    focus(&scene, &second);
    assert_eq!(active_title(), second_title);
    assert_eq!(state_of(&second).recency, 0);
    assert_eq!(state_of(&first).recency, 1);

    focus(&scene, &second);
    assert_eq!(active_title(), second_title, "focusing the focused window");
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn foreign_live_maximize_then_restore() {
    let mut scene = Scene::start("maximize");
    let title = scene.title("window");
    let shown = scene.window(&title, &[]);
    let before = state_of(&shown);
    assert_eq!(before.fullscreen, (0, 0));

    let maximized = operate(&scene, &shown, WindowOp::Maximize).expect("a maximize");
    assert_eq!(
        maximized.result.disposition(),
        DeliverySemantics::delivered_verified()
    );
    assert_eq!(state_of(&shown).fullscreen, (1, 1));

    let restored = operate(&scene, &shown, WindowOp::Restore).expect("a restore");
    assert_ne!(
        restored.result.disposition(),
        DeliverySemantics::not_delivered()
    );
    assert_eq!(state_of(&shown).fullscreen, (0, 0));

    operate(&scene, &shown, WindowOp::Restore).expect("a restore with nothing to undo");
    assert_eq!(state_of(&shown).fullscreen, (0, 0));
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn foreign_live_minimize_is_refused_and_the_window_stays_on_screen() {
    let mut scene = Scene::start("minimize");
    let title = scene.title("window");
    let shown = scene.window(&title, &[]);
    let before = state_of(&shown);

    let refused = operate(&scene, &shown, WindowOp::Minimize).expect_err("minimize is refused");
    assert_eq!(refused.code, ErrorCode::ActionFailed);
    assert_eq!(cause(&refused), "minimize_not_applied");
    assert_eq!(
        refused.disposition,
        DeliverySemantics::delivered_unverified()
    );
    std::thread::sleep(Duration::from_millis(300));
    let after = state_of(&shown);
    assert_eq!(after.shown, (true, false), "the window is still shown");
    assert_eq!(after.fullscreen, before.fullscreen);
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn foreign_live_close_ends_the_process() {
    let mut scene = Scene::start("close");
    let title = scene.title("window");
    let shown = scene.window(&title, &[]);

    let report = close_app(&scene.bus, &shown.app(), false, &lease()).expect("a close");
    assert_eq!(
        report.result.disposition(),
        DeliverySemantics::delivered_verified()
    );
    assert!(shown.wait_gone(), "the process exited");
    assert!(clients_of(shown.pid).is_empty(), "the window is gone");
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn foreign_live_close_ends_a_process_with_two_windows_of_one_title() {
    let mut scene = Scene::start("close-twins");
    let title = scene.title("window");
    let shown = scene.window(&title, &["--twin"]);
    assert_eq!(clients_of(shown.pid).len(), 2);

    close_app(&scene.bus, &shown.app(), false, &lease()).expect("a close");
    assert!(shown.wait_gone(), "the process exited");
}

fn prompt_is_reported_and_never_closed(same_title: bool) {
    let mut scene = Scene::start("prompt");
    let title = scene.title("window");
    let prompt_title = if same_title {
        title.clone()
    } else {
        scene.title("prompt")
    };
    let shown = scene.window(&title, &["--prompt", &prompt_title]);

    let first = close_app(&scene.bus, &shown.app(), false, &lease()).expect_err("a prompt");
    assert_eq!(cause(&first), "dialog_pending");
    assert_eq!(first.disposition, DeliverySemantics::delivered_unverified());
    assert!(!shown.gone(), "the app is alive");
    assert_eq!(clients_of(shown.pid).len(), 2, "window and prompt are open");

    let again = close_app(&scene.bus, &shown.app(), false, &lease()).expect_err("still a prompt");
    assert_eq!(cause(&again), "dialog_pending");
    assert_eq!(again.disposition, DeliverySemantics::not_delivered());
    assert!(!shown.gone(), "a second close leaves the app alive");
    assert_eq!(
        clients_of(shown.pid).len(),
        2,
        "a second close never answers the prompt"
    );

    shown.answer_prompt();
    assert!(shown.wait_gone(), "answering the prompt ends the process");
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn foreign_live_close_with_a_prompt_reports_it_and_leaves_it_open() {
    prompt_is_reported_and_never_closed(false);
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn foreign_live_close_with_a_prompt_titled_like_its_window_reports_it_too() {
    prompt_is_reported_and_never_closed(true);
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn foreign_live_two_windows_of_one_title_in_one_process_are_ambiguous() {
    let mut scene = Scene::start("twins");
    scene.with_keyboard();
    let title = scene.title("window");
    let shown = scene.window(&title, &["--twin"]);
    let before = clients();

    let focused = focus_window(&scene.bus, &shown.window, &lease()).expect_err("ambiguous");
    assert_eq!(focused.code, ErrorCode::AmbiguousTarget);
    assert_eq!(cause(&focused), "ambiguous_window");
    assert_eq!(focused.disposition, DeliverySemantics::not_delivered());
    for op in [WindowOp::Maximize, WindowOp::Minimize, WindowOp::Restore] {
        let refused = operate(&scene, &shown, op).expect_err("ambiguous");
        assert_eq!(refused.code, ErrorCode::AmbiguousTarget);
        assert_eq!(refused.disposition, DeliverySemantics::not_delivered());
    }
    assert_eq!(clients(), before, "the compositor saw no change");
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn foreign_live_a_same_titled_window_of_another_program_is_left_alone() {
    let mut scene = Scene::start("other-program");
    scene.with_keyboard();
    let title = scene.title("window");
    let ours = scene.window(&title, &[]);
    let theirs = scene.other_program_window(&title, "other.program");
    let untouched = state_of(&theirs).fullscreen;

    operate(&scene, &ours, WindowOp::Maximize).expect("our window is maximized");
    assert_eq!(state_of(&ours).fullscreen, (1, 1));
    assert_eq!(state_of(&theirs).fullscreen, untouched);

    focus(&scene, &ours);
    assert_eq!(state_of(&ours).recency, 0);

    operate(&scene, &ours, WindowOp::Restore).expect("our window is restored");
    assert_eq!(state_of(&ours).fullscreen, (0, 0));
    assert_eq!(state_of(&theirs).fullscreen, untouched);
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn foreign_live_a_window_whose_app_id_is_unrelated_to_its_process_is_not_changed() {
    let mut scene = Scene::start("unrelated");
    let title = scene.title("window");
    let shown = scene.other_program_window(&title, "zzzz.unrelated");
    let before = clients();

    let refused = operate(&scene, &shown, WindowOp::Maximize).expect_err("identity unproven");
    assert_eq!(cause(&refused), "toplevel_identity_unproven");
    assert_eq!(refused.disposition, DeliverySemantics::not_delivered());
    assert_eq!(clients(), before, "the compositor saw no change");
}

#[test]
fn foreign_live_the_guard_refuses_a_session_that_is_not_named_isolated() {
    let named = std::env::var_os("AGENT_DESKTOP_ISOLATED_HYPRLAND").is_some_and(|v| v == "1");
    if named {
        return;
    }
    let outcome = std::panic::catch_unwind(super::foreign_live_rig::require_isolated_hyprland);
    assert!(
        outcome.is_err(),
        "without the variable the guard must refuse"
    );
}
