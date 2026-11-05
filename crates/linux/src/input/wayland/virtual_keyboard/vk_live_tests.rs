//! Live key delivery through the virtual keyboard, inside the isolated
//! Hyprland the rig starts. Every case drives the adapter's own physical
//! action path (`execute_action` with a headed request) against the
//! key-logging fixture and asserts on what the fixture logged.

use std::time::Duration;

use agent_desktop_core::Modifier;

use super::vk_live_modifiers::Holder;
use super::vk_live_rig::{KeyApp, assert_no_residue, scene_with_keyboard, step_label, until_for};

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_letters_digits_and_shifted_symbols_type_exactly() {
    let (mut scene, helpers) = scene_with_keyboard("letters");
    let app = KeyApp::open(&mut scene, "letters", &[]);
    let result = app.type_text(&scene, "aZ?!~ 19").expect("typed");
    assert_eq!(step_label(&result), "VirtualKeyboard.type_text");
    assert!(app.wait_text("aZ?!~ 19"), "got {:?}", app.text());
    assert_no_residue(&app, helpers);
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_ctrl_a_selects_all_in_the_entry() {
    let (mut scene, helpers) = scene_with_keyboard("ctrla");
    let app = KeyApp::open(&mut scene, "ctrla", &[]);
    app.type_text(&scene, "hello").expect("typed");
    assert!(app.wait_text("hello"));
    let result = app
        .press(&scene, "a", vec![Modifier::Ctrl])
        .expect("pressed");
    assert_eq!(step_label(&result), "VirtualKeyboard.press_key");
    assert!(
        until_for(Duration::from_secs(5), || app
            .selections()
            .iter()
            .any(|bounds| bounds == "0\t5")),
        "{:?}",
        app.lines()
    );
    app.type_text(&scene, "X")
        .expect("typed over the selection");
    assert!(app.wait_text("X"), "got {:?}", app.text());
    let ctrl_a = app
        .events()
        .into_iter()
        .find(|event| event.kind == "PRESS" && event.name == "a" && event.mods.contains("ctrl"));
    assert!(ctrl_a.is_some(), "{:?}", app.events());
    assert_no_residue(&app, helpers);
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_return_and_tab_arrive_as_their_keysyms() {
    let (mut scene, helpers) = scene_with_keyboard("returntab");
    let app = KeyApp::open(&mut scene, "returntab", &[]);
    app.press(&scene, "return", vec![]).expect("return");
    app.press(&scene, "tab", vec![]).expect("tab");
    app.type_text(&scene, "a\tb\nc").expect("typed");
    assert!(until_for(Duration::from_secs(5), || app
        .events()
        .iter()
        .filter(|event| event.kind == "PRESS")
        .count()
        >= 7));
    let pressed: Vec<String> = app
        .events()
        .into_iter()
        .filter(|event| event.kind == "PRESS")
        .map(|event| event.name)
        .collect();
    assert_eq!(pressed, ["Return", "Tab", "a", "Tab", "b", "Return", "c"]);
    assert_no_residue(&app, helpers);
}

const THAI_MIXED: &str = "\u{e19}\u{e49}\u{e33}\u{e01} abc \u{e44}\u{e17}\u{e22}";

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_thai_with_combining_marks_latin_and_an_emoji_type_exactly() {
    let (mut scene, helpers) = scene_with_keyboard("thai");
    let app = KeyApp::open(&mut scene, "thai", &[]);
    app.type_text(&scene, THAI_MIXED).expect("thai typed");
    assert!(app.wait_text(THAI_MIXED), "got {:?}", app.text());
    app.type_text(&scene, "\u{1F600}").expect("emoji typed");
    let both = format!("{THAI_MIXED}\u{1F600}");
    assert!(app.wait_text(&both), "got {:?}", app.text());
    assert_no_residue(&app, helpers);
}

pub(super) fn spanning_text() -> String {
    let thai = ('\u{e01}'..='\u{e2e}').map(String::from);
    let greek = "\u{3b1}\u{3b2}\u{3b3}\u{3b4}\u{3b5}\u{3b6}"
        .chars()
        .map(String::from);
    let latin = ('a'..='z').map(String::from);
    thai.chain(greek)
        .zip(latin.cycle())
        .map(|(distinct, latin)| format!("{distinct}{latin}"))
        .collect()
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_text_over_the_chunk_size_with_distinct_characters_keeps_order() {
    let text = spanning_text();
    assert!(text.chars().count() > 96, "three chunks or more");
    let (mut scene, helpers) = scene_with_keyboard("chunks");
    let app = KeyApp::open(&mut scene, "chunks", &[]);
    app.type_text(&scene, &text).expect("typed");
    assert!(app.wait_text(&text), "got {:?}", app.text());
    assert_no_residue(&app, helpers);
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_alt_chord_reaches_the_app_with_mod1() {
    let (mut scene, helpers) = scene_with_keyboard("alt");
    let app = KeyApp::open(&mut scene, "alt", &[]);
    let result = app.press(&scene, "x", vec![Modifier::Alt]).expect("alt+x");
    assert_eq!(step_label(&result), "VirtualKeyboard.press_key");
    let arrived = |app: &KeyApp| {
        app.events()
            .into_iter()
            .find(|event| event.kind == "PRESS" && event.name == "x")
    };
    assert!(until_for(Duration::from_secs(5), || arrived(&app).is_some()));
    let x = arrived(&app).expect("x arrived");
    assert!(x.mods.contains("mod1"), "{x:?}");
    assert_no_residue(&app, helpers);
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_title_that_changes_on_every_key_still_types_exactly() {
    let text = "the title changes with every key, so a title read would fail mid-text";
    let (mut scene, helpers) = scene_with_keyboard("retitle");
    for (name, flag) in [("both", "--retitle"), ("surface", "--retitle-surface")] {
        let app = KeyApp::open(&mut scene, name, &[flag]);
        app.type_text(&scene, text).expect("typed");
        assert!(app.wait_text(text), "{flag}: got {:?}", app.text());
        assert_no_residue(&app, helpers);
    }
}

fn hyprctl(args: &[&str]) {
    let output = std::process::Command::new("hyprctl")
        .args(args)
        .output()
        .expect("hyprctl runs");
    assert!(output.status.success(), "{args:?}: {output:?}");
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_super_chord_fires_the_compositor_bind_and_the_app_sees_nothing() {
    let (mut scene, helpers) = scene_with_keyboard("super");
    let app = KeyApp::open(&mut scene, "super", &[]);
    let fired = scene.scratch_file("bind-fired");
    hyprctl(&[
        "keyword",
        "bind",
        &format!("SUPER,x,exec,touch {}", fired.display()),
    ]);
    let result = app.press(&scene, "x", vec![Modifier::Meta]);
    hyprctl(&["keyword", "unbind", "SUPER,x"]);
    let result = result.expect("super+x is delivered");
    assert_eq!(step_label(&result), "VirtualKeyboard.press_key");
    assert!(
        until_for(Duration::from_secs(5), || fired.exists()),
        "the bind did not fire"
    );
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        app.events().iter().all(|event| event.name != "x"),
        "the app saw the consumed key: {:?}",
        app.lines()
    );
    assert_no_residue(&app, helpers);
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_oracle_sees_a_modifier_request_the_old_letter_check_cannot() {
    let (mut scene, _helpers) = scene_with_keyboard("negmask");
    let app = KeyApp::open(&mut scene, "negmask", &[]);
    assert!(app.modifiers_released(), "{:?}", app.lines());
    let holder = Holder::modifier("shift");
    assert!(
        until_for(Duration::from_secs(5), || app.held_modifiers() == "shift"),
        "the oracle missed a held shift: {:?}",
        app.lines()
    );
    drop(holder);
    println!("MODS after the holder died: {}", app.held_modifiers());
    assert!(app.modifiers_released(), "{:?}", app.lines());
}

#[test]
#[ignore = "drives real windows; run inside an isolated nested Hyprland with AGENT_DESKTOP_ISOLATED_HYPRLAND=1"]
fn vk_live_oracle_sees_a_held_physical_shift_key() {
    let (mut scene, _helpers) = scene_with_keyboard("negkey");
    let app = KeyApp::open(&mut scene, "negkey", &[]);
    assert!(app.balanced(), "{:?}", app.lines());
    let _holder = Holder::key_for("Shift_L", 2500);
    assert!(
        until_for(Duration::from_secs(5), || !app.balanced()),
        "the oracle missed a held Shift_L: {:?}",
        app.lines()
    );
    assert!(
        until_for(Duration::from_secs(8), || app.balanced()),
        "{:?}",
        app.lines()
    );
}
