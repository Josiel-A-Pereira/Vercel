use super::*;
use agent_desktop_core::ErrorCode;

fn tap(keysym: u32) -> [KeysymStep; 2] {
    [
        KeysymStep { keysym, down: true },
        KeysymStep {
            keysym,
            down: false,
        },
    ]
}

fn taps(keysyms: &[u32]) -> Vec<KeysymStep> {
    keysyms.iter().flat_map(|keysym| tap(*keysym)).collect()
}

#[test]
fn base_keys_need_no_extras() {
    let extras = Extras::default();
    assert_eq!(
        extras
            .plan(&taps(&[0x61, 0x41, 0x3f, 0xff0d]))
            .expect("plans"),
        None
    );
}

#[test]
fn extras_are_assigned_in_order_of_first_use_without_duplicates() {
    let extras = Extras::default();
    let plan = extras
        .plan(&taps(&[0x0100_0e01, 0x61, 0xe9, 0x0100_0e01]))
        .expect("plans");
    assert_eq!(plan, Some(vec![0x0100_0e01, 0xe9]));
}

#[test]
fn a_chunk_already_covered_keeps_the_uploaded_keymap() {
    let mut extras = Extras::default();
    extras.replace(vec![0x0100_0e01, 0xe9, 0x0100_0e02]);
    assert_eq!(
        extras.plan(&taps(&[0xe9, 0x0100_0e01])).expect("plans"),
        None
    );
    assert_eq!(extras.plan(&taps(&[0x61])).expect("plans"), None);
}

#[test]
fn a_new_keysym_assigns_a_fresh_set() {
    let mut extras = Extras::default();
    extras.replace(vec![0x0100_0e01]);
    let plan = extras
        .plan(&taps(&[0x0100_0e02, 0x0100_0e01]))
        .expect("plans");
    assert_eq!(plan, Some(vec![0x0100_0e02, 0x0100_0e01]));
}

#[test]
fn slots_start_at_the_first_extra_keycode_and_use_the_wire_offset() {
    let mut extras = Extras::default();
    extras.replace(vec![0xe9, 0xea]);
    assert_eq!(extras.evdev_of(0xe9), Some(FIRST_EXTRA_XKB - WIRE_OFFSET));
    assert_eq!(
        extras.evdev_of(0xea),
        Some(FIRST_EXTRA_XKB + 1 - WIRE_OFFSET)
    );
    assert_eq!(extras.evdev_of(0xeb), None);
}

#[test]
fn every_slot_stays_within_the_declared_maximum() {
    let keysyms: Vec<u32> = (0..SLOT_COUNT as u32).map(|at| 0x0100_0e01 + at).collect();
    let mut extras = Extras::default();
    let plan = extras
        .plan(&taps(&keysyms))
        .expect("a full set fits")
        .expect("it is new");
    assert_eq!(plan.len(), 120);
    extras.replace(plan);
    let last = extras
        .evdev_of(keysyms[119])
        .expect("the last slot is assigned");
    assert_eq!(last + WIRE_OFFSET, LAST_XKB);
}

#[test]
fn slot_overflow_is_an_internal_error_with_nothing_delivered() {
    let keysyms: Vec<u32> = (0..=SLOT_COUNT as u32).map(|at| 0x0100_0e01 + at).collect();
    let error = Extras::default()
        .plan(&taps(&keysyms))
        .expect_err("121 distinct extras do not fit");
    assert_eq!(error.code, ErrorCode::Internal);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
}
