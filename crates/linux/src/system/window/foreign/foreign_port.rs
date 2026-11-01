use agent_desktop_core::Deadline;

use super::foreign_record::ToplevelRecord;
use super::foreign_request::ToplevelRequest;
use crate::system::x11::control::control_error::ControlError;

/// The seam every flow works through and tests replace.
pub(in crate::system) trait ToplevelPort {
    /// One bounded sync with the compositor. `Err(TimedOut)` at the deadline,
    /// `Err(NoDisplay)` on a broken connection.
    fn refresh(&self, deadline: Deadline) -> Result<(), ControlError>;

    /// Open toplevels whose first `done` arrived, in arrival order.
    fn records(&self) -> Vec<ToplevelRecord>;

    /// Sends and flushes. `Err(BadWindow)` for an unknown or closed id,
    /// `Err(NoDisplay)` when no `wl_seat` exists for `Activate`,
    /// `Err(SendFailed)` on a failed flush.
    fn send(&self, id: u32, request: ToplevelRequest) -> Result<(), ControlError>;
}
