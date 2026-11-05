//! The seam between the key session and the compositor connection. The real
//! link and the test fake implement it.

use agent_desktop_core::{AdapterError, Deadline};

pub(crate) trait VkPort {
    /// Queues a keymap upload. The text carries no trailing NUL; the port adds it.
    fn upload_keymap(&mut self, text: &str) -> Result<(), AdapterError>;

    /// Queues a `key` request. `evdev` is the wire keycode, xkb minus 8.
    fn key(&mut self, evdev: u32, down: bool) -> Result<(), AdapterError>;

    /// Queues a `modifiers` request that replaces the whole depressed mask and
    /// clears latched, locked and group.
    fn modifiers(&mut self, mask: u32) -> Result<(), AdapterError>;

    /// Writes everything queued and waits for one `wl_display.sync` reply.
    fn sync(&mut self, deadline: Deadline) -> Result<(), AdapterError>;

    /// Called once after the base keymap is uploaded and synced, before any
    /// key. A port whose keyboard is the first of its seat waits here until
    /// clients can receive keys; the default has nothing to wait for.
    fn settle(&mut self, _deadline: Deadline) -> Result<(), AdapterError> {
        Ok(())
    }

    /// Gives the compositor and its clients time to consume what was written,
    /// bounded by the deadline. Used before `release` when the delivery failed.
    fn drain(&mut self, deadline: Deadline);

    /// Destroys the virtual keyboard and waits for the compositor to see it.
    fn release(&mut self, deadline: Deadline) -> Result<(), AdapterError>;
}
