//! A recording `VkPort` for the session tests.

use std::cell::RefCell;
use std::rc::Rc;

use agent_desktop_core::{AdapterError, Deadline};

use super::vk_port::VkPort;

/// Logs every request as a short string and keeps each uploaded keymap text.
pub(super) struct FakeVk {
    log: Rc<RefCell<Vec<String>>>,
    keymaps: Rc<RefCell<Vec<String>>>,
    pub(super) fail_on: Option<&'static str>,
    pub(super) settles: bool,
}

impl FakeVk {
    pub(super) fn new() -> Self {
        Self {
            log: Rc::default(),
            keymaps: Rc::default(),
            fail_on: None,
            settles: false,
        }
    }

    pub(super) fn log(&self) -> Rc<RefCell<Vec<String>>> {
        Rc::clone(&self.log)
    }

    pub(super) fn keymaps(&self) -> Rc<RefCell<Vec<String>>> {
        Rc::clone(&self.keymaps)
    }

    fn record(&mut self, label: String) -> Result<(), AdapterError> {
        self.log.borrow_mut().push(label.clone());
        if self.fail_on == Some(label.as_str()) {
            return Err(AdapterError::internal("the fake failed this request"));
        }
        Ok(())
    }
}

impl VkPort for FakeVk {
    fn upload_keymap(&mut self, text: &str) -> Result<(), AdapterError> {
        self.keymaps.borrow_mut().push(text.to_owned());
        self.record("keymap".into())
    }

    fn key(&mut self, evdev: u32, down: bool) -> Result<(), AdapterError> {
        let edge = if down { "down" } else { "up" };
        self.record(format!("key:{evdev}:{edge}"))
    }

    fn modifiers(&mut self, mask: u32) -> Result<(), AdapterError> {
        self.record(format!("mods:{mask:x}"))
    }

    fn sync(&mut self, _deadline: Deadline) -> Result<(), AdapterError> {
        self.record("sync".into())
    }

    fn settle(&mut self, _deadline: Deadline) -> Result<(), AdapterError> {
        if self.settles {
            return self.record("settle".into());
        }
        Ok(())
    }

    fn drain(&mut self, _deadline: Deadline) {
        let _ = self.record("drain".into());
    }

    fn release(&mut self, _deadline: Deadline) -> Result<(), AdapterError> {
        self.record("release".into())
    }
}
