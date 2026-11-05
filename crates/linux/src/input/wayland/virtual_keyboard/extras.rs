//! The one-level keys that carry keysyms outside the base layout, one slot
//! per distinct keysym of the chunk being typed.

use agent_desktop_core::{AdapterError, DeliverySemantics};

use super::locate::locate_base;
use crate::input::keyboard::keysym_step::KeysymStep;

/// The first xkb keycode used for an extra key; the base layout ends below it.
pub(super) const FIRST_EXTRA_XKB: u32 = 136;
/// The highest xkb keycode the keymap declares and X clients accept.
pub(super) const LAST_XKB: u32 = 255;
/// The xkb keycode offset of the wire: a `key` request carries xkb minus 8.
pub(super) const WIRE_OFFSET: u32 = 8;
const SLOT_COUNT: usize = (LAST_XKB - FIRST_EXTRA_XKB + 1) as usize;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct Extras {
    slots: Vec<u32>,
}

impl Extras {
    pub(super) fn evdev_of(&self, keysym: u32) -> Option<u32> {
        let at = self.slots.iter().position(|slot| *slot == keysym)?;
        let xkb = FIRST_EXTRA_XKB + u32::try_from(at).ok()?;
        Some(xkb - WIRE_OFFSET)
    }

    /// The slot assignment a chunk needs, or `None` when every keysym of the
    /// chunk already has a slot so the keymap stays as uploaded.
    pub(super) fn plan(&self, keys: &[KeysymStep]) -> Result<Option<Vec<u32>>, AdapterError> {
        let mut needed: Vec<u32> = Vec::new();
        for key in keys {
            if locate_base(key.keysym).is_none() && !needed.contains(&key.keysym) {
                needed.push(key.keysym);
            }
        }
        if needed.iter().all(|keysym| self.slots.contains(keysym)) {
            return Ok(None);
        }
        if needed.len() > SLOT_COUNT {
            return Err(AdapterError::internal(format!(
                "A chunk needs {} distinct extra keys but the virtual keyboard has {SLOT_COUNT}",
                needed.len()
            ))
            .with_suggestion("Type the text in smaller parts")
            .with_disposition(DeliverySemantics::not_delivered()));
        }
        Ok(Some(needed))
    }

    pub(super) fn replace(&mut self, slots: Vec<u32>) {
        self.slots = slots;
    }
}

#[cfg(test)]
#[path = "extras_tests.rs"]
mod tests;
