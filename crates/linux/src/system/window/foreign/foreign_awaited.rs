use super::foreign_flags::ToplevelFlags;
use crate::system::x11::control::control_error::ControlError;

/// How waiting for a toplevel's state to change ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::system) enum Awaited {
    Reached,
    Gone,
    Unmet(ToplevelFlags),
    /// A sync failed, so no state read since is evidence of anything.
    Unsynced(ControlError),
}
