use super::*;
use crate::input::keyboard::key_names::{KEYSYM_CONTROL_L, KEYSYM_SHIFT_L};
use crate::input::wayland::keysym_send::run_chunks;
use crate::input::wayland::virtual_keyboard::vk_fake::FakeVk;
use agent_desktop_core::{DeliverySemantics, ErrorCode};
use std::cell::RefCell;
use std::rc::Rc;

const THAI_KO_KAI: u32 = 0x0100_0e01;
const THAI_KHO_KHAI: u32 = 0x0100_0e02;

struct Rig {
    session: VkSession<FakeVk>,
    log: Rc<RefCell<Vec<String>>>,
    keymaps: Rc<RefCell<Vec<String>>>,
}

fn live() -> Deadline {
    Deadline::after(5_000).expect("a five second deadline builds")
}

fn started() -> Rig {
    let port = FakeVk::new();
    let log = port.log();
    let keymaps = port.keymaps();
    let mut session = VkSession::new(port);
    session.create(live()).expect("create");
    session.start(live()).expect("start");
    Rig {
        session,
        log,
        keymaps,
    }
}

fn step(keysym: u32, down: bool) -> KeysymStep {
    KeysymStep { keysym, down }
}

fn tap(keysym: u32) -> Vec<KeysymStep> {
    vec![step(keysym, true), step(keysym, false)]
}

fn send(rig: &mut Rig, steps: &[KeysymStep]) -> Result<(), AdapterError> {
    steps
        .iter()
        .try_for_each(|key| rig.session.keysym(key.keysym, key.down, live()))
}

fn log(rig: &Rig) -> Vec<String> {
    rig.log.borrow().clone()
}

fn written_since(rig: &Rig, mark: usize) -> Vec<String> {
    log(rig)[mark..].to_vec()
}

#[test]
fn start_uploads_the_base_keymap_and_syncs_before_any_key() {
    let rig = started();
    assert_eq!(log(&rig), ["keymap", "sync"]);
    assert_eq!(rig.keymaps.borrow()[0], keymap_text(&[]));
    assert!(!rig.session.needs_lead());
    assert!(!rig.session.resumed());
    assert!(!rig.session.retain());
}

#[test]
fn a_tap_writes_one_group_when_the_key_is_released() {
    let mut rig = started();
    let mark = log(&rig).len();
    rig.session
        .keysym(0x61, true, live())
        .expect("press queues");
    assert_eq!(written_since(&rig, mark), Vec::<String>::new());
    rig.session
        .keysym(0x61, false, live())
        .expect("release writes");
    assert_eq!(
        written_since(&rig, mark),
        ["mods:0", "key:30:down", "key:30:up", "sync"]
    );
}

#[test]
fn a_shift_level_symbol_wraps_shift_around_that_key_only() {
    let mut rig = started();
    let mark = log(&rig).len();
    send(&mut rig, &tap(0x41)).expect("A sends");
    send(&mut rig, &tap(u32::from(b'?'))).expect("? sends");
    send(&mut rig, &tap(0x61)).expect("a sends");
    assert_eq!(
        written_since(&rig, mark),
        [
            "mods:0",
            "key:42:down",
            "mods:1",
            "key:30:down",
            "key:30:up",
            "key:42:up",
            "mods:0",
            "sync",
            "mods:0",
            "key:42:down",
            "mods:1",
            "key:53:down",
            "key:53:up",
            "key:42:up",
            "mods:0",
            "sync",
            "mods:0",
            "key:30:down",
            "key:30:up",
            "sync",
        ]
    );
}

#[test]
fn a_chord_writes_nothing_until_balanced_then_writes_the_group_in_order() {
    let mut rig = started();
    let mark = log(&rig).len();
    let chord = [
        step(KEYSYM_CONTROL_L, true),
        step(KEYSYM_SHIFT_L, true),
        step(0x61, true),
        step(0x61, false),
        step(KEYSYM_SHIFT_L, false),
    ];
    send(&mut rig, &chord).expect("the open chord queues");
    assert_eq!(written_since(&rig, mark), Vec::<String>::new());
    rig.session
        .keysym(KEYSYM_CONTROL_L, false, live())
        .expect("the last release writes");
    let written = written_since(&rig, mark);
    assert_eq!(
        written,
        [
            "mods:0",
            "key:29:down",
            "mods:4",
            "key:42:down",
            "mods:5",
            "key:30:down",
            "key:30:up",
            "key:42:up",
            "mods:4",
            "key:29:up",
            "mods:0",
            "sync",
        ]
    );
    assert_eq!(written.first().map(String::as_str), Some("mods:0"));
}

#[test]
fn discarding_a_partial_chord_writes_nothing_and_counts_the_steps() {
    let mut rig = started();
    let mark = log(&rig).len();
    send(&mut rig, &[step(KEYSYM_CONTROL_L, true), step(0x61, true)])
        .expect("the partial chord queues");
    assert_eq!(rig.session.discard_unsent(), 2);
    assert_eq!(written_since(&rig, mark), Vec::<String>::new());
    send(
        &mut rig,
        &[step(0x61, false), step(KEYSYM_CONTROL_L, false)],
    )
    .expect("releases of keys that are not held are no-ops");
    assert_eq!(written_since(&rig, mark), Vec::<String>::new());
    assert_eq!(rig.session.discard_unsent(), 0);
}

#[test]
fn a_key_after_a_discard_starts_a_clean_group() {
    let mut rig = started();
    send(&mut rig, &[step(KEYSYM_CONTROL_L, true)]).expect("queues");
    rig.session.discard_unsent();
    let mark = log(&rig).len();
    send(&mut rig, &tap(0x62)).expect("sends");
    assert_eq!(
        written_since(&rig, mark),
        ["mods:0", "key:48:down", "key:48:up", "sync"]
    );
}

