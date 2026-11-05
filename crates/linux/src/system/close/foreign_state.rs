use std::cell::Cell;

use agent_desktop_core::AdapterError;

use crate::system::registry::registry_frames::RootRead;
use crate::system::window::foreign::foreign_close_route::ForeignCloseRoute;
use crate::system::window::foreign::foreign_link::ForeignLink;
use crate::system::window::window_ops::require_complete_frames;

/// The compositor route of one close and whether its last frame read left a
/// frame unanswered.
pub(super) struct ForeignState {
    pub(super) route: ForeignCloseRoute<ForeignLink>,
    pub(super) unanswered: Cell<bool>,
}

impl Default for ForeignState {
    fn default() -> Self {
        Self {
            route: ForeignCloseRoute::default(),
            unanswered: Cell::new(false),
        }
    }
}

impl ForeignState {
    /// Forgets the previous read so no early return can carry its flag over.
    pub(super) fn begin_read(&self) {
        self.unanswered.set(false);
    }

    pub(super) fn frames_unanswered(&self) -> Result<(), AdapterError> {
        if self.unanswered.get() {
            require_complete_frames(RootRead::TimedOut)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::close::world::answered_frames;

    #[test]
    fn a_new_read_starts_with_no_frame_unanswered() {
        let state = ForeignState::default();
        let timed_out = answered_frames(Vec::new(), RootRead::TimedOut, &state.unanswered);
        assert!(timed_out.is_err());
        assert!(state.frames_unanswered().is_err());
        state.begin_read();
        assert!(state.frames_unanswered().is_ok());
    }
}
