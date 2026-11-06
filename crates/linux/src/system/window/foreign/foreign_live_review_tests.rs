use agent_desktop_core::{AdapterError, DeliverySemantics, WindowOp};

use super::foreign_live_rig::{DIALOG_FIXTURE, Scene, Shown, clients, clients_of, lease};
use super::foreign_names::ProcessNames;
use crate::system::close::close_app;
use crate::system::focus::focus_window::focus_window;
use crate::system::window::window_op::window_op;

fn names_of(shown: &Shown) -> ProcessNames {
    ProcessNames::of(shown.pid, &shown.window.app)
}

fn active_pid() -> u64 {
    clients()
        .into_iter()
        .find(|client| client.recency == 0)
        .map(|client| client.pid)
        .unwrap_or_default()
}

fn cause(error: &AdapterError) -> &str {
    error
        .details
        .as_ref()
        .and_then(|details| details["cause"].as_str())
        .unwrap_or_default()
}

fn decoy_pair(scene: &mut Scene, title: &str) -> (Shown, Shown) {
    let plain = Scene::plain_fixture();
    let target = scene.program_window(plain, "targettool", title, &["--app-id", "zzqqxx"]);
    let decoy = scene.program_window(
        plain,
        "decoyproc",
        title,
        &["--app-id", "targettool-viewer"],
    );
    assert!(!names_of(&target).relates("zzqqxx"), "{:?}", target.window);
    assert!(names_of(&target).relates("targettool-viewer"));
    assert!(!names_of(&decoy).relates("targettool-viewer"));
    assert_ne!(target.pid, decoy.pid);
    (target, decoy)
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn foreign_live_review_focus_never_lands_on_another_process_with_the_same_title() {
    let mut scene = Scene::start("review-focus");
    scene.with_keyboard();
    let title = scene.title("Home");
    let (target, decoy) = decoy_pair(&mut scene, &title);
    let filler_title = scene.title("filler");
    let filler = scene.window(&filler_title, &[]);
    assert_eq!(active_pid(), u64::from(filler.pid));

    let outcome = focus_window(&scene.bus, &target.window, &lease());

    let decoy_recency = clients_of(decoy.pid)[0].recency;
    assert_ne!(
        decoy_recency, 0,
        "the decoy took focus; outcome {outcome:?}"
    );
    if outcome.is_ok() {
        assert_eq!(active_pid(), u64::from(target.pid));
    }
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn foreign_live_review_close_app_never_closes_another_process_with_the_same_title() {
    let mut scene = Scene::start("review-close");
    let title = scene.title("Home");
    let (target, decoy) = decoy_pair(&mut scene, &title);

    let outcome = close_app(&scene.bus, &target.app(), false, &lease());

    std::thread::sleep(std::time::Duration::from_millis(500));
    assert!(!decoy.gone(), "the decoy was closed; outcome {outcome:?}");
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn foreign_live_review_close_app_with_a_dialog_of_an_unrelated_app_id_is_dialog_pending() {
    let mut scene = Scene::start("review-dialog");
    let title = scene.title("window");
    let dialog_title = scene.title("dialog");
    let shown = scene.program_window(
        DIALOG_FIXTURE,
        "targettool",
        &title,
        &["--app-id", "org.targettool.Main", "--dialog", &dialog_title],
    );
    let names = names_of(&shown);
    assert!(names.relates("org.targettool.Main"));
    assert!(!names.relates("zzqqxx"));
    let classes: Vec<String> = clients_of(shown.pid)
        .into_iter()
        .map(|client| client.class)
        .collect();
    assert_eq!(classes.len(), 2, "window and dialog are open: {classes:?}");
    assert_ne!(classes[0], classes[1], "the app ids differ: {classes:?}");

    let outcome = close_app(&scene.bus, &shown.app(), false, &lease());

    assert!(
        !shown.gone(),
        "the app closed with its dialog open: {outcome:?}"
    );
    let error = outcome.expect_err("a dialog holds the close");
    assert_eq!(cause(&error), "dialog_pending", "{error:?}");
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn foreign_live_review_maximize_of_a_maximized_window_is_not_delivered() {
    let mut scene = Scene::start("review-maximize");
    let title = scene.title("window");
    let shown = scene.window(&title, &[]);
    let first = window_op(&scene.bus, &shown.window, WindowOp::Maximize, &lease())
        .expect("the first maximize");
    assert_eq!(
        first.result.disposition(),
        DeliverySemantics::delivered_verified()
    );
    assert_eq!(clients_of(shown.pid)[0].fullscreen, (1, 1));

    let again = window_op(&scene.bus, &shown.window, WindowOp::Maximize, &lease())
        .expect("the second maximize");

    assert_eq!(
        again.result.disposition(),
        DeliverySemantics::not_delivered(),
        "nothing was sent: {again:?}"
    );
}