#[test]
fn a_keysym_absent_from_the_keymap_is_an_internal_error_and_writes_nothing() {
    let mut rig = started();
    let mark = log(&rig).len();
    let error = rig
        .session
        .keysym(THAI_KO_KAI, true, live())
        .expect_err("Thai has no slot before prepare");
    assert_eq!(error.code, ErrorCode::Internal);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert_eq!(written_since(&rig, mark), Vec::<String>::new());
    assert_eq!(rig.session.discard_unsent(), 0);
}

#[test]
fn prepare_uploads_extras_and_syncs_before_the_first_key_of_the_chunk() {
    let mut rig = started();
    let chunk: Vec<KeysymStep> = [0x61, THAI_KO_KAI].into_iter().flat_map(tap).collect();
    let mark = log(&rig).len();
    rig.session
        .prepare(&chunk, live())
        .expect("prepare uploads");
    assert_eq!(written_since(&rig, mark), ["keymap", "sync"]);
    assert_eq!(rig.keymaps.borrow().len(), 2);
    assert!(rig.keymaps.borrow()[1].contains("key <K136> { [ 0x1000e01 ] };"));
    send(&mut rig, &chunk).expect("the chunk sends");
    let written = written_since(&rig, mark);
    assert_eq!(written[0], "keymap");
    assert!(written.contains(&"key:128:down".to_owned()));
}

#[test]
fn prepare_re_uploads_only_when_the_extra_set_changes() {
    let mut rig = started();
    let thai: Vec<KeysymStep> = tap(THAI_KO_KAI);
    rig.session.prepare(&thai, live()).expect("first upload");
    rig.session.prepare(&thai, live()).expect("same set");
    rig.session.prepare(&tap(0x61), live()).expect("base only");
    assert_eq!(rig.keymaps.borrow().len(), 2);
    let both: Vec<KeysymStep> = [THAI_KO_KAI, THAI_KHO_KHAI]
        .into_iter()
        .flat_map(tap)
        .collect();
    rig.session.prepare(&both, live()).expect("a new keysym");
    assert_eq!(rig.keymaps.borrow().len(), 3);
    assert!(rig.keymaps.borrow()[2].contains("0x1000e02"));
    let mark = log(&rig).len();
    send(&mut rig, &both).expect("both send");
    assert!(!written_since(&rig, mark).contains(&"keymap".to_owned()));
}

#[test]
fn slot_overflow_in_prepare_is_an_error_and_uploads_nothing() {
    let mut rig = started();
    let chunk: Vec<KeysymStep> = (0..121).flat_map(|at| tap(THAI_KO_KAI + at)).collect();
    let mark = log(&rig).len();
    let error = rig
        .session
        .prepare(&chunk, live())
        .expect_err("121 extras do not fit");
    assert_eq!(error.code, ErrorCode::Internal);
    assert_eq!(written_since(&rig, mark), Vec::<String>::new());
}

#[test]
fn stop_clears_modifiers_destroys_and_drops_pending_steps() {
    let mut rig = started();
    send(&mut rig, &[step(0x61, true)]).expect("queues");
    let mark = log(&rig).len();
    rig.session.stop(live()).expect("stop");
    assert_eq!(written_since(&rig, mark), ["mods:0", "sync", "release"]);
    assert_eq!(rig.session.discard_unsent(), 0);
}

#[test]
fn a_failed_group_flush_leaves_no_pending_state() {
    let mut port = FakeVk::new();
    port.fail_on = Some("sync");
    let mut session = VkSession::new(port);
    session.keysym(0x61, true, live()).expect("queues");
    let error = session
        .keysym(0x61, false, live())
        .expect_err("the group sync fails");
    assert_eq!(error.disposition, DeliverySemantics::Unknown);
    assert_eq!(session.discard_unsent(), 0);
}

#[test]
fn start_gives_the_port_one_chance_to_settle_after_the_keymap_sync_and_before_any_key() {
    let mut port = FakeVk::new();
    port.settles = true;
    let log = port.log();
    let mut session = VkSession::new(port);
    session.create(live()).expect("create");
    session.start(live()).expect("start");
    session.keysym(0x61, true, live()).expect("press");
    session.keysym(0x61, false, live()).expect("release");
    let log = log.borrow();
    assert_eq!(log[..3], ["keymap", "sync", "settle"]);
    assert_eq!(log.iter().filter(|entry| *entry == "settle").count(), 1);
}

fn run(port: FakeVk) -> Result<(), AdapterError> {
    let mut session = VkSession::new(port);
    let chunk = tap(0x61);
    run_chunks(&mut session, &[chunk.as_slice()], &mut || Ok(()), live()).map(|_| ())
}

fn tail(log: &Rc<RefCell<Vec<String>>>, count: usize) -> Vec<String> {
    let entries = log.borrow();
    entries[entries.len() - count..].to_vec()
}

#[test]
fn a_failed_delivery_drains_then_syncs_then_destroys_the_keyboard() {
    let mut port = FakeVk::new();
    port.fail_on = Some("key:30:down");
    let log = port.log();
    run(port).expect_err("the key write fails");
    assert_eq!(tail(&log, 4), ["drain", "mods:0", "sync", "release"]);
}

#[test]
fn a_finished_delivery_syncs_before_destroy_and_drains_once_in_the_caller() {
    let port = FakeVk::new();
    let log = port.log();
    run(port).expect("typed");
    assert_eq!(tail(&log, 3), ["mods:0", "sync", "release"]);
    assert!(!log.borrow().iter().any(|entry| entry == "drain"));
}
