use agent_desktop_core::{AdapterError, Deadline};

/// A kept answer to "does the compositor still call this one window
/// activated?", pinned to a toplevel an earlier proof identified. It asks
/// about that toplevel only, so a window whose title changes while keys are
/// being sent stays the same window.
pub(crate) trait ActiveWatch {
    /// One sync with the compositor, then whether the pinned toplevel is
    /// still listed and activated. A toplevel that is gone is `false`; a
    /// connection that cannot sync is an error, never a yes.
    fn active(&self, deadline: Deadline) -> Result<bool, AdapterError>;
}
