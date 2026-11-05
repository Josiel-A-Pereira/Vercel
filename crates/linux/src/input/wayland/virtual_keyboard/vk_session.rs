//! The key session that delivers keys through the virtual keyboard.

use agent_desktop_core::{AdapterError, Deadline};

use super::extras::Extras;
use super::group_builder::plan_group;
use super::keymap_text::keymap_text;
use super::locate::{locate, missing_keysym};
use super::vk_port::VkPort;
use super::wire_op::WireOp;
use crate::input::keyboard::keysym_step::KeysymStep;
use crate::input::wayland::key_session::KeySession;

/// Delivers keys as balanced groups. `keysym` queues steps and writes nothing
/// until every held key is released, then writes the whole group in one flush,
/// so a killed process has only ever written complete groups and focus cannot
/// split a press from its release. `Drop` writes nothing.
pub(crate) struct VkSession<P: VkPort> {
    port: P,
    extras: Extras,
    pending: Vec<KeysymStep>,
    held: Vec<u32>,
}

impl<P: VkPort> VkSession<P> {
    pub(crate) fn new(port: P) -> Self {
        Self {
            port,
            extras: Extras::default(),
            pending: Vec::new(),
            held: Vec::new(),
        }
    }

    fn upload(&mut self, slots: &[u32], deadline: Deadline) -> Result<(), AdapterError> {
        self.port.upload_keymap(&keymap_text(slots))?;
        self.port.sync(deadline)
    }

    fn write_group(&mut self, deadline: Deadline) -> Result<(), AdapterError> {
        let steps = std::mem::take(&mut self.pending);
        let ops = plan_group(&steps, &self.extras)?;
        for op in ops {
            match op {
                WireOp::Key { evdev, down } => self.port.key(evdev, down)?,
                WireOp::Modifiers(mask) => self.port.modifiers(mask)?,
            }
        }
        self.port.sync(deadline)
    }
}

impl<P: VkPort> KeySession for VkSession<P> {
    fn create(&mut self, _deadline: Deadline) -> Result<(), AdapterError> {
        Ok(())
    }

    fn start(&mut self, deadline: Deadline) -> Result<(), AdapterError> {
        self.extras = Extras::default();
        self.upload(&[], deadline)?;
        self.port.settle(deadline)
    }

    fn prepare(&mut self, keys: &[KeysymStep], deadline: Deadline) -> Result<(), AdapterError> {
        let Some(slots) = self.extras.plan(keys)? else {
            return Ok(());
        };
        self.upload(&slots, deadline)?;
        self.extras.replace(slots);
        Ok(())
    }

    fn keysym(&mut self, keysym: u32, down: bool, deadline: Deadline) -> Result<(), AdapterError> {
        if down {
            locate(keysym, &self.extras).ok_or_else(|| missing_keysym(keysym))?;
            self.held.push(keysym);
        } else {
            let Some(at) = self.held.iter().rposition(|held| *held == keysym) else {
                return Ok(());
            };
            self.held.remove(at);
        }
        self.pending.push(KeysymStep { keysym, down });
        if self.held.is_empty() {
            return self.write_group(deadline);
        }
        Ok(())
    }

    fn stop(&mut self, deadline: Deadline) -> Result<(), AdapterError> {
        self.discard_unsent();
        self.port.modifiers(0)?;
        let synced = self.port.sync(deadline);
        let released = self.port.release(deadline);
        synced.and(released)
    }

    fn drain(&mut self, deadline: Deadline) {
        self.port.drain(deadline);
    }

    fn needs_lead(&self) -> bool {
        false
    }

    fn discard_unsent(&mut self) -> u32 {
        let dropped = u32::try_from(self.pending.len()).unwrap_or(u32::MAX);
        self.pending.clear();
        self.held.clear();
        dropped
    }
}

#[cfg(test)]
#[path = "vk_session_tests.rs"]
mod tests;
