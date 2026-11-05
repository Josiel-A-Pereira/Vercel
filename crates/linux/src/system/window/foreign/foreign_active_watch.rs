use agent_desktop_core::{AdapterError, Deadline};

use super::foreign_active::activated_id;
use super::foreign_await::flags_of;
use super::foreign_port::ToplevelPort;
use super::foreign_refusal::control_failure;
use super::foreign_target::ForeignTarget;
use crate::system::window::active_watch::ActiveWatch;

/// One kept compositor link and the id of the toplevel it proved activated.
pub(in crate::system) struct PinnedToplevel<P: ToplevelPort> {
    port: P,
    id: u32,
}

impl<P: ToplevelPort> PinnedToplevel<P> {
    /// Pins the toplevel the target stands for, or `None` when the list does
    /// not prove it to be that program's one window and activated right now.
    pub(in crate::system) fn pin(
        port: P,
        target: &ForeignTarget<'_>,
        deadline: Deadline,
    ) -> Result<Option<Self>, AdapterError> {
        Ok(activated_id(&port, target, deadline)?.map(|id| Self { port, id }))
    }
}

impl<P: ToplevelPort> ActiveWatch for PinnedToplevel<P> {
    fn active(&self, deadline: Deadline) -> Result<bool, AdapterError> {
        self.port
            .refresh(deadline)
            .map_err(|error| control_failure(error, deadline))?;
        Ok(flags_of(&self.port, self.id).is_some_and(|flags| flags.activated))
    }
}

#[cfg(test)]
#[path = "foreign_active_watch_tests.rs"]
mod tests;
