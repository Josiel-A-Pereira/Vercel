use std::cell::{Cell, RefCell};
use std::rc::Rc;

use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics, ErrorCode};

use super::{deliver, send_on};
use crate::input::delivery_channel::DeliveryChannel;
use crate::input::keyboard::keysym_step::KeysymStep;
use crate::input::wayland::keysym_send::send_via;
use crate::input::wayland::virtual_keyboard::vk_fake::FakeVk;
use crate::input::x11::progress::Progress;

const KEY_A: u32 = 0x61;
const KEY_B: u32 = 0x62;
const CONTROL_L: u32 = 0xffe3;
const THAI_KO_KAI: u32 = 0x0100_0e01;
const CONTROL_CHARACTER: u32 = 0x0100_0001;
const EVDEV_A: u32 = 30;

fn live() -> Deadline {
    Deadline::after(5_000).expect("a five second deadline builds")
}

fn edge(keysym: u32, down: bool) -> KeysymStep {
    KeysymStep { keysym, down }
}

fn tap(keysym: u32) -> Vec<KeysymStep> {
    vec![edge(keysym, true), edge(keysym, false)]
}

fn refs(chunks: &[Vec<KeysymStep>]) -> Vec<&[KeysymStep]> {
    chunks.iter().map(Vec::as_slice).collect()
}

fn keys_on_wire(log: &Rc<RefCell<Vec<String>>>) -> Vec<String> {
    log.borrow()
        .iter()
        .filter(|entry| entry.starts_with("key:"))
        .cloned()
        .collect()
}

#[test]
fn text_is_typed_on_the_virtual_keyboard_with_no_shift_bootstrap() {
    let chunks = vec![tap(KEY_A), tap(THAI_KO_KAI)];
    let port = FakeVk::new();
    let log = port.log();
    let keymaps = port.keymaps();
    let delivery = send_on(port, &chunks, &refs(&chunks), &mut || Ok(()), live()).expect("typed");
    assert_eq!(delivery.delivered_events, 4);
    assert_eq!(delivery.channel, DeliveryChannel::VirtualKeyboard);
    assert_eq!(keymaps.borrow().len(), 2);
    assert_eq!(&log.borrow()[..2], ["keymap", "sync"]);
    assert_eq!(keys_on_wire(&log).len(), 4);
    assert!(
        !keys_on_wire(&log)
            .iter()
            .any(|key| key.starts_with("key:42:"))
    );
    assert_eq!(
        &log.borrow()[log.borrow().len() - 3..],
        ["mods:0", "sync", "release"]
    );
}

#[test]
fn untypable_text_is_refused_before_the_session_uploads_a_keymap_or_writes_a_key() {
    let chunks = vec![tap(KEY_A), tap(CONTROL_CHARACTER)];
    let port = FakeVk::new();
    let log = port.log();
    let error =
        send_on(port, &chunks, &refs(&chunks), &mut || Ok(()), live()).expect_err("refused");
    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    let details = error.details.expect("details");
    assert_eq!(details["cause"], "character_outside_layout");
    assert_eq!(details["physical_delivery_started"], false);
    assert!(log.borrow().is_empty(), "wire: {:?}", log.borrow());
}

#[test]
fn a_gate_refusal_mid_chord_leaves_no_key_on_the_wire_and_nothing_delivered() {
    let chunks = vec![vec![
        edge(CONTROL_L, true),
        edge(KEY_A, true),
        edge(KEY_A, false),
        edge(CONTROL_L, false),
    ]];
    let port = FakeVk::new();
    let log = port.log();
    let calls = Cell::new(0);
    let counting: &mut dyn FnMut() -> Result<(), AdapterError> = &mut || {
        calls.set(calls.get() + 1);
        if calls.get() < 4 {
            return Ok(());
        }
        Err(AdapterError::new(ErrorCode::ActionFailed, "focus moved")
            .with_details(serde_json::json!({ "physical_delivery_started": false }))
            .with_disposition(DeliverySemantics::not_delivered()))
    };
    let error = send_on(port, &chunks, &refs(&chunks), counting, live()).expect_err("refused");
    assert_eq!(calls.get(), 4);
    assert!(keys_on_wire(&log).is_empty(), "wire: {:?}", log.borrow());
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert_eq!(
        error.details.expect("details")["physical_delivery_started"],
        false
    );
}

