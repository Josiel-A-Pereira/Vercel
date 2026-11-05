//! The modifier masks the virtual keyboard sets, in the standard xkb order of
//! the real modifiers, and the base keys that carry them.

use crate::input::keyboard::key_names::{
    KEYSYM_ALT_L, KEYSYM_CONTROL_L, KEYSYM_SHIFT_L, KEYSYM_SUPER_L,
};

pub(super) const SHIFT: u32 = 0x1;
pub(super) const CONTROL: u32 = 0x4;
pub(super) const ALT: u32 = 0x8;
pub(super) const SUPER: u32 = 0x40;

/// Keysym, mask and the xkb real modifier name of each modifier key.
pub(super) const MODIFIER_KEYS: [(u32, u32, &str); 4] = [
    (KEYSYM_SHIFT_L, SHIFT, "Shift"),
    (KEYSYM_CONTROL_L, CONTROL, "Control"),
    (KEYSYM_ALT_L, ALT, "Mod1"),
    (KEYSYM_SUPER_L, SUPER, "Mod4"),
];

/// The mask bit a modifier keysym sets while held, or `None` for any other key.
pub(super) fn mask_of(keysym: u32) -> Option<u32> {
    MODIFIER_KEYS
        .iter()
        .find(|(known, _, _)| *known == keysym)
        .map(|(_, mask, _)| *mask)
}

#[cfg(test)]
#[path = "modifier_mask_tests.rs"]
mod tests;
