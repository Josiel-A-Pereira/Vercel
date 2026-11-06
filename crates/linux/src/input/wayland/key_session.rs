use agent_desktop_core::{AdapterError, Deadline};

use crate::input::keyboard::keysym_step::KeysymStep;

/// One compositor keyboard session. The calls are the steps of a delivery in
/// the order a caller makes them; `stop` ends whatever `create` made.
pub(crate) trait KeySession {
    fn create(&mut self, deadline: Deadline) -> Result<(), AdapterError>;
    fn start(&mut self, deadline: Deadline) -> Result<(), AdapterError>;
    fn keysym(&mut self, keysym: u32, down: bool, deadline: Deadline) -> Result<(), AdapterError>;
    fn stop(&mut self, deadline: Deadline) -> Result<(), AdapterError>;

    /// Whether the session was created and started by an earlier delivery and
    /// still holds its keyboard, so a delivery skips create, start and lead.
    fn resumed(&self) -> bool {
        false
    }

    /// Whether a delivery that succeeded leaves the session open for the next.
    fn retain(&self) -> bool {
        false
    }

    /// Called once per chunk with nothing held, before its first key. A
    /// session whose keymap depends on the chunk uploads it here.
    fn prepare(&mut self, _keys: &[KeysymStep], _deadline: Deadline) -> Result<(), AdapterError> {
        Ok(())
    }

    /// Whether a delivery sends its Shift lead and waits for it to settle
    /// before the first chunk.
    fn needs_lead(&self) -> bool {
        true
    }

    /// Called on a failed delivery before `stop`, once, so a session whose
    /// peer reads its writes asynchronously can let them land first. A
    /// finished delivery drains in the caller instead.
    fn drain(&mut self, _deadline: Deadline) {}

    /// Drops every step the session has accepted but not yet written and
    /// forgets held state, returning how many steps were dropped.
    fn discard_unsent(&mut self) -> u32 {
        0
    }
}