#[test]
fn a_write_failure_while_closing_a_group_is_uncertain_with_the_prefix_counted() {
    let chunks = vec![tap(KEY_A)];
    let mut port = FakeVk::new();
    port.fail_on = Some("key:30:up");
    let log = port.log();
    let error = send_on(port, &chunks, &refs(&chunks), &mut || Ok(()), live()).expect_err("fails");
    assert_eq!(error.disposition, DeliverySemantics::uncertain());
    let details = error.details.expect("details");
    assert_eq!(details["physical_delivery_started"], true);
    assert_eq!(details["delivered_events"], 1);
    assert_eq!(
        keys_on_wire(&log),
        [format!("key:{EVDEV_A}:down"), format!("key:{EVDEV_A}:up")]
    );
    assert_eq!(log.borrow().last().map(String::as_str), Some("release"));
}

#[test]
fn the_route_takes_the_virtual_keyboard_when_the_compositor_offers_one() {
    let chunks = vec![tap(0x0100_4f60), tap(KEY_B)];
    let port = FakeVk::new();
    let keymaps = port.keymaps();
    let delivery = send_via(&chunks, &mut || Ok(()), live(), |_| Ok(Some(port)))
        .expect("typed through the virtual keyboard");
    assert_eq!(delivery.channel, DeliveryChannel::VirtualKeyboard);
    assert_eq!(delivery.delivered_events, 4);
    assert!(
        keymaps
            .borrow()
            .iter()
            .any(|text| text.contains("0x1004f60"))
    );
}

#[test]
fn untypable_text_refused_on_the_route_never_creates_a_keymap() {
    let chunks = vec![tap(CONTROL_CHARACTER)];
    let port = FakeVk::new();
    let log = port.log();
    let error = send_via(&chunks, &mut || Ok(()), live(), |_| Ok(Some(port))).expect_err("no");
    assert_eq!(error.code, ErrorCode::ActionNotSupported);
    assert!(log.borrow().is_empty());
}

#[test]
fn a_stop_request_after_the_first_key_drains_then_releases_the_keyboard_once() {
    let chunks = vec![tap(KEY_A), tap(KEY_B), tap(KEY_A)];
    let port = FakeVk::new();
    let log = port.log();
    let progress = Progress::default();
    let watched = Rc::clone(&log);
    let cancelling: &mut dyn FnMut() -> Result<(), AdapterError> = &mut || {
        if !keys_on_wire(&watched).is_empty() {
            progress.cancel();
        }
        Ok(())
    };
    let error = deliver(port, &refs(&chunks), &progress, cancelling, live()).expect_err("stopped");
    let entries = log.borrow().clone();
    let first_key = entries
        .iter()
        .position(|entry| entry.starts_with("key:"))
        .expect("one key was written");
    let tail: Vec<&str> = entries[first_key + 1..]
        .iter()
        .map(String::as_str)
        .collect();
    assert!(
        !tail
            .iter()
            .any(|entry| entry.starts_with("key:") && entry.ends_with(":down")),
        "wire: {entries:?}"
    );
    let count = |name: &str| {
        entries
            .iter()
            .filter(|entry| entry.as_str() == name)
            .count()
    };
    assert_eq!(count("drain"), 1, "wire: {entries:?}");
    assert_eq!(count("release"), 1);
    assert_eq!(
        &tail[tail.len() - 4..],
        ["drain", "mods:0", "sync", "release"]
    );
    assert_eq!(error.disposition, DeliverySemantics::uncertain());
    let details = error.details.expect("details");
    assert_eq!(details["cause"], "stop_signal");
    assert_eq!(details["physical_delivery_started"], true);
}
