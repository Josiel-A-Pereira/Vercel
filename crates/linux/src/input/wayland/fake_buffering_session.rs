use agent_desktop_core::{AdapterError, Deadline};

use super::fake_session::FakeSession;
use super::key_session::KeySession;
use crate::input::keyboard::keysym_step::KeysymStep;

/// A session that behaves like the virtual keyboard's: it takes keys without
/// writing them and writes a group only once nothing is held. `written` is
/// what reached the wire.
pub(super) struct BufferingSession {
    inner: FakeSession,
    pending: Vec<(u32, bool)>,
    held: u32,
    pub(super) written: Vec<(u32, bool)>,
}

impl BufferingSession {
    pub(super) fn new() -> Self {
        Self {
            inner: FakeSession::new(),
            pending: Vec::new(),
            held: 0,
            written: Vec::new(),
        }
    }

    pub(super) fn events(&self) -> Vec<String> {
        self.inner.events()
    }

    pub(super) fn log(&self) -> std::rc::Rc<std::cell::RefCell<Vec<String>>> {
        self.inner.log()
    }
}

impl KeySession for BufferingSession {
    fn create(&mut self, deadline: Deadline) -> Result<(), AdapterError> {
        self.inner.create(deadline)
    }

    fn start(&mut self, deadline: Deadline) -> Result<(), AdapterError> {
        self.inner.start(deadline)
    }

    fn prepare(&mut self, keys: &[KeysymStep], _deadline: Deadline) -> Result<(), AdapterError> {
        self.inner
            .log()
            .borrow_mut()
            .push(format!("prepare:{}", keys.len()));
        Ok(())
    }

    fn keysym(&mut self, keysym: u32, down: bool, deadline: Deadline) -> Result<(), AdapterError> {
        if !down && self.held == 0 {
            return Ok(());
        }
        self.inner.keysym(keysym, down, deadline)?;
        self.held = if down {
            self.held + 1
        } else {
            self.held.saturating_sub(1)
        };
        self.pending.push((keysym, down));
        if self.held == 0 {
            self.written.append(&mut self.pending);
        }
        Ok(())
    }

    fn stop(&mut self, deadline: Deadline) -> Result<(), AdapterError> {
        self.inner.stop(deadline)
    }

    fn needs_lead(&self) -> bool {
        false
    }

    fn discard_unsent(&mut self) -> u32 {
        let dropped = u32::try_from(self.pending.len()).unwrap_or(u32::MAX);
        self.pending.clear();
        self.held = 0;
        dropped
    }
}
