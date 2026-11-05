use super::*;
use crate::input::keyboard::key_names::{KEYSYM_CONTROL_L, KEYSYM_SHIFT_L};
use agent_desktop_core::{DeliverySemantics, ErrorCode};

const SHIFT_KEY: u32 = 42;
const CONTROL_KEY: u32 = 29;
const KEY_A: u32 = 30;
const KEY_SLASH: u32 = 53;

fn down(keysym: u32) -> KeysymStep {
    KeysymStep { keysym, down: true }
}

fn up(keysym: u32) -> KeysymStep {
    KeysymStep {
        keysym,
        down: false,
    }
}

fn tap(keysym: u32) -> Vec<KeysymStep> {
    vec![down(keysym), up(keysym)]
}

fn key(evdev: u32, down: bool) -> WireOp {
    WireOp::Key { evdev, down }
}

fn plan(steps: &[KeysymStep]) -> Vec<WireOp> {
    plan_group(steps, &Extras::default()).expect("the group plans")
}

#[test]
fn a_plain_key_clears_residue_first_and_sets_no_mask() {
    assert_eq!(
        plan(&tap(0x61)),
        [WireOp::Modifiers(0), key(KEY_A, true), key(KEY_A, false)]
    );
}

#[test]
fn an_upper_case_letter_wraps_shift_around_that_key_only() {
    assert_eq!(
        plan(&tap(0x41)),
        [
            WireOp::Modifiers(0),
            key(SHIFT_KEY, true),
            WireOp::Modifiers(0x1),
            key(KEY_A, true),
            key(KEY_A, false),
            key(SHIFT_KEY, false),
            WireOp::Modifiers(0),
        ]
    );
}

#[test]
fn a_shift_level_symbol_wraps_shift_and_a_later_plain_key_does_not() {
    let steps: Vec<KeysymStep> = [u32::from(b'?'), 0x61].into_iter().flat_map(tap).collect();
    assert_eq!(
        plan(&steps),
        [
            WireOp::Modifiers(0),
            key(SHIFT_KEY, true),
            WireOp::Modifiers(0x1),
            key(KEY_SLASH, true),
            key(KEY_SLASH, false),
            key(SHIFT_KEY, false),
            WireOp::Modifiers(0),
            key(KEY_A, true),
            key(KEY_A, false),
            WireOp::Modifiers(0),
        ]
    );
}

#[test]
fn a_chord_writes_its_masks_in_order_and_ends_with_a_clear() {
    let steps = [
        down(KEYSYM_CONTROL_L),
        down(KEYSYM_SHIFT_L),
        down(0x61),
        up(0x61),
        up(KEYSYM_SHIFT_L),
        up(KEYSYM_CONTROL_L),
    ];
    assert_eq!(
        plan(&steps),
        [
            WireOp::Modifiers(0),
            key(CONTROL_KEY, true),
            WireOp::Modifiers(0x4),
            key(SHIFT_KEY, true),
            WireOp::Modifiers(0x5),
            key(KEY_A, true),
            key(KEY_A, false),
            key(SHIFT_KEY, false),
            WireOp::Modifiers(0x4),
            key(CONTROL_KEY, false),
            WireOp::Modifiers(0),
        ]
    );
}

#[test]
fn a_held_shift_is_not_wrapped_again() {
    let steps = [
        down(KEYSYM_SHIFT_L),
        down(0x41),
        up(0x41),
        up(KEYSYM_SHIFT_L),
    ];
    let ops = plan(&steps);
    let shift_downs = ops.iter().filter(|op| **op == key(SHIFT_KEY, true)).count();
    assert_eq!(shift_downs, 1);
    assert_eq!(ops.last(), Some(&WireOp::Modifiers(0)));
}

#[test]
fn every_group_starts_with_modifiers_and_ends_clear_when_a_mask_was_set() {
    let groups = [
        tap(0x61),
        tap(0x41),
        vec![
            down(KEYSYM_CONTROL_L),
            down(0x61),
            up(0x61),
            up(KEYSYM_CONTROL_L),
        ],
    ];
    for steps in groups {
        let ops = plan(&steps);
        assert!(matches!(ops.first(), Some(WireOp::Modifiers(_))));
        let any_mask = ops
            .iter()
            .any(|op| matches!(op, WireOp::Modifiers(mask) if *mask != 0));
        assert_eq!(
            ops.last() == Some(&WireOp::Modifiers(0)),
            any_mask || ops.len() == 1
        );
        if any_mask {
            assert_eq!(ops.last(), Some(&WireOp::Modifiers(0)));
        }
    }
}

#[test]
fn extras_resolve_to_their_slot_keycode() {
    let mut extras = Extras::default();
    extras.replace(vec![0x0100_0e01]);
    let ops = plan_group(&tap(0x0100_0e01), &extras).expect("plans");
    assert_eq!(ops, [WireOp::Modifiers(0), key(128, true), key(128, false)]);
}

#[test]
fn a_keysym_missing_from_the_keymap_fails_the_whole_group() {
    let steps = [down(0x61), up(0x61), down(0x0100_0e01), up(0x0100_0e01)];
    let error = plan_group(&steps, &Extras::default()).expect_err("Thai has no slot yet");
    assert_eq!(error.code, ErrorCode::Internal);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
}
