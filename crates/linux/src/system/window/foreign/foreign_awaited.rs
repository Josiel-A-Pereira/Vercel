use super::foreign_flags::ToplevelFlags;

/// How waiting for a toplevel's state to change ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::system) enum Awaited {
    Reached,
    Gone,
    Unmet(ToplevelFlags),
}
