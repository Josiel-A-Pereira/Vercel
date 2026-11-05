use super::*;
use crate::input::keyboard::key_names::KEYSYM_RETURN;

#[test]
fn masks_follow_the_standard_xkb_order_of_real_modifiers() {
    assert_eq!(mask_of(KEYSYM_SHIFT_L), Some(0x1));
    assert_eq!(mask_of(KEYSYM_CONTROL_L), Some(0x4));
    assert_eq!(mask_of(KEYSYM_ALT_L), Some(0x8));
    assert_eq!(mask_of(KEYSYM_SUPER_L), Some(0x40));
}

#[test]
fn other_keys_carry_no_mask() {
    assert_eq!(mask_of(0x61), None);
    assert_eq!(mask_of(KEYSYM_RETURN), None);
}
