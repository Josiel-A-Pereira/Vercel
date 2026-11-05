//! Where a keysym lives on the virtual keyboard.

use agent_desktop_core::{AdapterError, DeliverySemantics};

use super::base_key::BASE_KEYS;
use super::extras::{Extras, WIRE_OFFSET};

/// The evdev keycode that produces a keysym and whether it sits on the Shift
/// level of a base key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Located {
    pub(super) evdev: u32,
    pub(super) shift: bool,
}

/// The base-layout position of `keysym`, or `None` when it needs an extra key.
pub(super) fn locate_base(keysym: u32) -> Option<Located> {
    BASE_KEYS.iter().find_map(|key| {
        if key.plain == keysym {
            return Some(Located {
                evdev: key.evdev,
                shift: false,
            });
        }
        (key.shifted == Some(keysym)).then_some(Located {
            evdev: key.evdev,
            shift: true,
        })
    })
}

/// The position of `keysym` on the keymap as currently uploaded: the base
/// layout first, then the extra slots.
pub(super) fn locate(keysym: u32, extras: &Extras) -> Option<Located> {
    locate_base(keysym).or_else(|| {
        extras.evdev_of(keysym).map(|evdev| Located {
            evdev,
            shift: false,
        })
    })
}

pub(super) fn xkb_of(evdev: u32) -> u32 {
    evdev + WIRE_OFFSET
}

/// The error for a keysym the current keymap does not carry. Nothing has been
/// written when it is raised.
pub(super) fn missing_keysym(keysym: u32) -> AdapterError {
    AdapterError::internal(format!(
        "Keysym 0x{keysym:x} is not on the virtual keyboard's current keymap"
    ))
    .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "locate_tests.rs"]
mod tests;
