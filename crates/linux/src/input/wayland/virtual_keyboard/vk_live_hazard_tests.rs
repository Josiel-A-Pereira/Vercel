//! Live key delivery under the hazards of a virtual keyboard: a target that is
//! not focused, focus that moves away mid-text, a sender that dies mid-text,
//! and a seat with no other keyboard. Same rig as `vk_live_tests`.

use std::time::{Duration, Instant};

use agent_desktop_core::{Action, DeliverySemantics, ErrorCode, Modifier};

use super::vk_live_rig::{
    Cli, KeyApp, assert_no_residue, only_scene_keyboards, scene_with_keyboard, step_label,
    until_for, virtual_keyboards,
};
use crate::system::window::foreign_live_rig::Scene;

const LONG: &str = "abcdefghijklmnopqrstuvwxyz";

fn long_text(letters: usize) -> String {
    LONG.chars().cycle().take(letters).collect()
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_a_targeted_press_to_an_unfocused_window_is_refused_and_no_window_logs_a_key() {
    let (mut scene, helpers) = scene_with_keyboard("unfocused");
    let target = KeyApp::open(&mut scene, "target", &[]);
    let holder = KeyApp::open(&mut scene, "holder", &[]);
    holder.raise(&scene);
    assert!(holder.focus(&scene));
    let error = target
        .press(&scene, "x", vec![])
        .expect_err("the unfocused target is refused");
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    let details = error.details.as_ref().expect("details");
    assert_eq!(details["physical_delivery_started"], false, "{details}");
    println!("refusal details: {details}");
    std::thread::sleep(Duration::from_millis(500));
    assert!(target.events().is_empty(), "{:?}", target.lines());
    assert!(holder.events().is_empty(), "{:?}", holder.lines());
    assert_no_residue(&holder, helpers);
}

fn move_focus_when(app: &KeyApp, keys: usize, title: String) -> std::thread::JoinHandle<()> {
    let log = app.log.clone();
    std::thread::spawn(move || {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(60) {
            let seen = std::fs::read_to_string(&log)
                .unwrap_or_default()
                .lines()
                .filter(|line| line.starts_with("PRESS"))
                .count();
            if seen >= keys {
                let _ = std::process::Command::new("hyprctl")
                    .args(["dispatch", "focuswindow", &format!("title:^{title}$")])
                    .output();
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    })
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_focus_moved_away_during_a_long_text_stops_delivery_without_residue() {
    let text = long_text(600);
    let (mut scene, helpers) = scene_with_keyboard("moved");
    let typing = KeyApp::open(&mut scene, "typing", &[]);
    let other = KeyApp::open(&mut scene, "other", &[]);
    typing.raise(&scene);
    assert!(typing.focus(&scene));
    let mover = move_focus_when(&typing, 40, other.shown.window.title.clone());
    let error = typing
        .type_text(&scene, &text)
        .expect_err("delivery stops once focus is gone");
    mover.join().expect("the mover ends");
    println!(
        "stop error: {:?} details {:?}",
        error.message, error.details
    );
    let details = error.details.as_ref().expect("details");
    assert_eq!(details["check"], "window_active");
    assert_eq!(details["physical_delivery_started"], true);
    assert_eq!(error.disposition, DeliverySemantics::uncertain());
    std::thread::sleep(Duration::from_millis(500));
    let typed = typing.text();
    assert!(
        typed.len() < text.len() && text.starts_with(&typed),
        "{typed:?}"
    );
    assert!(typing.balanced() && other.balanced());
    assert!(
        other.events().len() <= 2,
        "at most one whole group reached the second window: {:?}",
        other.events()
    );
    assert!(other.text().len() <= 1);
    assert_no_residue(&other, helpers);
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_killing_the_sender_mid_text_leaves_no_stuck_key_or_modifier() {
    let text = long_text(900);
    let (mut scene, helpers) = scene_with_keyboard("killed");
    let app = KeyApp::open(&mut scene, "killed", &[]);
    let cli = Cli::in_scene(&mut scene);
    let reference = cli.entry_ref(app.window_id());
    let mut sender = cli.spawn_type(&reference, &text);
    assert!(until_for(Duration::from_secs(30), || {
        app.events()
            .iter()
            .filter(|event| event.kind == "PRESS")
            .count()
            >= 30
    }));
    sender.kill().expect("SIGKILL reaches the sender");
    sender.wait().expect("the sender is reaped");
    std::thread::sleep(Duration::from_millis(400));
    let settled = app.lines().len();
    std::thread::sleep(Duration::from_millis(1800));
    assert_eq!(
        app.lines().len(),
        settled,
        "the log kept growing: a key is stuck and repeating"
    );
    assert!(app.text().len() < text.len());
    assert_no_residue(&app, helpers);
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_the_first_key_arrives_when_the_seat_has_no_other_keyboard() {
    let mut scene = Scene::start("firstkey");
    assert!(
        virtual_keyboards().is_empty(),
        "the rig seat must start with no keyboard: {:?}",
        virtual_keyboards()
    );
    let app = KeyApp::open_unfocused(&mut scene, "firstkey");
    app.raise(&scene);
    let chunks = crate::input::keyboard::text_plan::plan_text_keysyms("hi there");
    let delivery = crate::input::deliver::send_keysyms(
        &chunks,
        &mut || Ok(()),
        agent_desktop_core::Deadline::after(20_000).expect("a deadline"),
    )
    .expect("typed with no other keyboard in the seat");
    assert_eq!(
        delivery.channel,
        crate::input::delivery_channel::DeliveryChannel::VirtualKeyboard
    );
    assert!(app.wait_text("hi there"), "got {:?}", app.text());
    assert!(app.balanced());
    assert!(only_scene_keyboards(0));
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_no_virtual_keyboard_outlives_a_press_a_type_or_a_refusal() {
    let (mut scene, helpers) = scene_with_keyboard("devices");
    let app = KeyApp::open(&mut scene, "devices", &[]);
    let hidden = KeyApp::open(&mut scene, "hidden", &[]);
    app.raise(&scene);
    assert!(app.focus(&scene));
    app.press(&scene, "x", vec![Modifier::Ctrl]).expect("press");
    assert!(only_scene_keyboards(helpers), "{:?}", virtual_keyboards());
    app.type_text(&scene, "ok").expect("type");
    assert!(only_scene_keyboards(helpers), "{:?}", virtual_keyboards());
    hidden.press(&scene, "x", vec![]).expect_err("refused");
    assert!(only_scene_keyboards(helpers), "{:?}", virtual_keyboards());
    let refused = app
        .physical_only(&scene, Action::TypeText("\u{1}".to_owned()))
        .expect_err("a control character is refused");
    assert_eq!(refused.code, ErrorCode::ActionNotSupported);
    assert_eq!(refused.disposition, DeliverySemantics::not_delivered());
    assert!(only_scene_keyboards(helpers), "{:?}", virtual_keyboards());
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_two_hundred_characters_type_in_a_measured_time() {
    let text = long_text(200);
    let (mut scene, helpers) = scene_with_keyboard("timing");
    let app = KeyApp::open(&mut scene, "timing", &[]);
    let started = Instant::now();
    let result = app.type_text(&scene, &text).expect("typed");
    let elapsed = started.elapsed();
    println!("TIMING 200 characters: {elapsed:?}");
    assert_eq!(step_label(&result), "VirtualKeyboard.type_text");
    assert!(app.wait_text(&text), "got {:?}", app.text());
    assert_no_residue(&app, helpers);
}
