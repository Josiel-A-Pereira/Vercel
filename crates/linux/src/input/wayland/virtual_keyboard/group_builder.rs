//! Turns the balanced steps of one group into the ordered wire requests.

use agent_desktop_core::AdapterError;

use super::extras::Extras;
use super::locate::{Located, locate, locate_base, missing_keysym};
use super::modifier_mask::{SHIFT, mask_of};
use super::wire_op::WireOp;
use crate::input::keyboard::key_names::KEYSYM_SHIFT_L;
use crate::input::keyboard::keysym_step::KeysymStep;

struct GroupBuilder {
    ops: Vec<WireOp>,
    mask: u32,
    any_mask: bool,
    wrapped: Vec<u32>,
    shift_evdev: u32,
}

impl GroupBuilder {
    fn new(shift_evdev: u32) -> Self {
        Self {
            ops: vec![WireOp::Modifiers(0)],
            mask: 0,
            any_mask: false,
            wrapped: Vec::new(),
            shift_evdev,
        }
    }

    fn key(&mut self, evdev: u32, down: bool) {
        self.ops.push(WireOp::Key { evdev, down });
    }

    fn set_mask(&mut self, mask: u32) {
        if mask == self.mask {
            return;
        }
        self.mask = mask;
        self.any_mask |= mask != 0;
        self.ops.push(WireOp::Modifiers(mask));
    }

    fn modifier(&mut self, located: Located, bit: u32, down: bool) {
        self.key(located.evdev, down);
        let mask = if down {
            self.mask | bit
        } else {
            self.mask & !bit
        };
        self.set_mask(mask);
    }

    fn press(&mut self, keysym: u32, located: Located) {
        if located.shift && self.mask & SHIFT == 0 {
            self.key(self.shift_evdev, true);
            self.set_mask(self.mask | SHIFT);
            self.wrapped.push(keysym);
        }
        self.key(located.evdev, true);
    }

    fn release(&mut self, keysym: u32, located: Located) {
        self.key(located.evdev, false);
        let Some(at) = self.wrapped.iter().rposition(|held| *held == keysym) else {
            return;
        };
        self.wrapped.remove(at);
        if self.wrapped.is_empty() {
            self.key(self.shift_evdev, false);
            self.set_mask(self.mask & !SHIFT);
        }
    }

    fn finish(mut self) -> Vec<WireOp> {
        self.set_mask(0);
        if self.any_mask && self.ops.last() != Some(&WireOp::Modifiers(0)) {
            self.ops.push(WireOp::Modifiers(0));
        }
        self.ops
    }
}

/// The requests that deliver `steps` as one group. The first is always a
/// `modifiers` request, so residue from another keyboard cannot alter the
/// text, and the last is `modifiers(0)` whenever any mask was non-zero. A
/// keysym missing from the keymap fails the whole group before anything is
/// written.
pub(super) fn plan_group(
    steps: &[KeysymStep],
    extras: &Extras,
) -> Result<Vec<WireOp>, AdapterError> {
    let shift = locate_base(KEYSYM_SHIFT_L).ok_or_else(|| missing_keysym(KEYSYM_SHIFT_L))?;
    let mut group = GroupBuilder::new(shift.evdev);
    for step in steps {
        let located = locate(step.keysym, extras).ok_or_else(|| missing_keysym(step.keysym))?;
        match (mask_of(step.keysym), step.down) {
            (Some(bit), down) => group.modifier(located, bit, down),
            (None, true) => group.press(step.keysym, located),
            (None, false) => group.release(step.keysym, located),
        }
    }
    Ok(group.finish())
}

#[cfg(test)]
#[path = "group_builder_tests.rs"]
mod tests;
